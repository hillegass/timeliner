//! Text measurement with parley, using the embedded fonts (plus any installed
//! system fonts). All sizes are in points.

use std::sync::Arc;

use parley::fontique::{Blob, Collection, CollectionOptions};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStyle, FontWeight, Layout,
    LayoutContext, StyleProperty,
};

use crate::richtext::{BlockKind, ListMarker, RichText};
use crate::style::Style;

/// The fonts built into the binary. Registered fonts take precedence over
/// system fonts with the same family name.
const EMBEDDED_FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/OpenSans-Regular.ttf"),
    include_bytes!("../assets/fonts/OpenSans-Bold.ttf"),
    include_bytes!("../assets/fonts/OpenSans-Italic.ttf"),
    include_bytes!("../assets/fonts/OpenSans-BoldItalic.ttf"),
    include_bytes!("../assets/fonts/Inconsolata-Regular.ttf"),
    include_bytes!("../assets/fonts/Inconsolata-Bold.ttf"),
];

/// The families those files provide.
const EMBEDDED_FAMILIES: &[&str] = &["Open Sans", "Inconsolata"];

/// The license texts the OFL requires to travel with the fonts.
pub const FONT_LICENSES: &str = concat!(
    "Open Sans\n=========\n\n",
    include_str!("../assets/fonts/OpenSans-OFL.txt"),
    "\n\nInconsolata\n===========\n\n",
    include_str!("../assets/fonts/Inconsolata-OFL.txt"),
);

/// Text color, carried through layout so rendering knows how to draw each run.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Ink(pub [f32; 4]);

/// Vertical space between blocks, as a multiple of the text size.
const PARAGRAPH_GAP: f32 = 0.5;
const ITEM_GAP: f32 = 0.15;
/// Indentation per list level, and the minimum width of the marker column.
const LIST_INDENT: f32 = 1.25;

pub struct Fonts {
    font_cx: FontContext,
    layout_cx: LayoutContext<Ink>,
    system: bool,
}

/// One laid-out piece of text, positioned relative to its text block's top-left.
// Fields are read by the rendering milestone.
#[allow(dead_code)]
pub struct Placed {
    pub layout: Layout<Ink>,
    pub x: f32,
    pub y: f32,
}

/// Event text, laid out and wrapped.
#[allow(dead_code)]
pub struct Measured {
    pub pieces: Vec<Placed>,
    pub width: f32,
    pub height: f32,
    /// True when one word was wider than the wrap width, so the text is wider
    /// than the limit.
    pub overflowed: bool,
}

/// The font settings for one kind of text.
#[derive(Debug, Clone, Copy)]
pub struct TextStyle<'a> {
    pub family: &'a str,
    pub mono_family: &'a str,
    pub size: f32,
    pub bold: bool,
    pub ink: Ink,
}

impl Fonts {
    /// Loads the embedded fonts and checks that the style's families exist.
    /// System fonts are only scanned (which takes a noticeable fraction of a
    /// second) when the style names a family that isn't embedded.
    pub fn new(style: &Style) -> Result<Fonts, Vec<String>> {
        let embedded = [&style.default_font_family, &style.monospace_font_family]
            .iter()
            .all(|f| EMBEDDED_FAMILIES.contains(&f.as_str()));
        Fonts::with_system(style, !embedded)
    }

