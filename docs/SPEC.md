# timeliner — Specification (draft)

Status: **draft**. All questions in `QUESTIONS.md` are answered. Defaults Claude proposed and Aaron hasn't explicitly confirmed are noted there.

## Purpose

`timeliner` is a command-line tool, mainly for Claude agents, that renders a horizontal timeline as a PNG. The look follows Vizzlo-style timelines: a title, a horizontal line, a marker per event, and boxed event descriptions that alternate above and below the line.

## Command line

```
timeliner [--style=<style.json>] <desc.json> <output.png>
```

- `<desc.json>`: the timeline description. If it is `-`, read stdin.
- `<output.png>`: the output path. If it is `-`, write the PNG to stdout.
- `--style`: optional style file. Keys it leaves out fall back to the built-in default style, so a style file can be a partial override (even `{}`). Unknown keys are an error, with a "did you mean" hint.
- `--` keeps its usual meaning (end of options), so `timeliner -- -odd-name.json out.png` works.
- On success, after any warnings, exactly one summary line goes to stderr:
  ```
  timeliner: wrote out.png (1584×1120 px, 12 events, 2 jogs, 0 warnings)
  ```
  When writing to stdout, the name is shown as `<stdout>`.
- Exit code is 0 on success and non-zero on any error. Errors go to stderr. Nothing goes to stdout except the PNG when stdout output is selected.

## Input format

All keys in both files use snake_case. Both files are parsed as JSON5 (unquoted keys, trailing commas, comments allowed), which accepts strict JSON too. Strings can't contain raw newlines (not even in JSON5), so multi-line templates use `\n`.

### Description (`desc.json`)

| Key | Type | Required | Meaning |
|---|---|---|---|
| `title` | string | no | Title text |
| `subtitle` | string | no | Subtitle text |
| `datetime_format` | string | no | strftime format used when a date is substituted into text |
| `time_zone` | string | no | IANA zone name. When set, all times are displayed in this zone |
| `start` | date | no | Left end of the timeline. If omitted, the line is open-ended on the left (arrow) |
| `start_label` | string | no | Label at the left end. Only allowed with `start` |
| `stop` | date | no | Right end of the timeline. If omitted, the line is open-ended on the right (arrow) |
| `stop_label` | string | no | Label at the right end. Only allowed with `stop` |
| `event_text` | string | no | Template for every event box's contents. Default: `"**{label}**\n\n{start}"`. It lives here, not in the style, because its fields are tied to the data |
| `shows_stop` | bool | no (default `false`) | Default for every event: draw a range from `start` to `stop` (see below) |
| `events` | array | yes | Events (see below). May be empty: the image then shows just the title, the line, and its end labels. An empty list needs both `start` and `stop`, because an open end has no event to extend to |

Events may be listed in any order. They are sorted by `start` (by actual instant), and events with equal starts keep their input order (stable sort). Error messages still use the original `events[i]` index.

Each event has:

| Key | Type | Required | Meaning |
|---|---|---|---|
| `label` | string | yes | Event name, available as `{label}` in `event_text` |
| `start` | date | yes | Where the marker goes, and where the connector attaches |
| `stop` | date | no | End date. Must not be before `start` |
| `shows_stop` | bool | no | Overrides the top-level `shows_stop` for this event |
| *any other key* | string / number / bool | no | Available for substitution in `event_text` |

Dates and times are ISO 8601. Each `start` or `stop` may be:

- a year: `1066`, `-2560` (a JSON integer like `1066` is accepted too);
- a year and month: `1066-10`, `-0043-03`;
- a date: `2024-03-05`;
- a date and time: `2024-03-05T14:32`, `2024-03-05T14:32:07`, or with a space instead of the `T`;
- a date and time with a zone: `2024-03-05T14:32-05:00`, `2024-03-05T19:32Z`, or with an IANA zone name, `2024-03-05T14:32-05:00[America/New_York]` (RFC 9557).

All these forms can be mixed in one file, except as limited under time zones below.

