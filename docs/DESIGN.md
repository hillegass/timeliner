# timeliner — Design notes

Technology choices and architecture. Crate versions were checked on crates.io on 2026-11-01. **Nothing has been added to `Cargo.toml` yet.**

## Rendering backend: `vello_cpu` (decided 2026-11-01)

Vello ships in several flavors from the same repo:

- `vello` 0.11: the GPU compute renderer on `wgpu`. Writing a PNG with it means creating a headless device, rendering to a texture, and copying the pixels back. It needs a working GPU or a software Vulkan/Metal driver. Agent sandboxes, containers, and CI machines often have neither.
- `vello_cpu` 0.3: a CPU renderer with the same imaging model, using `peniko` brushes and `kurbo` shapes. It renders straight into a pixel buffer, needs no GPU, and starts quickly.

**Decision:** use `vello_cpu`. A CLI tool for agents has to run anywhere, and the output is one static image. The scene-building code (paths, fills, strokes, glyph runs, transforms) is nearly the same in both, so a GPU backend could be added later behind a feature flag if it's ever needed.

The PNG is written with the `png` crate, or `vello_cpu`'s own PNG export if it has one (to check when we add crates).

## Text: yes, use `parley`

Vello only *draws* glyphs: you hand it a font and glyph IDs at given positions. It doesn't choose fonts, shape text, or break lines. `parley` 0.12 is Linebender's text layout library and does all three:

- **Font selection:** `fontique` picks fonts by family, weight, and style, including registered in-memory fonts.
- **Shaping:** kerning and ligatures.
- **Rich text:** bold and italic spans in one paragraph.
- **Line breaking:** wraps at a maximum width.
- **Measurement:** gives the width, height, and line metrics needed to size the boxes.

The output is positioned glyph runs that go straight into vello's glyph drawing API. Doing this by hand (for example with `swash` or `skrifa` plus our own line breaking) would mean reimplementing parley poorly.

How we use it:

- Each block of event text (a paragraph or a list item) is one parley layout. Lists are laid out by our code: the bullet or number is a separate layout, and the item text is indented next to it. Parley doesn't know about markdown lists.
- The start and stop labels are laid out like event text and drawn with a 90° rotation.
- **Fonts:** embed Open Sans and a monospace font, Inconsolata (both SIL OFL), in the binary and register them with parley. Inconsolata has no italic, so italic code text is drawn with a synthetic slant. Then output is the same on every machine, and the tool never depends on what fonts are installed, which matters for agents. Font files are the static TTFs from the upstream Google Fonts repos (Open Sans regular/bold/italic/bold italic, Inconsolata regular/bold). They live in `assets/fonts/` with their `OFL.txt` license files and are included with `include_bytes!`. The OFL requires the license to travel with the font, so `timeliner --licenses` prints it. System fonts are also allowed. fontique's system collection is used alongside the embedded fonts, and a family name is resolved embedded-first, so `"Open Sans"` always means the bundled copy. A family found nowhere is an error that suggests the closest installed names. Output that uses system fonts is only reproducible on machines that have the same fonts.

## Color: yes, `color`, though it comes in anyway

`peniko` (which vello uses for brushes) is built on Linebender's `color` crate (0.3) and re-exports it. `color::parse_color` already handles CSS named colors (`lightgray`), hex, `rgb()`, `hsl()`, `oklch()`, and others. We'll use it to parse every color in the style file and convert to the brush color at render time. There's no separate color code to write.

## Markdown: `pulldown-cmark` (decided 2026-11-01)

`pulldown-cmark` 0.13 is a pure-Rust CommonMark pull parser with no HTML dependency. It emits a stream of events (`Start(Emphasis)`, `Text`, `Start(List(Some(1)))`, and so on). That maps directly onto building parley styled spans. It's the standard choice and the best fit here. `comrak` (a full AST, GFM) is heavier than we need, and `markdown-rs` is more spec-focused than useful.

**Decided:** standard CommonMark meaning (`*x*`/`_x_` italic, `**x**` bold).

**Why a library for a small subset:** the hard part of markdown is emphasis, not lists. CommonMark's delimiter rules decide that `2 * 3 * 4` is not italic, that `snake_case` is not italic, how `***x***` and `\*` behave, and when a `*` opens or closes. A hand-written parser would get these cases wrong in ways Claude doesn't expect, because Claude writes real CommonMark. `pulldown-cmark` with `default-features = false` is small (a few dependencies, quick to compile), so the cost is low.

**Substitution doesn't need parser support.** `{` and `}` mean nothing in CommonMark, so the steps are:

1. Parse the template once.
2. Merge adjacent `Text` events, since the parser can split a run at escapes or unmatched delimiters, which would cut through a `{field}`.
3. Substitute fields in each text run.

Because substitution happens after parsing, values containing `*` or `_` can't change the formatting. Inline `Code` events become monospace spans. Substitution applies to them too: pulldown-cmark gives code text verbatim, so `{field}` inside backticks survives parsing. Unsupported events (headings, links, fenced code blocks) are flattened to plain text.