    /// Like `new`, choosing whether to include system fonts. They're also
    /// needed when the text has characters the embedded fonts lack.
    pub fn with_system(style: &Style, system: bool) -> Result<Fonts, Vec<String>> {
        let mut font_cx = FontContext {
            collection: Collection::new(CollectionOptions {
                shared: false,
                system_fonts: system,
            }),
            ..FontContext::default()
        };
        for data in EMBEDDED_FONTS {
            font_cx
                .collection
                .register_fonts(Blob::new(Arc::new(*data)), None);
        }
        let mut errors = Vec::new();
        for (key, family) in [
            ("default_font_family", &style.default_font_family),
            ("monospace_font_family", &style.monospace_font_family),
        ] {
            if font_cx.collection.family_id(family).is_none() {
                errors.push(format!(
                    "{key}: no font family named \"{family}\"{}",
                    suggest(&mut font_cx, family)
                ));
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(Fonts {
            font_cx,
            layout_cx: LayoutContext::new(),
            system,
        })
    }

    pub fn uses_system(&self) -> bool {
        self.system
    }

    /// Lays out one piece of styled text. Each `(range, bold, italic, code)`
    /// span overrides the base style.
    fn layout(
        &mut self,
        text: &str,
        style: TextStyle,
        spans: &[(std::ops::Range<usize>, bool, bool, bool)],
    ) -> Layout<Ink> {
        let mut b = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, false);
        b.push_default(StyleProperty::FontFamily(FontFamily::named(style.family)));
        b.push_default(StyleProperty::FontSize(style.size));
        b.push_default(StyleProperty::Brush(style.ink));
        if style.bold {
            b.push_default(StyleProperty::FontWeight(FontWeight::BOLD));
        }
        for (range, bold, italic, code) in spans {
            if *bold {
                b.push(StyleProperty::FontWeight(FontWeight::BOLD), range.clone());
            }
            if *italic {
                b.push(StyleProperty::FontStyle(FontStyle::Italic), range.clone());
            }
            if *code {
                b.push(
                    StyleProperty::FontFamily(FontFamily::named(style.mono_family)),
                    range.clone(),
                );
            }
        }
        b.build(text)
    }

    /// Lays out plain text (such as list markers), wrapped at `max_width` if given.
    pub fn plain(&mut self, text: &str, style: TextStyle, max_width: Option<f32>) -> Layout<Ink> {
        let mut layout = self.layout(text, style, &[]);
        layout.break_all_lines(max_width);
        layout.align(Alignment::Start, AlignmentOptions::default());
        layout
    }

    /// Lays out event text wrapped at `max_width`. If a single word is wider
    /// than that, the text is widened just enough to fit it.
    pub fn rich(&mut self, text: &RichText, style: TextStyle, max_width: f32) -> Measured {
        let em = style.size;
        let mut blocks = Vec::new();
        for block in &text.blocks {
            let mut s = String::new();
            let mut spans = Vec::new();
            for run in &block.runs {
                let start = s.len();
                s.push_str(&run.text);
                spans.push((
                    start..s.len(),
                    run.style.bold,
                    run.style.italic,
                    run.style.code,
                ));
            }
            let layout = self.layout(&s, style, &spans);
            let (indent, marker) = match block.kind {
                BlockKind::Paragraph => (0.0, None),
                BlockKind::Item { depth, marker } => {
                    let marker = marker.map(|m| {
                        let label = match m {
                            ListMarker::Bullet => "•".to_string(),
                            ListMarker::Number(n) => format!("{n}."),
                        };
                        self.plain(&label, style, None)
                    });
                    let column = marker
                        .as_ref()
                        .map_or(0.0, |m| m.width() + 0.35 * em)
                        .max(LIST_INDENT * em);
                    (
                        depth as f32 * LIST_INDENT * em + column,
                        marker.map(|m| (m, depth)),
                    )
                }
            };
            blocks.push((block.kind, layout, indent, marker));
        }

        // The narrowest the text can be: its longest word (plus indents).
        let needed = blocks
            .iter()
            .map(|(_, l, indent, _)| indent + l.calculate_content_widths().min)
            .fold(0.0, f32::max);
        let overflowed = needed > max_width + 0.01;
        let wrap = max_width.max(needed);

        let mut pieces = Vec::new();
        let (mut y, mut width) = (0.0f32, 0.0f32);
        let mut prev: Option<BlockKind> = None;
        for (kind, mut layout, indent, marker) in blocks {
            if let Some(p) = prev {
                let both_items =
                    matches!(p, BlockKind::Item { .. }) && matches!(kind, BlockKind::Item { .. });
                y += em * if both_items { ITEM_GAP } else { PARAGRAPH_GAP };
            }
            prev = Some(kind);
            layout.break_all_lines(Some(wrap - indent));
            layout.align(Alignment::Start, AlignmentOptions::default());
            if let Some((m, depth)) = marker {
                // Align the marker's baseline with the item's first line.
                let shift =
                    layout.first_baseline().unwrap_or(0.0) - m.first_baseline().unwrap_or(0.0);
                pieces.push(Placed {
                    layout: m,
                    x: depth as f32 * LIST_INDENT * em,
                    y: y + shift,
                });
            }
            width = width.max(indent + layout.width());
            let h = layout.height();
            pieces.push(Placed {
                layout,
                x: indent,
                y,
            });
            y += h;
        }
        Measured {
            pieces,
            width,
            height: y,
            overflowed,
        }
    }
}

/// True if any character in `layout` has no glyph in the fonts available,
/// so it would be drawn as an empty box.
pub fn missing_glyphs(layout: &Layout<Ink>) -> bool {
    layout.lines().any(|line| {
        line.items().any(|item| match item {
            parley::PositionedLayoutItem::GlyphRun(run) => run.glyphs().any(|g| g.id == 0),
            _ => false,
        })
    })
}

/// A " (did you mean ...)" hint from the available family names.
fn suggest(font_cx: &mut FontContext, family: &str) -> String {
    let lower = family.to_lowercase();
    let names: Vec<String> = font_cx
        .collection
        .family_names()
        .map(str::to_string)
        .collect();
    let mut close: Vec<&String> = names
        .iter()
        .filter(|n| {
            let n = n.to_lowercase();
            n.contains(&lower)
                || lower.contains(&n)
                || n.split(' ').next() == lower.split(' ').next()
        })
        .collect();
    close.sort();
    close.dedup();
    match close.len() {
        0 => String::new(),
        _ => format!(
            " (did you mean {}?)",
            close
                .iter()
                .take(3)
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Diagnostics;
    use crate::richtext::Template;

    fn fonts() -> Fonts {
        let mut d = Diagnostics::default();
        Fonts::new(&crate::style::load(None, &mut d).unwrap()).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn style(size: f32) -> TextStyle<'static> {
        TextStyle {
            family: "Open Sans",
            mono_family: "Inconsolata",
            size,
            bold: false,
            ink: Ink::default(),
        }
    }

    fn rich(t: &str) -> RichText {
        Template::parse(t)
            .unwrap()
            .render(|_, _| Ok(String::new()))
            .unwrap()
    }

    #[test]
    fn bold_is_wider_and_size_scales() {
        let mut f = fonts();
        let regular = f.plain("Gettysburg", style(12.0), None).width();
        let bold = f
            .plain(
                "Gettysburg",
                TextStyle {
                    bold: true,
                    ..style(12.0)
                },
                None,
            )
            .width();
        let big = f.plain("Gettysburg", style(24.0), None).width();
        assert!(bold > regular, "{bold} > {regular}");
        assert!((big - 2.0 * regular).abs() < 0.5, "{big} vs {regular}");
        // Open Sans at 12pt: "Gettysburg" is roughly 60pt wide.
        assert!((50.0..70.0).contains(&regular), "{regular}");
    }

    #[test]
    fn wraps_to_width() {
        let mut f = fonts();
        let m = f.rich(
            &rich("Spotsylvania Court House: May 8, 1864"),
            style(12.0),
            112.0,
        );
        assert!(!m.overflowed);
        assert!(m.width <= 112.0, "{}", m.width);
        assert!(
            m.pieces[0].layout.len() >= 2,
            "should wrap onto several lines"
        );
    }

    #[test]
    fn long_words_widen_the_text() {
        let mut f = fonts();
        let m = f.rich(
            &rich("Supercalifragilisticexpialidocious"),
            style(12.0),
            60.0,
        );
        assert!(m.overflowed);
        assert!(m.width > 60.0);
        assert_eq!(m.pieces[0].layout.len(), 1, "the word stays on one line");
    }

    #[test]
    fn lists_are_indented_with_markers() {
        let mut f = fonts();
        let m = f.rich(
            &rich("**Shiloh**\n\n- North: 1,754\n- South: 1,728"),
            style(12.0),
            112.0,
        );
        // Title, then (marker, text) for each item.
        assert_eq!(m.pieces.len(), 5);
        assert_eq!(m.pieces[0].x, 0.0);
        assert_eq!(m.pieces[1].x, 0.0, "bullet sits at the left edge");
        assert!(m.pieces[2].x >= 15.0, "item text is indented");
        assert!(m.pieces[3].y > m.pieces[1].y);
        assert!(m.height > 3.0 * 12.0);
    }

    #[test]
    fn code_uses_the_monospace_font() {
        let mut f = fonts();
        // In Inconsolata every glyph has the same advance.
        let narrow = f.rich(&rich("`iiii`"), style(12.0), 500.0).width;
        let wide = f.rich(&rich("`WWWW`"), style(12.0), 500.0).width;
        assert!((narrow - wide).abs() < 0.01, "{narrow} vs {wide}");
    }

    #[test]
    fn unknown_family_is_an_error() {
        let mut d = Diagnostics::default();
        let mut s = crate::style::load(None, &mut d).unwrap();
        s.default_font_family = "Open Sanz".into();
        let errs = Fonts::new(&s).err().unwrap();
        assert!(
            errs[0].starts_with("default_font_family: no font family named \"Open Sanz\""),
            "{errs:?}"
        );
        assert!(errs[0].contains("\"Open Sans\""), "{errs:?}");
    }

    #[test]
    fn system_fonts_only_when_needed() {
        let f = fonts();
        assert!(
            !f.uses_system(),
            "the default style needs only embedded fonts"
        );
        let mut d = Diagnostics::default();
        let mut s = crate::style::load(None, &mut d).unwrap();
        s.monospace_font_family = "No Such Mono".into();
        // An unknown family is looked for among system fonts too.
        assert!(Fonts::new(&s).is_err());
    }

    #[test]
    fn detects_missing_glyphs() {
        let mut f = fonts();
        assert!(!missing_glyphs(&f.plain(
            "Gettysburg • 1,754",
            style(12.0),
            None
        )));
        assert!(missing_glyphs(&f.plain("東京", style(12.0), None)));
    }

    #[test]
    fn licenses_are_included() {
        assert!(FONT_LICENSES.contains("SIL OPEN FONT LICENSE"));
        assert!(FONT_LICENSES.contains("Inconsolata"));
    }
}
