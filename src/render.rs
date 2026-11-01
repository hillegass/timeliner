//! Draws a `Plan` with vello_cpu and encodes it as PNG. Everything is drawn
//! in points under a `pixels_per_point` scale.

use std::f64::consts::FRAC_PI_2;

use parley::{Layout, PositionedLayoutItem};
use vello_cpu::kurbo::{Affine, BezPath, Cap, Join, Point, Rect, RoundedRect, Shape, Stroke};
use vello_cpu::{Glyph, Pixmap, RenderContext, Resources};

use crate::layout::{EventGeom, Geometry};
use crate::plan::Plan;
use crate::style::{Color, Marker, Style};
use crate::text::{Ink, Measured};

/// The largest image vello_cpu can render in either direction.
const MAX_PIXELS: f64 = 65_535.0;

pub fn render(plan: &Plan, style: &Style) -> Result<Vec<u8>, String> {
    let ppp = style.pixels_per_point;
    let (w, h) = ((plan.width * ppp).round(), (plan.height * ppp).round());
    if w > MAX_PIXELS || h > MAX_PIXELS {
        return Err(format!(
            "the image would be {w}×{h} px, but the limit is {MAX_PIXELS} px in each direction; \
             lower pixels_per_point or make the timeline less crowded"
        ));
    }
    let (w, h) = (w as u16, h as u16);
    let mut d = Drawer {
        ctx: RenderContext::new(w, h),
        resources: Resources::new(),
        base: Affine::scale(ppp),
        style,
    };

    d.fill(
        &Rect::new(0.0, 0.0, plan.width, plan.height),
        style.background,
    );
    let g = &plan.geometry;
    d.timeline(g);
    for e in &g.events {
        d.range(g, e);
    }
    for e in &g.events {
        d.connector(e);
    }
    d.end_markers(g);
    for e in &g.events {
        d.marker(Point::new(e.marker_x, g.line_y), style.marker_color);
        if let Some(x) = e.stop_x {
            d.marker(Point::new(x, g.line_y), style.marker_color);
        }
    }
    for (e, text) in g.events.iter().zip(&plan.event_texts) {
        d.event_box(e, text);
    }
    if let (Some(r), Some(m)) = (g.start_label, &plan.start_label) {
        d.rotated_label(r, m);
    }
    if let (Some(r), Some(m)) = (g.stop_label, &plan.stop_label) {
        d.rotated_label(r, m);
    }

    let mut pixmap = Pixmap::new(w, h);
    d.ctx.flush();
    d.ctx.render(&mut pixmap, &mut d.resources);
    pixmap
        .into_png()
        .map_err(|e| format!("can't encode the PNG: {e}"))
}

struct Drawer<'a> {
    ctx: RenderContext,
    resources: Resources,
    /// Points to pixels.
    base: Affine,
    style: &'a Style,
}

