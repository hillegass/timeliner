f# Open questions

Answer inline (replace "Answer:" with the decision). Resolved items get moved into `SPEC.md` / `DESIGN.md`.

## CLI

**Q1. stdin/stdout sentinel.** The request says `--`. In most CLIs (and in `clap`), `--` means "end of options", so `timeliner -- --` is awkward to parse, and `-` is the Unix convention for stdin/stdout. Use `-`, accept both, or keep `--` only?
*Recommendation:* use `-`, and also accept `--` in either position.
Answer: **`-` only** (the Unix convention). `--` is not a stdin/stdout marker (Aaron, 2026-11-01).

**Q2. Partial style files.** Should `--style` merge with the defaults, so a file of just `{marker_color:"blue"}` works, or does it have to be complete?
*Recommendation:* merge, and reject unknown keys.
Answer: **Merge.** Any key the file leaves out takes its default value (Aaron, 2026-11-01). Unknown keys are rejected, per the recommendation.

## Input format

**Q3. JSON or JSON5?** The examples use unquoted keys and have small syntax slips (`stop:""1865-04-09"`, a missing comma after `subtitle_size`, a missing closing `}`). Accept JSON5, or require strict JSON?
*Recommendation:* JSON5. Strict JSON is a subset of it, so both are accepted.
Answer: **JSON5** (relaxed syntax; strict JSON also works) (Aaron, 2026-11-01).

**Q4. `{name}` vs `label`.** The template uses `{name}`, but events have a `label` key. Should `name` be an alias for `label`, should the template change to `{label}`, or should both work?
Answer: **`{label}`**. The template was a typo. No `name` alias (Aaron, 2026-11-01).

**Q5. Multi-day events.** Spotsylvania runs May 8–21. Does the marker go at `start`, at the midpoint, or should a range be drawn as a thicker bar or capsule along the line? Is `stop` optional for events?
Answer: **`shows_stop`**, a bool at the top level of desc.json, which each event can override. When it's true, a second marker goes at `stop` and the line between the markers is drawn in `marker_color`. The connector attaches at the start marker. `stop` is optional (Aaron, 2026-11-01).

**Q6. Times.** `datetime_format` suggests date-times. Do inputs ever include times (`1862-04-06T05:00`), or only dates? Do we need dates before 1 AD or BCE dates (geological or ancient timelines)?
Answer: **Times are supported**, e.g. a lawyer's timeline of phone calls and texts (Aaron, 2026-11-01). Years −9999 to +9999 are allowed (Aaron, 2026-11-01). See Q23 for how they are written and shown.

**Q7. Units.** Which length units are supported: `pt` only, or also `px`, `in`, `mm`? Is a bare number (`12`) allowed, and if so what unit is it?
Answer: **`px`, `pt`, `in`, `mm`, `cm`** (Aaron, 2026-11-01). Bare numbers are not allowed: a length must always have a unit.

**Q8. Width.** Is `linelength` the line's length or the whole image width?
Answer: **The whole image width.** `linelength` is renamed to `image_width`, and the timeline runs from `left_margin` to `right_margin`. All keys in both files use snake_case (Aaron, 2026-11-01).

**Q9. Fonts.** OK to embed Open Sans (regular, bold, italic, bold italic; about 500 KB) and Inconsolata for `` `code` `` spans in the binary? (Monospace font answered: **Inconsolata**, Aaron, 2026-11-01. Embedding answered: **yes, embed both**, Aaron, 2026-11-01.) Should `default_font_family` also be able to name system fonts, or a font file path?
Answer: Embed Open Sans and Inconsolata. Family names may also refer to **installed system fonts** (Aaron, 2026-11-01). Embedded fonts win on a name clash; an unknown family is an error.

**Q10. Other values for enum-like keys.** What other values are allowed?
- ~~`title` / `subtitle`~~ Answered: `"left"`, `"center"`, `"right"`. Later superseded: titles were removed entirely; the surrounding document supplies them (Aaron, 2026-11-01).
- ~~`marker`~~ Answered: `"dot"`, `"square"`, `"diamond"`, `"bar"`, `"none"` (Aaron, 2026-11-01).
- ~~`alternating: false`~~ Answered: replaced by `event_text_place`: `"alternating"` | `"above"` | `"below"` (Aaron, 2026-11-01).
Answer: All parts answered (see above).

