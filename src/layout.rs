//! Layout: pure geometry in points, no fonts or pixels. Given box sizes and
//! dates, places the line, markers, event boxes, and connectors.
//!
//! Each side of the line is laid out on its own, in "distance from the line"
//! coordinates (`d`, growing outward), and mapped to page `y` at the end.
//!
//! Boxes are placed right to left. A box hangs to the right of its marker
//! (the connector meets its left edge), so placing the rightmost box first
//! lets each earlier event's connector pass to the left of the boxes already
//! placed: crowded events stack outward like a staircase instead of jogging.
//! For each event, candidate positions are scored by how much they grow the
//! side, then by whether they need a jog, then by how far they move.

use crate::style::Place;

const NANOS_PER_DAY: f64 = 86_400e9;
/// Search weights, in points of extra height.
#[derive(Debug, Clone, Copy)]
struct Costs {
    /// Each jog costs this much...
    jog: f64,
    /// ...plus this much per point of sideways travel.
    per_point: f64,
    /// A box covering the marker of an event not yet placed on its side,
    /// which forces that event to jog or move out.
    cover: f64,
}

impl Costs {
    fn jog(&self, shift: f64) -> f64 {
        self.jog + self.per_point * shift
    }
}

/// How finished layouts are compared: boxes close to the line are worth
/// some jogging.
const JUDGE: Costs = Costs {
    jog: 4.0,
    per_point: 0.15,
    cover: 0.0,
};

/// The search is greedy, and small changes in its weights send it down
/// different paths, so it runs with several and the best result is kept.
/// What each point of a box's distance from the line adds to a finished
/// layout's score.
const DISTANCE_COST: f64 = 0.1;

const SEARCHES: [Costs; 3] = [
    Costs {
        jog: 12.0,
        per_point: 0.5,
        cover: 30.0,
    },
    Costs {
        jog: 12.0,
        per_point: 1.0,
        cover: 30.0,
    },
    Costs {
        jog: 8.0,
        per_point: 0.3,
        cover: 20.0,
    },
];

#[derive(Debug, Clone)]
pub struct Input {
    pub image_width: f64,
    pub left_margin: f64,
    pub right_margin: f64,
    pub top_margin: f64,
    pub bottom_margin: f64,
    pub place: Place,
    pub marker_size: f64,
    pub connector_width: f64,
    pub connector_corner: f64,
    pub rounded_corner: f64,
    pub rect_gap: f64,
    /// The timeline's ends; `None` is an open end.
    pub start: Option<i128>,
    pub stop: Option<i128>,
    /// Rotated end-label sizes as (thickness across the line, length along
    /// the label), for closed ends that have a label.
    pub start_label: Option<(f64, f64)>,
    pub stop_label: Option<(f64, f64)>,
    /// Events sorted by start.
    pub events: Vec<EventIn>,
}

#[derive(Debug, Clone)]
pub struct EventIn {
    /// Names the event in warnings, e.g. `events[3] "Shiloh"`.
    pub name: String,
    pub start: i128,
    /// Set when the event shows a range.
    pub stop: Option<i128>,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Above,
    Below,
}

#[derive(Debug, Clone)]
pub struct Geometry {
    pub height: f64,
    pub line_y: f64,
    pub line_x0: f64,
    pub line_x1: f64,
    pub open_start: bool,
    pub open_stop: bool,
    /// Rotated label rectangles at the closed ends.
    pub start_label: Option<Rect>,
    pub stop_label: Option<Rect>,
    pub events: Vec<EventGeom>,
    pub jogs: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EventGeom {
    pub marker_x: f64,
    pub stop_x: Option<f64>,
    // Read by tests; rendering works from the rectangles directly.
    #[allow(dead_code)]
    pub side: Side,
    pub rect: Rect,
    /// From the marker's edge to the box: 2 points if straight, 4 with a jog.
    pub connector: Vec<(f64, f64)>,
}

/// A box in side coordinates: `d0` is the edge nearer the line.
#[derive(Debug, Clone, Copy)]
struct SRect {
    x0: f64,
    x1: f64,
    d0: f64,
    d1: f64,
}

impl SRect {
    fn grow(self, by: f64) -> SRect {
        SRect {
            x0: self.x0 - by,
            x1: self.x1 + by,
            d0: self.d0 - by,
            d1: self.d1 + by,
        }
    }

    fn overlaps(&self, o: &SRect) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.d0 < o.d1 && o.d0 < self.d1
    }
}

type Pt = (f64, f64);

#[derive(Debug, Clone)]
struct Placed {
    index: usize,
    rect: SRect,
    path: Vec<Pt>,
}

/// Spacing derived from the style.
#[derive(Clone, Copy)]
struct Metrics {
    /// Nearest a box may come to the line.
    base: f64,
    /// Where connectors start (the marker's edge).
    stem: f64,
    /// Minimum distance between connectors, and between a connector and a box.
    clear: f64,
    lane_step: f64,
    gap: f64,
    corner: f64,
    attach: f64,
    left: f64,
    right: f64,
}