impl Drawer<'_> {
    fn fill(&mut self, shape: &impl Shape, color: Color) {
        self.ctx.set_transform(self.base);
        self.ctx.set_paint(color);
        self.ctx.fill_path(&shape.to_path(0.01));
    }

    fn stroke(&mut self, path: &BezPath, stroke: Stroke, color: Color) {
        self.ctx.set_transform(self.base);
        self.ctx.set_paint(color);
        self.ctx.set_stroke(stroke);
        self.ctx.stroke_path(path);
    }

    /// The line, with an arrowhead at each open end.
    fn timeline(&mut self, g: &Geometry) {
        let s = self.style;
        let half = s.timeline_width / 2.0;
        let arrow = s.marker_size;
        let x0 = if g.open_start {
            g.line_x0 + arrow * 0.5
        } else {
            g.line_x0
        };
        let x1 = if g.open_stop {
            g.line_x1 - arrow * 0.5
        } else {
            g.line_x1
        };
        self.fill(
            &Rect::new(x0, g.line_y - half, x1, g.line_y + half),
            s.timeline_color,
        );
        for (open, tip, dir) in [
            (g.open_start, g.line_x0, 1.0),
            (g.open_stop, g.line_x1, -1.0),
        ] {
            if open {
                let mut p = BezPath::new();
                p.move_to((tip, g.line_y));
                p.line_to((tip + dir * arrow, g.line_y - arrow / 2.0));
                p.line_to((tip + dir * arrow, g.line_y + arrow / 2.0));
                p.close_path();
                self.fill(&p, s.timeline_color);
            }
        }
    }

    /// The stretch of line between a ranged event's start and stop.
    fn range(&mut self, g: &Geometry, e: &EventGeom) {
        if let Some(stop) = e.stop_x {
            let half = self.style.timeline_width / 2.0;
            let r = Rect::new(e.marker_x, g.line_y - half, stop, g.line_y + half);
            self.fill(&r, self.style.marker_color);
        }
    }

    fn end_markers(&mut self, g: &Geometry) {
        if !g.open_start {
            self.marker(Point::new(g.line_x0, g.line_y), self.style.timeline_color);
        }
        if !g.open_stop {
            self.marker(Point::new(g.line_x1, g.line_y), self.style.timeline_color);
        }
    }

    fn marker(&mut self, c: Point, color: Color) {
        let s = self.style;
        let r = s.marker_size / 2.0;
        match s.marker {
            Marker::None => {}
            Marker::Dot => self.fill(&vello_cpu::kurbo::Circle::new(c, r), color),
            Marker::Square => self.fill(&Rect::new(c.x - r, c.y - r, c.x + r, c.y + r), color),
            Marker::Diamond => {
                let mut p = BezPath::new();
                p.move_to((c.x, c.y - r));
                p.line_to((c.x + r, c.y));
                p.line_to((c.x, c.y + r));
                p.line_to((c.x - r, c.y));
                p.close_path();
                self.fill(&p, color);
            }
            Marker::Bar => {
                let half = s.timeline_width.max(1.0) / 2.0;
                self.fill(&Rect::new(c.x - half, c.y - r, c.x + half, c.y + r), color);
            }
        }
    }

    /// A connector with its bends rounded to `connector_corner`.
    fn connector(&mut self, e: &EventGeom) {
        let s = self.style;
        let path = rounded_polyline(&e.connector, s.connector_corner);
        let mut stroke = Stroke::new(s.connector_width)
            .with_caps(Cap::Butt)
            .with_join(Join::Round);
        if s.connector_dashed {
            let w = s.connector_width;
            stroke = stroke.with_dashes(0.0, [4.0 * w, 3.0 * w]);
        }
        self.stroke(&path, stroke, s.connector_color);
    }

    fn event_box(&mut self, e: &EventGeom, text: &Measured) {
        let s = self.style;
        let r = e.rect;
        let shape = RoundedRect::new(r.x0, r.y0, r.x1, r.y1, s.rounded_corner);
        self.fill(&shape, s.rect_fill_color);
        if s.rect_line_width > 0.0 {
            self.stroke(
                &shape.to_path(0.01),
                Stroke::new(s.rect_line_width),
                s.rect_line_color,
            );
        }
        let origin = Affine::translate((r.x0 + s.rect_margin, r.y0 + s.rect_margin));
        for p in &text.pieces {
            let at = origin * Affine::translate((f64::from(p.x), f64::from(p.y)));
            self.text(&p.layout, at);
        }
    }

    /// End-label text, rotated to read bottom to top inside `r`.
    fn rotated_label(&mut self, r: crate::layout::Rect, m: &Measured) {
        let at = Affine::translate((r.x0, r.y1)) * Affine::rotate(-FRAC_PI_2);
        for p in &m.pieces {
            self.text(
                &p.layout,
                at * Affine::translate((f64::from(p.x), f64::from(p.y))),
            );
        }
    }

    /// Draws a parley layout whose top-left corner is at `at` (in points).
    fn text(&mut self, layout: &Layout<Ink>, at: Affine) {
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let Ink(rgba) = run.style().brush;
                let font_run = run.run();
                let synthesis = font_run.synthesis();
                // Fonts without an italic (like Inconsolata) get a slant.
                let skew = synthesis.skew().map_or(Affine::IDENTITY, |deg| {
                    Affine::skew(f64::from(deg).to_radians().tan(), 0.0)
                });
                self.ctx.set_transform(self.base * at);
                self.ctx.set_paint(Color::new(rgba));
                let glyphs = run.positioned_glyphs().map(|g| Glyph {
                    id: g.id,
                    x: g.x,
                    y: g.y,
                });
                let coords: Vec<i16> = font_run
                    .normalized_coords()
                    .iter()
                    .map(|c| c.to_bits())
                    .collect();
                // A failed glyph run (e.g. a broken system font) is skipped
                // rather than failing the whole image.
                let _ = self
                    .ctx
                    .glyph_run(&mut self.resources, font_run.font())
                    .font_size(font_run.font_size())
                    .normalized_coords(&coords)
                    .glyph_transform(skew)
                    .hint(false)
                    .fill_glyphs(glyphs);
            }
        }
    }
}

/// An orthogonal polyline with each interior corner replaced by a quarter
/// circle of radius `r`, shrunk where segments are too short.
fn rounded_polyline(points: &[(f64, f64)], r: f64) -> BezPath {
    let mut p = BezPath::new();
    let Some(&first) = points.first() else {
        return p;
    };
    p.move_to(first);
    for i in 1..points.len() {
        let cur = Point::from(points[i]);
        if i + 1 == points.len() {
            p.line_to(cur);
            break;
        }
        let (prev, next) = (Point::from(points[i - 1]), Point::from(points[i + 1]));
        let (a, b) = (cur - prev, next - cur);
        let radius = r.min(a.hypot() / 2.0).min(b.hypot() / 2.0);
        if radius <= 0.0 || a.hypot() == 0.0 || b.hypot() == 0.0 {
            p.line_to(cur);
            continue;
        }
        let start = cur - a.normalize() * radius;
        let end = cur + b.normalize() * radius;
        p.line_to(start);
        p.quad_to(cur, end);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_corners_shrink_on_short_segments() {
        let path = rounded_polyline(&[(0.0, 0.0), (0.0, 4.0), (20.0, 4.0), (20.0, 30.0)], 5.0);
        let bounds = path.bounding_box();
        assert_eq!(
            (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
            (0.0, 0.0, 20.0, 30.0)
        );
        // Straight connectors have no curves.
        let straight = rounded_polyline(&[(3.0, 0.0), (3.0, 9.0)], 5.0);
        assert_eq!(straight.elements().len(), 2);
    }
}
