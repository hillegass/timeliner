//! Turns the loaded input into everything rendering needs: measured text and
//! the page geometry.

use parley::Layout;

use crate::datefmt::DateStyle;
use crate::desc::Desc;
use crate::diag::Diagnostics;
use crate::layout::{self, EventIn, Geometry};
use crate::richtext::{Block, BlockKind, RichText, Run, RunStyle};
use crate::style::{Align, Color, Style};
use crate::text::{Fonts, Ink, Measured, TextStyle};

/// Space between the header (title and subtitle) and the tallest box above
/// the line, and between the title and subtitle as a fraction of the
/// subtitle size.
const HEADER_GAP: f64 = 24.0;
const SUBTITLE_GAP: f64 = 0.25;

pub struct Plan {
    pub width: f64,
    pub height: f64,
    pub title: Option<Placed>,
    pub subtitle: Option<Placed>,
    /// One per event, in `desc.events` order.
    pub event_texts: Vec<Measured>,
    pub start_label: Option<Measured>,
    pub stop_label: Option<Measured>,
    pub geometry: Geometry,
}

/// A laid-out line of text and its top-left corner on the page.
pub struct Placed {
    pub layout: Layout<Ink>,
    pub x: f64,
    pub y: f64,
}

pub fn ink(c: Color) -> Ink {
    Ink(c.components)
}

pub fn plan(
    file: &str,
    desc: &Desc,
    style: &Style,
    texts: &[RichText],
    fonts: &mut Fonts,
    diags: &mut Diagnostics,
) -> Plan {
    let content_width = style.image_width - style.left_margin - style.right_margin;
    let base = |size: f64, bold: bool, color: Color| TextStyle {
        family: &style.default_font_family,
        mono_family: &style.monospace_font_family,
        size: size as f32,
        bold,
        ink: ink(color),
    };

    // Header: title, then subtitle, each wrapped to the content width.
    let mut y = style.top_margin;
    let mut header =
        |text: &Option<String>, size: f64, bold: bool, color: Color, align: Align, gap: f64| {
            let text = text.as_deref()?;
            let layout = fonts.plain(text, base(size, bold, color), Some(content_width as f32));
            let w = f64::from(layout.width());
            let x = match align {
                Align::Left => style.left_margin,
                Align::Center => style.left_margin + (content_width - w) / 2.0,
                Align::Right => style.image_width - style.right_margin - w,
            };
            y += gap;
            let placed = Placed { x, y, layout };
            y += f64::from(placed.layout.height());
            Some(placed)
        };
    let title = header(
        &desc.title,
        style.title_size,
        style.title_bold,
        style.title_color,
        style.title,
        0.0,
    );
    let subtitle_gap = if title.is_some() {
        SUBTITLE_GAP * style.subtitle_size
    } else {
        0.0
    };
    let subtitle = header(
        &desc.subtitle,
        style.subtitle_size,
        style.subtitle_bold,
        style.subtitle_color,
        style.subtitle,
        subtitle_gap,
    );
    let header_height = if title.is_some() || subtitle.is_some() {
        y - style.top_margin + HEADER_GAP
    } else {
        0.0
    };

    // Event text, wrapped to the box limit minus padding.
    let text_style = base(style.event_text_size, false, style.text_color);
    let max_text = (style.image_width / style.max_event_width - 2.0 * style.rect_margin) as f32;
    let event_texts: Vec<Measured> = desc
        .events
        .iter()
        .zip(texts)
        .map(|(event, rich)| {
            let m = fonts.rich(rich, text_style, max_text);
            if m.overflowed {
                diags.warning(file, &event.path(), format!(
                    "a word is wider than the box limit (image_width / max_event_width), so the box was widened to {:.0}pt",
                    f64::from(m.width) + 2.0 * style.rect_margin
                ));
            }
            m
        })
        .collect();

    // End labels: the label in bold (if any), then the date, on two lines.
    let dates = DateStyle::for_desc(desc);
    let mut end_label = |t: &Option<crate::timepoint::TimePoint>, label: &Option<String>| {
        let t = t.as_ref()?;
        let date = dates
            .format(t, None)
            .unwrap_or_else(|_| t.describe().to_string());
        let mut runs = Vec::new();
        if let Some(label) = label {
            runs.push(Run {
                text: format!("{label}\n"),
                style: RunStyle {
                    bold: true,
                    ..RunStyle::default()
                },
            });
        }
        runs.push(Run {
            text: date,
            style: RunStyle::default(),
        });
        let rich = RichText {
            blocks: vec![Block {
                kind: BlockKind::Paragraph,
                runs,
            }],
        };
        Some(fonts.rich(&rich, text_style, f32::MAX))
    };
    let start_label = end_label(&desc.start, &desc.start_label);
    let stop_label = end_label(&desc.stop, &desc.stop_label);
    let size = |m: &Option<Measured>| {
        m.as_ref()
            .map(|m| (f64::from(m.height), f64::from(m.width)))
    };

    let input = layout::Input {
        image_width: style.image_width,
        left_margin: style.left_margin,
        right_margin: style.right_margin,
        top_margin: style.top_margin,
        bottom_margin: style.bottom_margin,
        header_height,
        place: style.event_text_place,
        marker_size: style.marker_size,
        connector_width: style.connector_width,
        connector_corner: style.connector_corner,
        rounded_corner: style.rounded_corner,
        rect_gap: style.rect_gap,
        start: desc.start.as_ref().map(|t| t.instant),
        stop: desc.stop.as_ref().map(|t| t.instant),
        start_label: size(&start_label),
        stop_label: size(&stop_label),
        events: desc
            .events
            .iter()
            .zip(&event_texts)
            .map(|(e, m)| EventIn {
                name: e.label.clone(),
                start: e.start.instant,
                stop: e.stop.as_ref().filter(|_| e.shows_stop).map(|t| t.instant),
                width: f64::from(m.width) + 2.0 * style.rect_margin,
                height: f64::from(m.height) + 2.0 * style.rect_margin,
            })
            .collect(),
    };
    let geometry = layout::layout(&input);
    for w in &geometry.warnings {
        diags.warning(file, "", w);
    }

    Plan {
        width: style.image_width,
        height: geometry.height,
        title,
        subtitle,
        event_texts,
        start_label,
        stop_label,
        geometry,
    }
}