## Other crates (candidates, not yet added)

| Need | Crate | Notes |
|---|---|---|
| CLI parsing | `clap` (derive) | Generates a good `--help`, which matters for agents |
| JSON5 parsing | `json5` 1.x | Works with serde. Accepts relaxed syntax (decided) |
| Typed config | `serde` | Use `deny_unknown_fields` on the style file so typos become errors |
| Dates | `jiff` (decided). Our formatter replaces `%Y` itself so it can print BCE years and partial dates | Handles offsets, IANA zones, DST, and RFC 9557 strings, and formats with strftime. Bundle the time zone database (`tzdb-bundle-always`) so results don't depend on the machine |
| Errors | `anyhow` or a small custom error type | Messages should name the JSON path |

## Pipeline

```
args ─► load desc + style (JSON5 → serde structs, defaults merged, validated)
     ─► build event text: markdown parse → substitute fields → rich text blocks
     ─► measure: parley layout for each text block → box sizes
     ─► layout: positions of line, markers, boxes, connectors (pure geometry, no rendering)
     ─► render: geometry → vello_cpu scene → pixmap → PNG bytes → file or stdout
```

Keeping **layout** as pure geometry (rectangles and polylines in points) separate from **render** means:

- **Testing:** layout can be unit-tested with fake text sizes, without fonts or pixels.
- **Validation:** an invariant checker (no overlapping boxes, no crossing connectors, nothing crossing the line, everything inside the margins) can run in tests, and possibly at runtime as a safety net.
- **Agent feedback:** an optional machine-readable report of the layout could be printed **[Q18]**.

Planned modules: `cli`, `model` (desc and style structs, units, colors), `template` (f-string substitution), `richtext` (markdown to styled blocks), `text` (parley wrapper and embedded fonts), `layout`, `render`.

## Layout algorithm

Implemented in `src/layout.rs` as pure geometry in points, with no fonts or pixels. `src/plan.rs` measures the text and calls it.

1. **Horizontal scale:** x(t) maps the timeline's ends linearly onto `left_margin..(image_width - right_margin)`. An open end extends 5% of the events' span past the outermost event, or one day if all events share one instant.
2. **Sides:** events are sorted by date, and `event_text_place` assigns each one a side. Each side is laid out independently in "distance from the line" coordinates and mapped to page y at the end.
3. **Box size:** text is wrapped by parley at `image_width / max_event_width − 2 × rect_margin`, widened to the longest word if needed, then padded by `rect_margin`.
4. **Boxes hang to the right of their markers:** the connector meets the box `rounded_corner` in from its left edge. The nearest a box can be to the line is `1.5 × marker_size + rect_gap`, which leaves a band where jogs run.
5. **Several layouts per side; the best one is kept.** "Best" means fewest warnings, then the lowest score, then fewest jogs. The score is height plus 12pt per jog plus half the jog's sideways distance, so boxes near the line are worth some jogging (Aaron, 2026-11-01).
   - **Beam search,** run left to right and right to left, each with three sets of search weights (`SEARCHES` in `layout.rs`). The search is greedy and sensitive to its weights, so running several is more robust than tuning one.
     - *The beam:* the search keeps the 12 best partial layouts and extends each with the cheapest candidates for the next event.
     - *Where a candidate box can go:* hanging from its marker; beside a nearby box; with its connector threading the gap beside a box; or against a margin. Each position is tried at the nearest distance where it fits (a skyline over placed boxes and connectors) and a few lanes farther out.
     - *How a candidate is scored:* the height it adds, a jog cost, and a look-ahead cost for each not-yet-placed marker on this side that the box covers. A covered marker forces its event to jog or move out.
   - **A staircase** that is always clean unless markers nearly coincide, at the cost of height. It works right to left; each box hangs straight just beyond the boxes and connectors beneath it. Boxes that must shift left at the right margin jog left past the covering boxes, each in a lower lane, so the jogs nest instead of crossing.
   - **Local improvement** runs on each result: each box in turn is taken out and put back in its best clean spot given all the others, and the change is kept if the side's score improves. This repeats until nothing improves (at most 4 passes). It fixes choices the greedy search couldn't see past.
6. **Vertical stacking:** the line's y is `top_margin` + the farthest extent above the line. The page height adds the farthest extent below the line plus `bottom_margin`.
7. **Checker:** `check_side` reports boxes that overlap or leave the margins, connectors that touch or cross, and connectors that cross another event's box. The layout is drawn regardless, and each problem becomes a warning.

The Civil War example is a good stress test: Wilderness (May 5), Spotsylvania (May 8–21), and Cold Harbor (May 31, 1864) are close together near the right end. At the default width, one day is about 0.48 pt, so the Wilderness and Spotsylvania markers are only about 1.4 pt apart. With `event_text_place: "alternating"` they land on opposite sides, which is fine. With `"above"`, their markers overlap and their connectors can't be kept apart, so this should produce a warning, not a failure.
