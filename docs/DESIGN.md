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
- Titles and labels are single parley layouts. The rotated start and stop labels are the same layout drawn with a 90° rotation.
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
     ─► layout: positions of title, line, markers, boxes, connectors (pure geometry, no rendering)
     ─► render: geometry → vello_cpu scene → pixmap → PNG bytes → file or stdout
```

Keeping **layout** as pure geometry (rectangles and polylines in points) separate from **render** means:

- **Testing:** layout can be unit-tested with fake text sizes, without fonts or pixels.
- **Validation:** an invariant checker (no overlapping boxes, no crossing connectors, nothing crossing the line, everything inside the margins) can run in tests, and possibly at runtime as a safety net.
- **Agent feedback:** an optional machine-readable report of the layout could be printed **[Q18]**.

Planned modules: `cli`, `model` (desc and style structs, units, colors), `template` (f-string substitution), `richtext` (markdown to styled blocks), `text` (parley wrapper and embedded fonts), `layout`, `render`.

## Layout algorithm

Coordinates are in points, and pixels are applied only at render time.

1. **Horizontal scale:** x(t) maps `start..stop` linearly onto `left_margin..(width - right_margin)`.
2. **Sides:** after sorting by date, `event_text_place` decides the side. With `"alternating"`, event `i` goes above if `i` is even and below if odd. `"above"` or `"below"` puts every event on one side, so that side's stack does all the collision avoidance.
3. **Box size:** text is wrapped (by parley) at `image_width / max_event_width - 2 × rect_margin`. If the longest word is wider than that, the wrap width becomes that word's width and a warning is printed. Box size is the text bounds plus `rect_margin` on every side.
4. **Per side, assign boxes to rows by distance from the line.** Boxes may move away from the line as far as needed, because the image grows to fit them. This is the main way to resolve crowding. For example, given three events close together, the outer two sit nearest the line and the middle one goes one row farther out. Its connector runs straight up (or down) through the gap between the two nearer boxes. Row placement works like this:
   - **Anchor:** every box has a fixed attach point: its near edge, `rounded_corner` in from its left side. The preferred position puts the attach point at the marker's x, so the box extends to the right of its marker, as in `docs/reference/`.
   - **Row 1:** go through the side's events left to right and place each box at its preferred position if it stays at least `rect_gap` away from every box already placed. All box-to-box checks in this step, in the skyline, and in the invariant checker use boxes expanded by `rect_gap`. A row-1 box must also leave a clear gap (connector width plus clearance) at the x of every event not yet placed, so those events' connectors can get through.
   - **Rows 2, 3, …:** repeat with the remaining events. Rows are not fixed heights; each box rests just beyond the boxes already placed beneath the part of the line it spans (a "skyline"). Its connector must pass through gaps in every row nearer the line.
   - **Jogs:** a box may also be shifted sideways from its preferred position, usually rightward past a neighbor, or leftward near the right margin. Its connector then goes vertical, horizontal, vertical. The horizontal jog runs in the clear band between the line and the nearest boxes, and must cross no box or other connector. The reference image's Aeolipile works this way: it jogs right past Windmill's box.
   - **Choosing:** for each event, compare moving out a row with jogging. Prefer whichever adds less total height; on a tie, prefer no jog.
   - **End labels:** the rotated `start_label` and `stop_label` take space above the line at each end, and boxes treat them as obstacles.
5. **Vertical stacking:** the line's y is set by the tallest stack above it, plus the title block. Page height is `top_margin` + title + upper stack + line + lower stack + `bottom_margin`, with no minimum.
6. **Best effort, never fail on crowding:** boxes never overlap one another or cross the line. That can always be satisfied, because rows can go out without limit. The connector rules (no touching or crossing) are goals. If they can't all be met, render the best layout found and print a warning to stderr for each violation, naming the events, e.g. `warning: connectors for "Wilderness" and "Spotsylvania Court House" touch (markers 1.4pt apart)`. The invariant checker produces these warnings, and tests use the same checker to assert that clean inputs produce no warnings.

The Civil War example is a good stress test: Wilderness (May 5), Spotsylvania (May 8–21), and Cold Harbor (May 31, 1864) are close together near the right end. At the default width, one day is about 0.48 pt, so the Wilderness and Spotsylvania markers are only about 1.4 pt apart. With `event_text_place: "alternating"` they land on opposite sides, which is fine. With `"above"`, their markers overlap and their connectors can't be kept apart, so this should produce a warning, not a failure.