pub fn layout(input: &Input) -> Geometry {
    let left = input.left_margin;
    let right = input.image_width - input.right_margin;
    let (t0, t1) = time_range(input);
    let x_of = |t: i128| left + (t - t0) as f64 / (t1 - t0) as f64 * (right - left);

    let m = Metrics {
        stem: input.marker_size / 2.0,
        base: input.marker_size * 1.5 + input.rect_gap,
        clear: input.connector_width + 3.0,
        lane_step: input.connector_width * 2.0 + 3.0,
        gap: input.rect_gap,
        corner: input.connector_corner,
        attach: input.rounded_corner,
        left,
        right,
    };

    let sides: Vec<Side> = (0..input.events.len())
        .map(|i| match input.place {
            Place::Above => Side::Above,
            Place::Below => Side::Below,
            Place::Alternating if i % 2 == 0 => Side::Above,
            Place::Alternating => Side::Below,
        })
        .collect();

    // End labels stand above the line at closed ends and block boxes there.
    let label_rect = |x: f64, size: Option<(f64, f64)>| {
        size.map(|(thick, len)| SRect {
            x0: x - thick / 2.0,
            x1: x + thick / 2.0,
            d0: m.stem + 2.0,
            d1: m.stem + 2.0 + len,
        })
    };
    let start_label = input
        .start
        .and_then(|_| label_rect(left, input.start_label));
    let stop_label = input.stop.and_then(|_| label_rect(right, input.stop_label));

    let mut warnings = Vec::new();
    let mut jogs = 0;
    let mut placed_all: Vec<Option<(Side, Placed)>> = vec![None; input.events.len()];
    let mut extents = [m.stem, m.stem];
    for (si, side) in [Side::Above, Side::Below].into_iter().enumerate() {
        let obstacles: Vec<SRect> = if side == Side::Above {
            start_label
                .iter()
                .chain(stop_label.iter())
                .copied()
                .collect()
        } else {
            Vec::new()
        };
        let order: Vec<usize> = (0..input.events.len())
            .filter(|&i| sides[i] == side)
            .collect();
        let xs: Vec<f64> = input.events.iter().map(|e| x_of(e.start)).collect();
        // Search in both directions with each set of weights, add a staircase
        // that is always clean, and keep the best.
        let best = SEARCHES
            .iter()
            .flat_map(|costs| {
                [
                    place_side(order.iter().copied(), &xs, input, &obstacles, &m, costs),
                    place_side(
                        order.iter().rev().copied(),
                        &xs,
                        input,
                        &obstacles,
                        &m,
                        costs,
                    ),
                ]
            })
            .chain([staircase(&order, &xs, input, &obstacles, &m)])
            .collect::<Vec<_>>();
        // Local improvement is the slow part, so it only polishes the two
        // most promising layouts.
        let mut best = best;
        best.sort_by_key(SideLayout::score);
        best.truncate(2);
        let best = best
            .into_iter()
            .map(|l| improve(l, &xs, input, &obstacles, &m))
            .min_by_key(SideLayout::score)
            .expect("several layouts");
        extents[si] = extents[si].max(best.extent);
        jogs += best.jogs;
        warnings.extend(best.warnings);
        for p in best.placed {
            let i = p.index;
            placed_all[i] = Some((side, p));
        }
    }

    let line_y = input.top_margin + extents[0];
    let height = line_y + extents[1] + input.bottom_margin;
    let to_y = |side: Side, d: f64| match side {
        Side::Above => line_y - d,
        Side::Below => line_y + d,
    };
    let to_rect = |side: Side, r: SRect| {
        let (a, b) = (to_y(side, r.d0), to_y(side, r.d1));
        Rect {
            x0: r.x0,
            x1: r.x1,
            y0: a.min(b),
            y1: a.max(b),
        }
    };

    let events = input
        .events
        .iter()
        .zip(placed_all)
        .map(|(e, p)| {
            let (side, p) = p.expect("every event is placed");
            EventGeom {
                marker_x: x_of(e.start),
                stop_x: e.stop.filter(|&s| s > e.start).map(x_of),
                side,
                rect: to_rect(side, p.rect),
                connector: p.path.iter().map(|&(x, d)| (x, to_y(side, d))).collect(),
            }
        })
        .collect();

    Geometry {
        height,
        line_y,
        line_x0: left,
        line_x1: right,
        open_start: input.start.is_none(),
        open_stop: input.stop.is_none(),
        start_label: start_label.map(|r| to_rect(Side::Above, r)),
        stop_label: stop_label.map(|r| to_rect(Side::Above, r)),
        events,
        jogs,
        warnings,
    }
}

/// The instants at the line's two ends. Open ends extend 5% of the events'
/// span past the outermost event (one day if all events share one instant).
fn time_range(input: &Input) -> (i128, i128) {
    let times = input
        .events
        .iter()
        .flat_map(|e| std::iter::once(e.start).chain(e.stop));
    let lo = times.clone().min();
    let hi = times.max();
    let span = match (lo, hi) {
        (Some(lo), Some(hi)) if hi > lo => hi - lo,
        _ => NANOS_PER_DAY as i128,
    };
    let pad = (span / 20).max(1);
    let t0 = input.start.unwrap_or_else(|| lo.unwrap_or(0) - pad);
    let t1 = input.stop.unwrap_or_else(|| hi.unwrap_or(0) + pad);
    // Guard against a degenerate range (validation normally prevents it).
    if t1 > t0 {
        (t0, t1)
    } else {
        (t0, t0 + NANOS_PER_DAY as i128)
    }
}

/// One side's layout and how good it is.
struct SideLayout {
    placed: Vec<Placed>,
    warnings: Vec<String>,
    extent: f64,
    jogs: usize,
    shift: f64,
}

