//! Event text: the `event_text` template is parsed as markdown once, then
//! fields are substituted into its text runs for each event. Parsing first
//! means a value containing `*` or `_` can't change the formatting.

use pulldown_cmark::{Event as Md, Parser, Tag, TagEnd};

use crate::template::{Piece, parse_pieces};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMarker {
    Bullet,
    Number(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    /// A list item at `depth` (0 for a top-level list). `marker` is `None` for
    /// a second paragraph inside the same item, which has no bullet.
    Item {
        depth: usize,
        marker: Option<ListMarker>,
    },
}

/// Formatted text ready for layout. A run's text may contain '\n' for a
/// hard line break.
#[derive(Debug, Clone, PartialEq)]
pub struct RichText {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    pub runs: Vec<Run>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub style: RunStyle,
}

/// A parsed `event_text`: the markdown structure, with each run's text split
/// into literals and fields.
#[derive(Debug, Clone)]
pub struct Template {
    blocks: Vec<TemplateBlock>,
}

#[derive(Debug, Clone)]
struct TemplateBlock {
    kind: BlockKind,
    runs: Vec<(Vec<Piece>, RunStyle)>,
}

impl Template {
    pub fn parse(text: &str) -> Result<Template, String> {
        let blocks = parse_markdown(text);
        let mut out = Vec::with_capacity(blocks.len());
        for b in blocks {
            let mut runs = Vec::with_capacity(b.runs.len());
            for r in b.runs {
                runs.push((parse_pieces(&r.text)?, r.style));
            }
            out.push(TemplateBlock { kind: b.kind, runs });
        }
        Ok(Template { blocks: out })
    }

    /// Substitutes fields. `lookup(name, spec)` returns a field's formatted
    /// text. Every failing field is reported, not just the first.
    pub fn render(
        &self,
        mut lookup: impl FnMut(&str, Option<&str>) -> Result<String, String>,
    ) -> Result<RichText, Vec<String>> {
        let mut errors = Vec::new();
        let mut blocks = Vec::with_capacity(self.blocks.len());
        for b in &self.blocks {
            let mut runs = Vec::with_capacity(b.runs.len());
            for (pieces, style) in &b.runs {
                let mut text = String::new();
                for p in pieces {
                    match p {
                        Piece::Literal(s) => text.push_str(s),
                        Piece::Field { name, spec } => match lookup(name, spec.as_deref()) {
                            Ok(s) => text.push_str(&s),
                            Err(e) => errors.push(e),
                        },
                    }
                }
                runs.push(Run {
                    text,
                    style: *style,
                });
            }
            // A block whose text is all empty (e.g. a field that's blank for
            // this event) is left out, so one template can serve events with
            // different fields.
            if runs.iter().any(|r| !r.text.trim().is_empty()) {
                blocks.push(Block { kind: b.kind, runs });
            }
        }
        if errors.is_empty() {
            Ok(RichText { blocks })
        } else {
            Err(errors)
        }
    }
}

/// Converts markdown to blocks of styled runs. Adjacent text with the same
/// style is merged into one run, so a `{field}` the parser happened to split
/// is whole again before fields are parsed.
fn parse_markdown(text: &str) -> Vec<Block> {
    let mut b = Builder::default();
    for event in Parser::new(text) {
        match event {
            Md::Start(Tag::Emphasis) => b.italic += 1,
            Md::End(TagEnd::Emphasis) => b.italic -= 1,
            Md::Start(Tag::Strong) => b.bold += 1,
            Md::End(TagEnd::Strong) => b.bold -= 1,
            Md::Start(Tag::List(first)) => {
                b.flush();
                b.flush_marker();
                b.lists.push(first);
            }
            Md::End(TagEnd::List(_)) => {
                b.flush();
                b.lists.pop();
            }
            Md::End(TagEnd::Item) => {
                b.flush();
                b.flush_marker();
            }
            Md::Start(Tag::Item) => {
                b.flush();
                let marker = match b.lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        ListMarker::Number(*n - 1)
                    }
                    _ => ListMarker::Bullet,
                };
                b.pending_marker = Some(marker);
            }
            Md::Start(
                Tag::Paragraph | Tag::Heading { .. } | Tag::CodeBlock(_) | Tag::BlockQuote(_),
            ) => b.flush(),
            Md::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::BlockQuote(_),
            ) => b.flush(),
            Md::Text(t) | Md::Html(t) | Md::InlineHtml(t) => b.push(&t, false),
            Md::Code(t) => b.push(&t, true),
            Md::SoftBreak => b.push(" ", false),
            Md::HardBreak => b.push("\n", false),
            _ => {}
        }
    }
    b.flush();
    b.blocks
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    current: Option<Block>,
    /// One entry per open list: the next number, or `None` for bullets.
    lists: Vec<Option<u64>>,
    pending_marker: Option<ListMarker>,
    bold: u32,
    italic: u32,
}