**Q11. Connector and end-marker colors.** Connectors use `timeline_color`? Or `rect_line_color`? Should they get their own `connector_color` / `connector_width` keys? What's the timeline's thickness: a `timeline_width` key?
Answer: Add `connector_color`, `connector_width`, `connector_dashed` (Aaron, 2026-11-01). Defaults: `"#1f3a68"` dark blue (changed from green by Aaron, 2026-11-01); the timeline and end markers are `"black"`, `"1pt"`, `false`. Also `timeline_width`, default `"2pt"` (Aaron, 2026-11-01).

## Event text

**Q12. Bold/italic syntax.** CommonMark makes `*x*` italic and `**x**` bold. Your spec makes `*x*` bold and `_x_` italic (Slack style). Both are doable (see `DESIGN.md`). Claude writes CommonMark from habit.
*Recommendation:* CommonMark.
Answer: **CommonMark** (Aaron, 2026-11-01). Use `pulldown-cmark` rather than a hand-written parser; see `DESIGN.md`.

**Q13. Format specs.** Beyond `{x}` and `{x:,d}`, which Python format specs matter (`.1f`, `%`, padding)? Should a per-field date format be allowed, e.g. `{start:%b %Y}`, overriding `datetime_format`?
Answer: **Explicit formats win:** show exactly what the template asks for. Numbers use the full Python format-spec mini-language, and `{start:%b %Y}`-style date formats override `datetime_format` and the partial-date precision rule (Aaron, 2026-11-01).

## Layout

**Q14. Page height.** Is there a fixed or minimum page height?
Answer: **No minimum.** The height is exactly the content plus `top_margin` and `bottom_margin`, so it is always more than the two margins combined (Aaron, 2026-11-01).

**Q15. Start and stop labels.** Should the vertical end labels show both the label and the formatted date (e.g. "Fort Sumter — April 12, 1861")? Do they read bottom-to-top, rotated 90° counter-clockwise?
Answer: **Label (if any) and date, reading bottom-to-top** (Aaron, 2026-11-01). Layout details (two lines, bold label, event text size) are proposed in `SPEC.md`.

**Q16. Box width.** What's the maximum box width before text wraps?
Answer: **The `max_event_width` key** (default `6`). No box may be wider than `image_width / max_event_width` (Aaron, 2026-11-01).

**Q17. GPU vs CPU.** OK to use `vello_cpu` (same project, no GPU needed) instead of GPU `vello`? See `DESIGN.md`.
Answer: **`vello_cpu`** (Aaron, 2026-11-01).

**Q18. Machine-readable output.** Should the tool print a short JSON summary to stderr on success (image size, number of events, warnings such as "3 boxes were stacked") so an agent can check the result without looking at the image?
Answer: **Yes, one plain-text line** (Aaron, 2026-11-01). Format in `SPEC.md`.

**Q19. Impossible layouts.** If events are too dense to place without crossings, should the tool fail with an error, or draw its best attempt and warn?
Answer: **Draw the best attempt and warn.** Use vertical shifting first: in a tight cluster, the outer boxes stay near the line and the middle one moves farther out, with its connector passing between them (Aaron, 2026-11-01). See `DESIGN.md`, layout steps 4 and 6.

**Q20. Reference image.** I can't open the Vizzlo example from here. Could you describe, or save to `docs/reference/`, details I should match: line thickness, whether connectors attach to the box edge or center, and whether boxes have shadows?
Answer: Aaron provided Vizzlo's mechanical-engineering timeline image (2026-11-01). It isn't in the repository, because the image is Vizzlo's copyrighted work. Differences from our spec are listed in Q24.

**Q21. Long words.** The defaults are now `image_width` 792pt and `event_text_size` 12pt. A box can then be up to 792 / 6 = 132 pt wide, leaving 112 pt for text: about 17 characters per line, which is enough for most labels. A single word or number wider than that can still happen. Should the box grow to fit it, should the word be broken mid-word, or should it be an error?
*Recommendation:* let the box grow to fit the word, and warn on stderr.
Answer: **The box grows** to fit the widest word on one line, with a warning on stderr. Words are never broken (Aaron, 2026-11-01).