impl SideLayout {
    /// Lower is better: fewer problems, then less height (with jogs counted
    /// as height, and a little for every box's distance from the line, so a
    /// box moves closer even when the side's height doesn't change), then
    /// fewer jogs.
    fn score(&self) -> (usize, i64, usize) {
        let jogs = self.jogs as f64 * JUDGE.jog + JUDGE.per_point * self.shift;
        let distance: f64 = self.placed.iter().map(|p| p.rect.d0).sum();
        let cost = self.extent + jogs + DISTANCE_COST * distance;
        (self.warnings.len(), (cost * 100.0) as i64, self.jogs)
    }
}

/// Local search after the beam: take each box out in turn, put it back in
/// its best spot given all the others, and keep the change if the whole
/// side scores better. Repeats until nothing improves. This fixes choices
/// the greedy search couldn't see past, such as a box left far from the
/// line after its neighbors settled.
fn improve(
    mut layout: SideLayout,
    xs: &[f64],
    input: &Input,
    obstacles: &[SRect],
    m: &Metrics,
) -> SideLayout {
    if !layout.warnings.is_empty() {
        return layout;
    }
    // Pair moves are costly on crowded sides; cap them by count (not time,
    // so output is the same on every machine).
    let mut pair_budget = PAIR_BUDGET;
    for _ in 0..IMPROVE_PASSES {
        let mut changed = false;
        for k in 0..layout.placed.len() {
            let i = layout.placed[k].index;
            let others: Vec<Placed> = layout
                .placed
                .iter()
                .filter(|p| p.index != i)
                .cloned()
                .collect();
            let mut best: Option<(Vec<Placed>, (usize, i64, usize))> = None;
            for c in candidates(
                i,
                xs[i],
                &input.events[i],
                &others,
                obstacles,
                &[],
                m,
                &JUDGE,
            ) {
                if c.violations > 0 {
                    continue;
                }
                let mut placed = others.clone();
                placed.insert(k.min(placed.len()), c.placed);
                // Candidates are clean, so the score needs no full check.
                let score = clean_score(&placed, obstacles, m);
                if score < best.as_ref().map_or(layout.score(), |b| b.1) {
                    best = Some((placed, score));
                }
            }
            if let Some((placed, _)) = best {
                layout = summarize(placed, obstacles, &input.events, m);
                changed = true;
            }
        }
        // Pairs of nearby boxes: moving one may only work if a neighbor
        // moves too (e.g. its connector runs where the first box would go).
        for a in 0..layout.placed.len() {
            let ia = layout.placed[a].index;
            let e = &input.events[ia];
            // Where this box would ideally be: first row, straight.
            let x0 = (xs[ia] - m.attach).clamp(m.left, (m.right - e.width).max(m.left));
            let ideal = SRect {
                x0,
                x1: x0 + e.width,
                d0: m.base,
                d1: m.base + e.height,
            }
            .grow(m.gap);
            if (layout.placed[a].rect.d0 - m.base).abs() < 0.01 && layout.placed[a].path.len() == 2
            {
                continue;
            }
            for b in 0..layout.placed.len() {
                let ib = layout.placed[b].index;
                // Only neighbors in the way of that ideal spot can help.
                let pb = &layout.placed[b];
                if a == b || !(pb.rect.overlaps(&ideal) || path_hits_rect(&pb.path, &ideal)) {
                    continue;
                }
                if pair_budget == 0 {
                    continue;
                }
                pair_budget -= 1;
                if let Some(better) = move_pair(&layout, ia, ib, xs, input, obstacles, m) {
                    layout = better;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    layout
}

/// Takes boxes `a` and `b` out, puts `a` back in one of its cheapest clean
/// spots, then `b` in its best clean spot, and returns the result if the
/// side scores better than `layout`.
fn move_pair(
    layout: &SideLayout,
    a: usize,
    b: usize,
    xs: &[f64],
    input: &Input,
    obstacles: &[SRect],
    m: &Metrics,
) -> Option<SideLayout> {
    let others: Vec<Placed> = layout
        .placed
        .iter()
        .filter(|p| p.index != a && p.index != b)
        .cloned()
        .collect();
    let clean = |i: usize, placed: &[Placed]| {
        let mut c: Vec<Candidate> = candidates(
            i,
            xs[i],
            &input.events[i],
            placed,
            obstacles,
            &[],
            m,
            &JUDGE,
        )
        .into_iter()
        .filter(|c| c.violations == 0)
        .collect();
        c.sort_by(|x, y| (x.placed.rect.d1 + x.penalty).total_cmp(&(y.placed.rect.d1 + y.penalty)));
        c
    };
    let mut best: Option<(Vec<Placed>, (usize, i64, usize))> = None;
    for ca in clean(a, &others).into_iter().take(PAIR_TRIES) {
        let mut with_a = others.clone();
        with_a.push(ca.placed);
        for cb in clean(b, &with_a).into_iter().take(PAIR_TRIES) {
            let mut placed = with_a.clone();
            placed.push(cb.placed);
            let score = clean_score(&placed, obstacles, m);
            if score < best.as_ref().map_or(layout.score(), |b| b.1) {
                best = Some((placed, score));
            }
        }
    }
    best.map(|(placed, _)| summarize(placed, obstacles, &input.events, m))
}

/// The score of a layout known to be clean, without checking it.
fn clean_score(placed: &[Placed], obstacles: &[SRect], m: &Metrics) -> (usize, i64, usize) {
    let jogged = || placed.iter().filter(|p| p.path.len() > 2);
    SideLayout {
        placed: placed.to_vec(),
        warnings: Vec::new(),
        extent: placed
            .iter()
            .map(|p| p.rect.d1)
            .chain(obstacles.iter().map(|o| o.d1))
            .fold(m.stem, f64::max),
        jogs: jogged().count(),
        shift: jogged().map(|p| (p.path[2].0 - p.path[1].0).abs()).sum(),
    }
    .score()
}

const IMPROVE_PASSES: usize = 4;
/// Most pair moves tried per side.
const PAIR_BUDGET: usize = 150;
/// How many of each box's cheapest spots a pair move tries.
const PAIR_TRIES: usize = 4;

/// Builds a `SideLayout` (extent, jogs, warnings) from placed boxes.
fn summarize(
    placed: Vec<Placed>,
    obstacles: &[SRect],
    events: &[EventIn],
    m: &Metrics,
) -> SideLayout {
    let jogged = || placed.iter().filter(|p| p.path.len() > 2);
    let jogs = jogged().count();
    let shift = jogged().map(|p| (p.path[2].0 - p.path[1].0).abs()).sum();
    let extent = placed
        .iter()
        .map(|p| p.rect.d1)
        .chain(obstacles.iter().map(|o| o.d1))
        .fold(m.stem, f64::max);
    let warnings = check_side(&placed, obstacles, events, m);
    SideLayout {
        placed,
        warnings,
        extent,
        jogs,
        shift,
    }
}

fn place_side(
    order: impl Iterator<Item = usize>,
    xs: &[f64],
    input: &Input,
    obstacles: &[SRect],
    m: &Metrics,
    costs: &Costs,
) -> SideLayout {
    // Beam search: keep the best few partial layouts, extend each with every
    // candidate position for the next event, and keep the best results.
    let order: Vec<usize> = order.collect();
    let mut beam = vec![Partial {
        placed: Vec::new(),
        violations: 0,
        shift: 0.0,
        penalty: 0.0,
        extent: m.stem,
        jogs: 0,
    }];
    for (k, &i) in order.iter().enumerate() {
        let future: Vec<f64> = order[k + 1..].iter().map(|&j| xs[j]).collect();
        let mut next: Vec<Partial> = Vec::new();
        for state in &beam {
            let mut children = candidates(
                i,
                xs[i],
                &input.events[i],
                &state.placed,
                obstacles,
                &future,
                m,
                costs,
            );
            children.sort_by(|a, b| {
                child_key(state, a)
                    .partial_cmp(&child_key(state, b))
                    .unwrap()
            });
            for c in children.into_iter().take(PER_STATE) {
                let mut placed = state.placed.clone();
                let jog = c.placed.path.len() > 2;
                let shift = if jog {
                    (c.placed.path[2].0 - c.placed.path[1].0).abs()
                } else {
                    0.0
                };
                let penalty = c.penalty;
                let extent = state.extent.max(c.placed.rect.d1);
                placed.push(c.placed);
                next.push(Partial {
                    placed,
                    violations: state.violations + c.violations,
                    shift: state.shift + shift,
                    penalty: state.penalty + penalty,
                    extent,
                    jogs: state.jogs + usize::from(jog),
                });
            }
        }
        next.sort_by(|a, b| a.key().partial_cmp(&b.key()).unwrap());
        next.dedup_by(|a, b| a.same_as(b));
        next.truncate(BEAM_WIDTH);
        beam = next;
    }
    let best = beam.into_iter().next().expect("the beam is never empty");

    let extent = obstacles.iter().map(|o| o.d1).fold(best.extent, f64::max);
    let warnings = check_side(&best.placed, obstacles, &input.events, m);
    SideLayout {
        placed: best.placed,
        warnings,
        extent,
        jogs: best.jogs,
        shift: best.shift,
    }
}

/// A layout that is clean whenever cleanliness is possible, at the cost of
/// height. Right to left, each box hangs straight from its marker just
/// beyond the boxes beneath it. Boxes hang to the right of their markers, so
/// boxes placed earlier can't cover a later marker, except near the right
/// margin, where boxes shift left. An event whose marker is covered (or whose
/// box must shift) jogs left past the covering boxes, in a lane beneath every
/// jog it passes, so jogs nest instead of crossing.
fn staircase(
    order: &[usize],
    xs: &[f64],
    input: &Input,
    obstacles: &[SRect],
    m: &Metrics,
) -> SideLayout {
    let mut extra_lanes = 0;
    loop {
        let (layout, short) = staircase_with(order, xs, input, obstacles, m, extra_lanes);
        // Out of lanes: move every box out to make room, and try again.
        if short == 0 || extra_lanes > order.len() {
            return layout;
        }
        extra_lanes += short;
    }
}

fn staircase_with(
    order: &[usize],
    xs: &[f64],
    input: &Input,
    obstacles: &[SRect],
    m: &Metrics,
    extra_lanes: usize,
) -> (SideLayout, usize) {
    let m = Metrics {
        base: m.base + extra_lanes as f64 * m.lane_step,
        ..*m
    };
    let top_lane = m.base - m.corner.max(m.clear);
    let bottom_lane = m.stem + m.corner.max(m.clear);
    let mut placed: Vec<Placed> = Vec::new();
    let mut short = 0;
    for &i in order.iter().rev() {
        let e = &input.events[i];
        let mx = xs[i];
        let max_ax = (m.right - e.width).max(m.left) + m.attach;
        let blocks = |ax: f64, p: &Placed| p.rect.x0 - m.clear < ax && ax < p.rect.x1 + m.clear;
        let mut ax = (mx).min(max_ax);
        // Move the attach point left until its vertical clears every box.
        while let Some(p) = placed.iter().find(|p| blocks(ax, p)) {
            ax = p.rect.x0 - m.clear;
        }
        let x0 = (ax - m.attach).max(m.left);
        let ax = x0 + m.attach;
        let d0 = skyline(x0, x0 + e.width, e.height, &placed, obstacles, &m);
        let path = if (ax - mx).abs() < 0.01 {
            vec![(mx, m.stem), (mx, d0)]
        } else {
            // Beneath every jog whose horizontal run overlaps this one.
            let lane = placed
                .iter()
                .filter(|p| p.path.len() > 2)
                .filter(|p| {
                    let (a, b) = (p.path[1].0.min(p.path[2].0), p.path[1].0.max(p.path[2].0));
                    a <= mx.max(ax) + m.clear && ax.min(mx) - m.clear <= b
                })
                .map(|p| p.path[1].1 - m.lane_step)
                .fold(top_lane, f64::min);
            if lane < bottom_lane - 1e-9 {
                short += 1;
            }
            vec![(mx, m.stem), (mx, lane), (ax, lane), (ax, d0)]
        };
        placed.push(Placed {
            index: i,
            rect: SRect {
                x0,
                x1: x0 + e.width,
                d0,
                d1: d0 + e.height,
            },
            path,
        });
    }
    let jogs = placed.iter().filter(|p| p.path.len() > 2).count();
    let shift = placed
        .iter()
        .filter(|p| p.path.len() > 2)
        .map(|p| (p.path[2].0 - p.path[1].0).abs())
        .sum();
    let extent = placed
        .iter()
        .map(|p| p.rect.d1)
        .chain(obstacles.iter().map(|o| o.d1))
        .fold(m.stem, f64::max);
    let warnings = check_side(&placed, obstacles, &input.events, &m);
    (
        SideLayout {
            placed,
            warnings,
            extent,
            jogs,
            shift,
        },
        short,
    )
}

const BEAM_WIDTH: usize = 12;
const PER_STATE: usize = 6;
/// Most jog heights tried per position.
const MAX_LANES: usize = 6;

/// A partial layout of one side during the search.
#[derive(Clone)]
struct Partial {
    placed: Vec<Placed>,
    violations: usize,
    shift: f64,
    /// Jog and covering costs so far.
    penalty: f64,
    extent: f64,
    jogs: usize,
}

impl Partial {
    fn key(&self) -> (usize, f64, usize) {
        (self.violations, self.extent + self.penalty, self.jogs)
    }

    fn same_as(&self, o: &Partial) -> bool {
        self.placed.iter().zip(&o.placed).all(|(a, b)| {
            (a.rect.x0 - b.rect.x0).abs() < 0.01
                && (a.rect.d0 - b.rect.d0).abs() < 0.01
                && a.path.len() == b.path.len()
        })
    }
}

struct Candidate {
    placed: Placed,
    violations: usize,
    penalty: f64,
}

fn child_key(state: &Partial, c: &Candidate) -> (usize, f64, u8) {
    (
        c.violations,
        state.extent.max(c.placed.rect.d1) + state.penalty + c.penalty,
        u8::from(c.placed.path.len() > 2),
    )
}

/// Every position worth considering for one box, with its rule violations.
#[allow(clippy::too_many_arguments)]
fn candidates(
    index: usize,
    mx: f64,
    e: &EventIn,
    placed: &[Placed],
    obstacles: &[SRect],
    future: &[f64],
    m: &Metrics,
    costs: &Costs,
) -> Vec<Candidate> {
    let (w, h) = (e.width, e.height);
    let preferred = mx - m.attach;
    let max_x0 = (m.right - w).max(m.left);

    // Horizontal positions: hanging from the marker, beside nearby boxes,
    // with the connector threading the gap beside a box, and at the margins.
    let mut near: Vec<SRect> = placed
        .iter()
        .map(|p| p.rect)
        .chain(obstacles.iter().copied())
        .collect();
    near.sort_by(|a, b| (a.x0 - mx).abs().total_cmp(&(b.x0 - mx).abs()));
    near.truncate(NEARBY);
    let mut xs = vec![preferred, m.left, max_x0];
    for r in &near {
        xs.push(r.x1 + m.gap);
        xs.push(r.x0 - m.gap - w);
        xs.push(r.x1 + m.gap / 2.0 - m.attach);
        xs.push(r.x0 - m.gap / 2.0 - m.attach);
    }
    let mut xs: Vec<f64> = xs.into_iter().map(|x| x.clamp(m.left, max_x0)).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.01);

    // Every (position, distance) pair, cheapest first. A candidate's cost is
    // known before its violations are, so once enough clean candidates are
    // found the rest can't do better.
    let current_extent = placed.iter().map(|p| p.rect.d1).fold(0.0, f64::max);
    let mut spots = Vec::new();
    for &x0 in &xs {
        let straight = (x0 + m.attach - mx).abs() < 0.01;
        let lowest = skyline(x0, x0 + w, h, placed, obstacles, m);
        // Extra room nearer the line makes space for more jog lanes.
        for k in 0..if straight { 1 } else { 3 } {
            let d0 = lowest + k as f64 * m.lane_step;
            let shift = if straight {
                0.0
            } else {
                (x0 + m.attach - mx).abs()
            };
            let covered = future
                .iter()
                .filter(|&&x| x0 - m.clear < x && x < x0 + w + m.clear)
                .count();
            let penalty =
                if straight { 0.0 } else { costs.jog(shift) } + costs.cover * covered as f64;
            spots.push((
                current_extent.max(d0 + h) + penalty,
                u8::from(!straight),
                x0,
                d0,
                penalty,
            ));
        }
    }
    spots.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut out = Vec::new();
    let mut clean = 0;
    for (_, _, x0, d0, penalty) in spots {
        if clean >= 2 * PER_STATE {
            break;
        }
        let ax = x0 + m.attach;
        let rect = SRect {
            x0,
            x1: x0 + w,
            d0,
            d1: d0 + h,
        };
        let base = rect_violations(&rect, placed, obstacles, m);
        if base >= 1000 {
            continue;
        }
        let paths: Vec<Vec<Pt>> = if (ax - mx).abs() < 0.01 {
            vec![vec![(mx, m.stem), (mx, d0)]]
        } else {
            lanes(d0, placed, m)
                .into_iter()
                .map(|lane| vec![(mx, m.stem), (mx, lane), (ax, lane), (ax, d0)])
                .collect()
        };
        // Keep the cleanest lane for this spot (farthest first on ties).
        let mut best: Option<Candidate> = None;
        for path in paths {
            let violations = base + path_violations(&path, placed, obstacles, m);
            if best.as_ref().is_none_or(|b| violations < b.violations) {
                best = Some(Candidate {
                    placed: Placed { index, rect, path },
                    violations,
                    penalty,
                });
            }
            if violations == 0 {
                break;
            }
        }
        if let Some(c) = best {
            clean += usize::from(c.violations == 0);
            out.push(c);
        }
    }
    out
}

/// How many nearby boxes to consider positions beside.
const NEARBY: usize = 8;

/// The nearest distance from the line at which a box spanning `x0..x1`, `h`
/// tall, stays `rect_gap` clear of every placed box and obstacle. It may fit
/// into a gap between boxes.
fn skyline(x0: f64, x1: f64, h: f64, placed: &[Placed], obstacles: &[SRect], m: &Metrics) -> f64 {
    // Boxes and obstacles keep `rect_gap` away; connectors keep `clear` away.
    let mut blockers: Vec<(SRect, f64)> = placed
        .iter()
        .map(|p| (p.rect, m.gap))
        .chain(obstacles.iter().map(|&o| (o, m.gap)))
        .chain(placed.iter().flat_map(|p| {
            p.path.windows(2).map(|s| {
                let r = SRect {
                    x0: s[0].0.min(s[1].0),
                    x1: s[0].0.max(s[1].0),
                    d0: s[0].1.min(s[1].1),
                    d1: s[0].1.max(s[1].1),
                };
                (r, m.clear / 2.0)
            })
        }))
        .filter(|(r, pad)| r.x0 - pad < x1 && x0 < r.x1 + pad)
        .collect();
    blockers.sort_by(|a, b| a.0.d0.total_cmp(&b.0.d0));
    let mut d = m.base;
    // Blockers only push the box outward, so in order of distance one pass
    // finds the first gap that fits.
    for (r, pad) in &blockers {
        if r.d0 - pad < d + h && d < r.d1 + pad {
            d = r.d1 + pad;
        }
    }
    d
}

/// Heights at which a jog may run: lanes between the marker and the box, and
/// just beyond each placed box (to pass over lower boxes). Farthest first:
/// boxes are placed right to left, and a later event jogging left must pass
/// beneath the jogs already placed to its right.
fn lanes(d0: f64, placed: &[Placed], m: &Metrics) -> Vec<f64> {
    let margin = m.corner.max(m.clear);
    // The band next to the line, beneath the nearest row of boxes (a few
    // lanes deep at most: candidates may sit farther out to widen it).
    let top = d0.min(m.base + 3.0 * m.lane_step) - margin;
    let mut band = Vec::new();
    let mut lane = m.stem + margin;
    while lane <= top + 1e-9 {
        band.push(lane);
        lane += m.lane_step;
    }
    // Just beyond each box nearer the line, nearest to this box first.
    let mut beyond: Vec<f64> = placed
        .iter()
        .map(|p| p.rect.d1 + m.gap / 2.0)
        .filter(|&l| l <= d0 - m.corner && l > top)
        .collect();
    beyond.sort_by(|a, b| b.total_cmp(a));
    beyond.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    beyond.truncate(MAX_LANES);
    band.reverse();
    let mut out: Vec<f64> = beyond.into_iter().chain(band).collect();
    if out.is_empty() {
        out.push((m.stem + d0) / 2.0);
    }
    out
}

/// Violations by the box itself. Overlapping boxes are never acceptable, so
/// they count far more than anything else.
fn rect_violations(rect: &SRect, placed: &[Placed], obstacles: &[SRect], m: &Metrics) -> usize {
    let spaced = rect.grow(m.gap - 0.01);
    let mut n = 1000
        * placed
            .iter()
            .map(|p| &p.rect)
            .chain(obstacles)
            .filter(|r| spaced.overlaps(r))
            .count();
    if rect.x0 < m.left - 0.01 || rect.x1 > m.right + 0.01 {
        n += 1000;
    }
    for p in placed {
        if path_hits_rect(&p.path, &rect.grow(m.clear / 2.0)) {
            n += 1;
        }
    }
    n
}

/// Violations by a candidate connector.
fn path_violations(path: &[Pt], placed: &[Placed], obstacles: &[SRect], m: &Metrics) -> usize {
    let mut n = 0;
    for r in placed.iter().map(|p| &p.rect).chain(obstacles) {
        if path_hits_rect(path, &r.grow(m.clear / 2.0)) {
            n += 1;
        }
    }
    for p in placed {
        if paths_distance(path, &p.path) < m.clear {
            n += 1;
        }
    }
    n
}

fn path_hits_rect(path: &[Pt], r: &SRect) -> bool {
    path.windows(2).any(|s| {
        let (a, b) = (s[0], s[1]);
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (d0, d1) = (a.1.min(b.1), a.1.max(b.1));
        x0 < r.x1 && r.x0 < x1.max(x0 + 1e-9) && d0 < r.d1 && r.d0 < d1.max(d0 + 1e-9)
    })
}

fn paths_distance(a: &[Pt], b: &[Pt]) -> f64 {
    let mut best = f64::INFINITY;
    for s in a.windows(2) {
        for t in b.windows(2) {
            best = best.min(seg_distance(s[0], s[1], t[0], t[1]));
        }
    }
    best
}

fn seg_distance(p1: Pt, p2: Pt, q1: Pt, q2: Pt) -> f64 {
    if segments_cross(p1, p2, q1, q2) {
        return 0.0;
    }
    [
        point_seg(p1, q1, q2),
        point_seg(p2, q1, q2),
        point_seg(q1, p1, p2),
        point_seg(q2, p1, p2),
    ]
    .into_iter()
    .fold(f64::INFINITY, f64::min)
}

fn point_seg(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

fn segments_cross(p1: Pt, p2: Pt, q1: Pt, q2: Pt) -> bool {
    let cross = |o: Pt, a: Pt, b: Pt| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let (d1, d2) = (cross(q1, q2, p1), cross(q1, q2, p2));
    let (d3, d4) = (cross(p1, p2, q1), cross(p1, p2, q2));
    (d1 > 0.0) != (d2 > 0.0)
        && (d3 > 0.0) != (d4 > 0.0)
        && d1 != 0.0
        && d2 != 0.0
        && d3 != 0.0
        && d4 != 0.0
}

/// Checks one side's finished layout and describes every rule it breaks.
fn check_side(
    placed: &[Placed],
    obstacles: &[SRect],
    events: &[EventIn],
    m: &Metrics,
) -> Vec<String> {
    let name = |p: &Placed| events[p.index].name.clone();
    let mut out = Vec::new();
    for (i, a) in placed.iter().enumerate() {
        if a.rect.x0 < m.left - 0.01 || a.rect.x1 > m.right + 0.01 {
            out.push(format!(
                "the box for {} doesn't fit between the margins",
                name(a)
            ));
        }
        for o in obstacles {
            if a.rect.overlaps(o) {
                out.push(format!("the box for {} overlaps an end label", name(a)));
            }
        }
        for b in &placed[i + 1..] {
            if a.rect.overlaps(&b.rect) {
                out.push(format!("the boxes for {} and {} overlap", name(a), name(b)));
            }
            let dist = paths_distance(&a.path, &b.path);
            if dist < m.clear - 0.01 {
                let what = if dist == 0.0 { "cross" } else { "touch" };
                out.push(format!(
                    "the connectors for {} and {} {what}",
                    name(a),
                    name(b)
                ));
            }
        }
        for b in placed {
            if b.index != a.index && path_hits_rect(&a.path, &b.rect) {
                out.push(format!(
                    "the connector for {} crosses the box for {}",
                    name(a),
                    name(b)
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i128 = NANOS_PER_DAY as i128;

    fn input(events: &[(i128, f64, f64)], place: Place) -> Input {
        Input {
            image_width: 792.0,
            left_margin: 48.0,
            right_margin: 48.0,
            top_margin: 64.0,
            bottom_margin: 64.0,
            place,
            marker_size: 12.0,
            connector_width: 1.0,
            connector_corner: 5.0,
            rounded_corner: 5.0,
            rect_gap: 8.0,
            start: Some(0),
            stop: Some(1000 * DAY),
            start_label: None,
            stop_label: None,
            events: events
                .iter()
                .enumerate()
                .map(|(i, &(day, w, h))| EventIn {
                    name: format!("e{i}"),
                    start: day * DAY,
                    stop: None,
                    width: w,
                    height: h,
                })
                .collect(),
        }
    }

    /// Hard rules that must hold for every layout, warnings or not.
    fn assert_sound(g: &Geometry, inp: &Input) {
        for (i, a) in g.events.iter().enumerate() {
            assert!(
                a.rect.x0 >= inp.left_margin - 0.01
                    && a.rect.x1 <= inp.image_width - inp.right_margin + 0.01,
                "e{i} in margins"
            );
            match a.side {
                Side::Above => assert!(a.rect.y1 < g.line_y),
                Side::Below => assert!(a.rect.y0 > g.line_y),
            }
            for b in &g.events[i + 1..] {
                let overlap = a.rect.x0 < b.rect.x1
                    && b.rect.x0 < a.rect.x1
                    && a.rect.y0 < b.rect.y1
                    && b.rect.y0 < a.rect.y1;
                assert!(!overlap, "boxes overlap: {:?} {:?}", a.rect, b.rect);
            }
            let end = *a.connector.last().unwrap();
            assert!(
                (end.0 - (a.rect.x0 + inp.rounded_corner)).abs() < 0.01,
                "connector meets the left edge"
            );
            assert!(
                a.connector
                    .windows(2)
                    .all(|s| s[0].0 == s[1].0 || s[0].1 == s[1].1),
                "orthogonal"
            );
        }
        assert!(g.height > g.line_y);
    }

    #[test]
    fn sparse_events_hang_straight() {
        let inp = input(
            &[(100, 100.0, 40.0), (400, 100.0, 40.0), (700, 100.0, 40.0)],
            Place::Above,
        );
        let g = layout(&inp);
        assert_sound(&g, &inp);
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
        assert_eq!(g.jogs, 0);
        for e in &g.events {
            assert_eq!(e.connector.len(), 2);
            assert_eq!(e.connector[0].0, e.marker_x);
            assert!((e.rect.x0 + 5.0 - e.marker_x).abs() < 0.01);
        }
        // All in the first row: the same distance from the line.
        assert!(
            g.events
                .iter()
                .all(|e| (e.rect.y1 - g.events[0].rect.y1).abs() < 0.01)
        );
        assert!(
            (g.line_y - (64.0 + 26.0 + 40.0)).abs() < 0.01,
            "{}",
            g.line_y
        );
    }

    #[test]
    fn crowded_events_are_clean() {
        let inp = input(
            &[(500, 120.0, 40.0), (520, 120.0, 40.0), (540, 120.0, 40.0)],
            Place::Above,
        );
        let g = layout(&inp);
        assert_sound(&g, &inp);
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
    }

    #[test]
    fn right_edge_box_shifts_left_with_a_jog() {
        let inp = input(&[(990, 120.0, 40.0)], Place::Below);
        let g = layout(&inp);
        assert_sound(&g, &inp);
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
        assert_eq!(g.jogs, 1);
        let e = &g.events[0];
        assert_eq!(e.connector.len(), 4);
        assert!((e.rect.x1 - 744.0).abs() < 0.01);
    }

    #[test]
    fn overlapping_markers_warn_but_still_lay_out() {
        let inp = input(&[(500, 100.0, 40.0), (503, 100.0, 40.0)], Place::Above);
        let g = layout(&inp);
        assert_sound(&g, &inp);
        assert!(
            g.warnings
                .iter()
                .any(|w| w.contains("e0") && w.contains("e1")),
            "{:?}",
            g.warnings
        );
    }

    #[test]
    fn alternating_uses_both_sides() {
        let inp = input(
            &[(100, 100.0, 40.0), (200, 100.0, 60.0)],
            Place::Alternating,
        );
        let g = layout(&inp);
        assert_eq!(g.events[0].side, Side::Above);
        assert_eq!(g.events[1].side, Side::Below);
        assert!((g.height - (g.line_y + 26.0 + 60.0 + 64.0)).abs() < 0.01);
    }

    #[test]
    fn open_ends_extend_past_events() {
        let mut inp = input(&[(100, 50.0, 20.0), (300, 50.0, 20.0)], Place::Above);
        inp.start = None;
        inp.stop = None;
        let g = layout(&inp);
        assert!(g.open_start && g.open_stop);
        // 5% of the 200-day span on each side: events at 1/22 and 21/22.
        let span = g.line_x1 - g.line_x0;
        assert!((g.events[0].marker_x - (g.line_x0 + span / 22.0)).abs() < 0.01);
        assert!((g.events[1].marker_x - (g.line_x0 + span * 21.0 / 22.0)).abs() < 0.01);
        // A single event (zero span) pads by a day each way: it sits in the middle.
        let mut one = input(&[(100, 50.0, 20.0)], Place::Above);
        one.start = None;
        one.stop = None;
        let g = layout(&one);
        assert!((g.events[0].marker_x - 396.0).abs() < 0.01);
    }

    #[test]
    fn end_labels_block_boxes() {
        let mut inp = input(&[(0, 100.0, 40.0)], Place::Above);
        inp.start_label = Some((30.0, 120.0));
        let g = layout(&inp);
        assert_sound(&g, &inp);
        let label = g.start_label.unwrap();
        let r = g.events[0].rect;
        let overlap = r.x0 < label.x1 && label.x0 < r.x1 && r.y0 < label.y1 && label.y0 < r.y1;
        assert!(!overlap);
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
    }

    /// Civil War dates (days since 1861-04-12) with the measured box sizes.
    #[test]
    fn civil_war_is_clean() {
        let days = [
            359, 439, 503, 523, 608, 628, 748, 810, 889, 1119, 1122, 1145,
        ];
        let ev: Vec<_> = days.iter().map(|&d| (d, 132.0, 110.0)).collect();
        let mut inp = input(&ev, Place::Alternating);
        inp.stop = Some(1458 * DAY);
        inp.start_label = Some((34.0, 150.0));
        inp.stop_label = Some((34.0, 160.0));
        let g = layout(&inp);
        assert_sound(&g, &inp);
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
    }

    /// A long, dense timeline stays fast enough (run with --release to time).
    #[test]
    fn many_events() {
        let ev: Vec<_> = (0..60).map(|i| (i * 16, 110.0, 70.0)).collect();
        let inp = input(&ev, Place::Alternating);
        let started = std::time::Instant::now();
        let g = layout(&inp);
        eprintln!(
            "60 events: {:?}, {} warnings, height {:.0}",
            started.elapsed(),
            g.warnings.len(),
            g.height
        );
        assert_sound(&g, &inp);
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
    }

    /// Many random layouts: the hard rules always hold.
    #[test]
    fn random_layouts_are_sound() {
        let mut seed = 12345u64;
        let mut rand = move |n: u64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        for _ in 0..40 {
            let count = 1 + rand(12) as usize;
            let mut ev: Vec<_> = (0..count)
                .map(|_| {
                    (
                        rand(1000) as i128,
                        40.0 + rand(100) as f64,
                        20.0 + rand(100) as f64,
                    )
                })
                .collect();
            ev.sort_by_key(|e| e.0);
            let place = [Place::Above, Place::Below, Place::Alternating][rand(3) as usize];
            let inp = input(&ev, place);
            assert_sound(&layout(&inp), &inp);
        }
    }
}