impl Builder {
    fn push(&mut self, text: &str, code: bool) {
        let style = RunStyle {
            bold: self.bold > 0,
            italic: self.italic > 0,
            code,
        };
        let block = self.current.get_or_insert_with(|| Block {
            kind: if self.lists.is_empty() {
                BlockKind::Paragraph
            } else {
                BlockKind::Item {
                    depth: self.lists.len() - 1,
                    marker: self.pending_marker.take(),
                }
            },
            runs: Vec::new(),
        });
        match block.runs.last_mut() {
            Some(last) if last.style == style => last.text.push_str(text),
            _ => block.runs.push(Run {
                text: text.to_string(),
                style,
            }),
        }
    }

    fn flush(&mut self) {
        if let Some(mut block) = self.current.take() {
            // Code blocks end with a newline; drop trailing line breaks.
            if let Some(last) = block.runs.last_mut() {
                let trimmed = last.text.trim_end_matches('\n').len();
                last.text.truncate(trimmed);
            }
            self.blocks.push(block);
        }
    }

    /// An item with no text of its own (e.g. only a nested list) still gets
    /// its bullet.
    fn flush_marker(&mut self) {
        if let Some(marker) = self.pending_marker.take() {
            self.blocks.push(Block {
                kind: BlockKind::Item {
                    depth: self.lists.len().saturating_sub(1),
                    marker: Some(marker),
                },
                runs: Vec::new(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: RunStyle = RunStyle {
        bold: false,
        italic: false,
        code: false,
    };
    const BOLD: RunStyle = RunStyle {
        bold: true,
        italic: false,
        code: false,
    };
    const ITALIC: RunStyle = RunStyle {
        bold: false,
        italic: true,
        code: false,
    };
    const CODE: RunStyle = RunStyle {
        bold: false,
        italic: false,
        code: true,
    };

    fn run(text: &str, style: RunStyle) -> Run {
        Run {
            text: text.into(),
            style,
        }
    }

    fn render(template: &str, fields: &[(&str, &str)]) -> RichText {
        Template::parse(template)
            .unwrap()
            .render(|name, _| {
                fields
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, v)| v.to_string())
                    .ok_or_else(|| format!("unknown field \"{name}\""))
            })
            .unwrap()
    }

    #[test]
    fn civil_war_template() {
        let t = render(
            "**{label}: {start}**\n\n- North: {union}\n- South: {confederate}",
            &[
                ("label", "Shiloh"),
                ("start", "April 06, 1862"),
                ("union", "1,754"),
                ("confederate", "1,728"),
            ],
        );
        let bullet = BlockKind::Item {
            depth: 0,
            marker: Some(ListMarker::Bullet),
        };
        assert_eq!(
            t.blocks,
            vec![
                Block {
                    kind: BlockKind::Paragraph,
                    runs: vec![run("Shiloh: April 06, 1862", BOLD)]
                },
                Block {
                    kind: bullet,
                    runs: vec![run("North: 1,754", PLAIN)]
                },
                Block {
                    kind: bullet,
                    runs: vec![run("South: 1,728", PLAIN)]
                },
            ]
        );
    }

    #[test]
    fn values_cant_change_formatting() {
        let t = render("{label} _x_", &[("label", "*not bold* __nor this__")]);
        assert_eq!(
            t.blocks[0].runs,
            vec![run("*not bold* __nor this__ ", PLAIN), run("x", ITALIC)]
        );
    }

    #[test]
    fn styles_and_code() {
        let t = render(
            "*a* _b_ **c** __d__ ***e*** `{phone}`",
            &[("phone", "555-0100")],
        );
        let r = &t.blocks[0].runs;
        assert_eq!(r[0], run("a", ITALIC));
        assert_eq!(r[2], run("b", ITALIC));
        assert_eq!(r[4], run("c", BOLD));
        assert_eq!(r[6], run("d", BOLD));
        assert_eq!(
            r[8],
            run(
                "e",
                RunStyle {
                    bold: true,
                    italic: true,
                    code: false
                }
            )
        );
        assert_eq!(r[10], run("555-0100", CODE));
    }

    #[test]
    fn commonmark_edge_cases() {
        // None of these are emphasis.
        let t = render(
            r"2 * 3 * 4 snake_case_name {first_name} \*lit\*",
            &[("first_name", "Ada")],
        );
        assert_eq!(
            t.blocks[0].runs,
            vec![run("2 * 3 * 4 snake_case_name Ada *lit*", PLAIN)]
        );
    }

    #[test]
    fn numbered_and_nested_lists() {
        let t = render("3. a\n4. b\n   - c\n\n   more\n5. d", &[]);
        let kinds: Vec<_> = t
            .blocks
            .iter()
            .map(|b| (b.kind, b.runs.first().map(|r| r.text.as_str())))
            .collect();
        let item = |depth, marker| BlockKind::Item { depth, marker };
        assert_eq!(
            kinds,
            vec![
                (item(0, Some(ListMarker::Number(3))), Some("a")),
                (item(0, Some(ListMarker::Number(4))), Some("b")),
                (item(1, Some(ListMarker::Bullet)), Some("c")),
                (item(0, None), Some("more")),
                (item(0, Some(ListMarker::Number(5))), Some("d")),
            ]
        );
    }

    #[test]
    fn breaks_and_flattening() {
        let t = render("one\ntwo\\\nthree\n\n# Head\n\n[link](http://x)", &[]);
        assert_eq!(t.blocks[0].runs, vec![run("one two\nthree", PLAIN)]);
        assert_eq!(t.blocks[1].runs, vec![run("Head", PLAIN)]);
        assert_eq!(t.blocks[2].runs, vec![run("link", PLAIN)]);
    }

    #[test]
    fn empty_blocks_are_dropped() {
        let t = render("**{label}**\n\n{duration}\n\n`{content}`", &[("label", "Call"), ("duration", "4 min"), ("content", "")]);
        assert_eq!(t.blocks.len(), 2);
        assert_eq!(t.blocks[1].runs, vec![run("4 min", PLAIN)]);
        let t = render("- {a}\n- {b}", &[("a", ""), ("b", "x")]);
        assert_eq!(t.blocks.len(), 1);
    }

    #[test]
    fn all_field_errors_are_reported() {
        let errs = Template::parse("{a} {b} {c}")
            .unwrap()
            .render(|n, _| {
                if n == "b" {
                    Ok("ok".into())
                } else {
                    Err(format!("bad {n}"))
                }
            })
            .unwrap_err();
        assert_eq!(errs, ["bad a", "bad c"]);
    }

    #[test]
    fn brace_errors_surface_at_parse() {
        assert!(Template::parse("**{label**}").is_err());
    }
}