**Q22. Time zones.** Phone records often carry a UTC offset (`2024-03-05T14:32:07-05:00`) or are in UTC (`…Z`), and different sources may use different zones. Options:
- (a) **No zones:** times are taken exactly as written, and an offset is an error.
- (b) **Offsets allowed:** events are placed on the line by their actual instant. Each is displayed in its own offset, or in one `time_zone` key (e.g. `"America/New_York"`) if one is set.
- (c) Like (b), and also add `%Z` (zone abbreviation) support to the display format.
*Recommendation:* (b). Mixing a value that has an offset with one that doesn't is an error, because the two can't be ordered reliably.
Answer: **Allow time zones; use `jiff`** (Aaron, 2026-11-01). Implemented as (b) plus (c): offsets, IANA names, an optional `time_zone` display key in desc.json, and `%Z`/`%z`.

**Q23. Writing and showing ancient dates.** Three parts:
- **Input form:** ISO 8601 writes 44 BC as `-000043-03-15`: year 0 is 1 BC, and the year has a sign and six digits. Should we also accept the shorter `-0043-03-15`? **Answered: yes, accept both** (Aaron, 2026-11-01).
- **Partial dates:** ancient events often have only a year (`-2560`, for the Great Pyramid) or a year and month. Should `YYYY` and `YYYY-MM` be allowed, meaning the start of that year or month? **Answered: yes, both allowed, meaning the *midpoint* of that year or month** (Aaron, 2026-11-01). Follow-up: should a partial date be displayed at its own precision (`1066`, `October 1066`) rather than through `datetime_format`, which would print the made-up midpoint, e.g. "July 2, 1066"? I'd say yes: years display as `%Y` and year-months as `%B %Y`. **Answered: yes, display at input precision** (Aaron, 2026-11-01).
- **Display:** strftime `%Y` would show 44 BC as `-43`. Should there be a way to get "44 BC", e.g. a custom `%E` code for era-style years (`44 BC`, `AD 1066`, or plain `1066` when every year is positive)?
Answer: Short form accepted; partial dates allowed, placed at their midpoint, and shown at input precision; years ≤ 0 shown as "N BC" (Aaron, 2026-11-01).

**Q24. Which features of the reference image should we adopt?** Compared with our spec, the reference image differs in these ways:
1. ~~**Text alignment.**~~ **Answered: the connector always attaches at the box's left edge** (Aaron, 2026-11-01). Each text block is left-aligned, with the connector at its left edge, not centered over the marker. Left alignment packs clusters better, and the jog rule still works the same way. Our spec centers each box on its marker. Adopt left alignment, keep centering, or make it a style key (`event_text_align`: `"left"`/`"center"`)?
2. ~~**Date labels at markers.**~~ **Answered: no.** Dates stay in the event text box via `{start}`; nothing is drawn beside markers (Aaron, 2026-11-01). Every marker has a small gray date on the opposite side of the line from its text block ("4000 BCE" above the line, with the Wheeled Vehicle text below). This keeps the date visible even when the template doesn't show it. Add it as an option, e.g. `marker_dates: true`?
3. ~~**Connector ends.**~~ **Answered: no end dot.** Connectors run to the box edge (Aaron, 2026-11-01). Connectors end in a small dot just short of the text block and don't touch it. Ours run to the box edge.
4. ~~**Rounded jog corners.**~~ **Answered: yes, round them** (Aaron, 2026-11-01). Radius is the `connector_corner` style key. The Aeolipile connector goes up, right, and up again, with rounded corners. It's also stacked one row higher than Windmill, which is the vertical shifting you described.
5. ~~**Line ends.**~~ **Answered: the line never fades.** `start` and `stop` are optional, and a missing end gets an outward arrowhead (Aaron, 2026-11-01). The timeline runs past the last event and fades out with a gradient. It has no end markers and no rotated start/stop labels; the subtitle shows the date range instead.
6. ~~**No boxes.**~~ **Answered: keep the boxes**, and add a `rect_gap` key for minimum spacing between boxes (Aaron, 2026-11-01). The text sits directly on the background, with a bold small-caps heading and an italic body. Our boxes can produce this look by setting `rect_fill_color` and `rect_line_color` to `"transparent"`.
7. ~~**Markers.**~~ **Answered: no marker outlines** (Aaron, 2026-11-01). Markers are light gray with a white outline, which separates them where they overlap (650 BCE / 645 BCE). Add a `marker_outline_color`?
8. ~~**BCE vs BC.**~~ **Answered: use "BCE"** (Aaron, 2026-11-01). Supersedes the "N BC" display in Q23.
Answer: All eight items answered (see above).