**Range check:** every event's `start` and `stop` must fall within the timeline's `start`…`stop`, endpoints included. An event outside that range is an error. All such events are reported at once, each with its index, label, and date, and the timeline's range, e.g. `events[4] "Fredericksburg": start 1860-12-11 is before timeline start 1861-04-12`. An open end (missing `start` or `stop`) has no limit on that side.

**Placement**
- A date with no time means midnight at the start of that day.
- Year-only and year-month values mean the **midpoint** of that period: halfway between its first instant and the first instant of the next period. For a year that's July 2, 12:00 (common year) or July 2, 00:00 (leap year); for a month, e.g. `1066-10` → October 16, 12:00.

**Year range:** −9999 to 9999 (jiff's range).
- **Numbering:** years use the proleptic Gregorian calendar with astronomical numbering, where year 0 is 1 BCE and year −43 is 44 BCE.
- **Negative years:** ISO 8601 / RFC 9557 writes them with a sign and six digits, e.g. `-000043-03-15`. The shorter four-digit form `-0043-03-15` is also accepted, as is a leading `+` on ordinary years. If jiff doesn't parse the short form, we normalize it to the six-digit form before parsing.

**Display**
- If `datetime_format` is omitted, the default depends on the input: `%B %-d, %Y` when every value is date-only, and `%B %-d, %Y %-I:%M %p` when any value has a time.
- Partial dates are displayed only to the precision they were written with, not through `datetime_format`: a year as `%Y` (`1066`) and a year-month as `%B %Y` (`October 1066`).
- Years 0 and below display as BCE: year `y` shows as `(1 − y) BCE`, so `-0043` → `44 BCE` and `0000` → `1 BCE`. This applies everywhere `%Y` is formatted, including full dates (`-0043-03-15` with `%B %-d, %Y` → `March 15, 44 BCE`). Positive years print as plain numbers, with no "CE".

**Time zones**
- **Placement:** events go on the line by the actual moment they happened, so 09:00 in New York and 06:00 in Los Angeles land at the same point.
- **Mixing:** values with a zone or offset can't be mixed with date-times that have none. That's an error, because the two can't be ordered reliably. Date-only and partial values count as zoneless.
- **Display:** times are shown in the top-level `time_zone` (an IANA name such as `"America/New_York"`, or `"UTC"`) if it's set, and otherwise in each value's own zone or offset. `datetime_format` may use `%Z` (abbreviation, e.g. EST) and `%z` (offset).

### Style (`style.json`)

Lengths are strings with a unit, e.g. `"12pt"`, `"0.5in"`. Supported units:

- `pt`: 1/72 in, the internal unit.
- `in`: 72 pt.
- `cm`: 72 / 2.54 pt.
- `mm`: 72 / 25.4 pt.
- `px`: output pixels, so 1 px = 1 / `pixels_per_point` pt.

A length without a unit (`12` or `"12"`) is an error, e.g. `marker_size: "12" needs a unit (pt, px, in, mm, cm)`. `pixels_per_point` and `max_event_width` are plain numbers, not lengths. Colors are CSS color strings: web-style hex (`"#8a2391"`, plus `"#rgb"`, `"#rgba"`, and `"#rrggbbaa"` with alpha), named colors (`"lightgray"`), and functions (`"rgb(…)"`, `"hsl(…)"`). The default style deliberately mixes hex and named colors, so both parsing paths get exercised.

| Key | Default | Meaning |
|---|---|---|
| `pixels_per_point` | `2` | Pixels per point in the output |
| `image_width` | `"792pt"` | Total image width (11 in, the long side of US Letter). The timeline spans from `left_margin` to `image_width - right_margin` |
| `default_font_family` | `"Open Sans"` | Font family: an embedded font (Open Sans, Inconsolata) or any installed system font |
| `monospace_font_family` | `"Inconsolata"` | Font for `` `code` `` spans in event text. Same lookup rules as `default_font_family` |
| `background` | `"white"` | Background color |
| `top_margin` / `bottom_margin` | `"64pt"` | Page margins |
| `left_margin` / `right_margin` | `"48pt"` | Page margins |
| `title` | `"center"` | Title alignment: `"left"`, `"center"`, or `"right"` (within the margins) |
| `title_size` / `title_bold` | `"24pt"` / `true` | |
| `title_color` | `"#1f3a68"` (dark blue) | Title text color |
| `subtitle` | `"center"` | Subtitle alignment: `"left"`, `"center"`, or `"right"` |
| `subtitle_size` / `subtitle_bold` | `"18pt"` / `true` | |
| `subtitle_color` | `"#1f3a68"` (dark blue) | Subtitle text color |
| `event_text_place` | `"alternating"` | Where event boxes go: `"alternating"` (above, below, above, …), `"above"`, or `"below"` |
| `rect_line_color` / `rect_fill_color` | `"black"` / `"#ebebeb"` (light gray) | Event box stroke and fill |
| `rect_line_width` | `"1pt"` | Event box stroke width |
| `rect_margin` | `"10pt"` | Padding between box edge and text |
| `max_event_width` | `6` | A number N: no event box may be wider than `image_width / N`, including its `rect_margin` |
| `rect_gap` | `"8pt"` | Minimum empty space between any two event boxes, horizontally and vertically |
| `rounded_corner` | `"5pt"` | Corner radius of event boxes |
| `text_color` | `"black"` | Event box text and start/stop labels |
| `timeline_color` | `"black"` | Timeline and end markers |
| `timeline_width` | `"2pt"` | Thickness of the timeline (and of `"bar"` markers) |
| `connector_color` | `"#1f3a68"` (dark blue) | Lines from markers to event boxes |
| `connector_width` | `"1pt"` | Thickness of connector lines |
| `connector_corner` | `"5pt"` | Radius of the rounded bends where a connector jogs. Reduced automatically if a segment is too short |
| `connector_dashed` | `false` | Draw connectors dashed. The dash pattern scales with `connector_width`: dashes 4× and gaps 3× the width |
| `marker` | `"dot"` | Event marker shape: `"dot"`, `"square"`, `"diamond"`, `"bar"`, or `"none"` |
| `marker_color` | `"#1f3a68"` (dark blue) | Event marker color |
| `marker_size` | `"12pt"` | Marker size: the diameter of a dot, the side of a square, the width and height of a diamond (corner to corner), or the length of a bar |
| `event_text_size` | `"12pt"` | Event box text size |

Example `event_text` (from the Civil War example, written in `desc.json`):

```
event_text: "**{label}: {start}**\n\n- North: {union:,d}\n- South: {confederate:,d}"
```

## Event text: template plus simplified markdown

1. The template is parsed as markdown **first**, and substitution is applied afterwards to the text runs. A label that contains `*` or `_` therefore can't change the formatting.
2. Supported markdown is a CommonMark subset: `*italic*` / `_italic_`, `**bold**` / `__bold__`, `` `code` `` (inline code, in `monospace_font_family`), numbered lists (`1.`), bullet lists (`-`), backslash escapes, and paragraphs separated by blank lines. Other constructs (headings, links, fenced code blocks) are rendered as plain text.
   - Fields are substituted inside code spans too, so `` `{phone}` `` shows the value in monospace.
   - Bold therefore needs `**…**`, as in the default `**{label}**`.
3. Substitution uses Python f-string syntax, `{field}` or `{field:spec}`:
   - **`{field}`:** the value with default formatting. Dates use `datetime_format`, and partial dates use their input precision.
   - **Numbers:** `spec` is Python's format-spec mini-language, `[[fill]align][sign][z][#][0][width][grouping][.precision][type]`. For example, `,d` gives 1,754, `.1f` gives 3.1, `.0%` gives 45%, and `>8,` right-aligns in 8 characters. Python's behavior is the reference for tests.
   - **Dates:** `spec` is a strftime format, e.g. `{start:%b %Y}`. An explicit format always wins over `datetime_format` and over the precision rule for partial dates. `{start:%B %-d, %Y}` on `1066` shows the midpoint, "July 2, 1066", because that's what was asked for. BCE years still display as "N BCE".
   - **Booleans** display as `true` / `false` and take string format specs.
   - **Literal braces:** `{{` and `}}`.
   - **Errors:** an unknown field, or a spec that doesn't fit the value's type (e.g. `,d` on a string), is an error naming the event, field, and spec.

## Rendering

- **Width:** `image_width × pixels_per_point`, which is 1584 px with the defaults.
- **Height:** exactly `top_margin` + content + `bottom_margin`. There is no minimum height, and the content is never empty, because the timeline is always drawn.
- **Title and subtitle:** at the top, aligned per the `title`/`subtitle` style keys, subtitle below the title. If the description omits `title` or `subtitle`, that line is not drawn and takes no space.
- **Timeline:** a solid horizontal line from the left margin to the right margin, drawn in `timeline_color` at `timeline_width`. It never fades. Time maps linearly from the left end to the right end.
- **Open ends:** if `start` (or `stop`) is omitted:
  - the line runs a little past the earliest (or latest) event: 5% of the event span, or one day if every event has the same date;
  - that end gets an arrowhead pointing outward instead of an end marker, in `timeline_color`, `marker_size` long and wide;
  - that end has no rotated label.
  Either end, or both, may be open.
- **Marker shapes:** all are centered on the line at the event's date.
  - `"dot"`: a filled circle.
  - `"square"`: a filled axis-aligned square.
  - `"diamond"`: a square rotated 45°.
  - `"bar"`: a vertical tick crossing the line, `marker_size` long and as thick as the timeline.
  - `"none"`: no marker. The connector starts at the line, and ranges (`shows_stop`) are still drawn as colored segments.
- **End markers:** only at ends with a given `start`/`stop`. They have the same shape and size as event markers, in the timeline color. With `"none"`, those ends get no markers.
- **Start and stop labels:** above the line at each closed end, rotated 90° counter-clockwise so they read bottom-to-top.
  - **Content:** two lines, the label (if `start_label`/`stop_label` is given) and then the formatted date. Without a label, only the date is shown.
  - **Style:** `event_text_size` and `text_color`, with the label in bold.
  - **Position:** the text starts just above the end marker and is centered horizontally on the end's x position.
- **Event markers:** one per event, on the line at the event's `start`.
- **Ranges:** when an event's effective `shows_stop` is true and it has a `stop` later than `start`:
  - a second marker (same shape, size, and color) is drawn at `stop`;
  - the line between the two markers is drawn in `marker_color` instead of `timeline_color`;
  - the connector still attaches at the `start` marker.
  If `stop` is missing or equals `start`, only the single marker is drawn.
- **Event boxes:** placed according to `event_text_place`. With `"alternating"`, the first event in date order goes above, the next below, and so on. A box is just big enough for its text plus `rect_margin` on every side. Text wraps so that the box is never wider than `image_width / max_event_width`. The one exception is a single word too wide for that limit: the box grows just enough to fit the word on one line, and a warning naming the event goes to stderr. Words are never broken mid-word.
- **Connectors:** each marker is joined to its box by a line.
  - The connector always meets its box at the box's **left edge**: on the edge facing the line, at `rounded_corner` in from the left side, where the straight part of the edge begins.
  - Ideally the box is placed so that point is directly above (or below) the marker, and the connector is a single vertical line.
  - If the box has to sit elsewhere (crowding, or the right margin), the connector gets a purely horizontal jog (orthogonal routing), with its two bends rounded to `connector_corner`.
  - Connectors never touch or cross each other or any box other than their own. If crowding makes that impossible, the image is still written and each violation is reported as a warning on stderr.
- **Box placement:** boxes may move away from the line (up for the upper side, down for the lower side) to avoid collisions, but never across the line.
