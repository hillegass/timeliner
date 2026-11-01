# Describing timeliner to Claude

## Tool description

Use this text in a system prompt or a tool definition's `description`. `skill/timeliner/SKILL.md` contains the same information packaged as a Claude Code skill.

> **timeliner** renders a horizontal timeline of dated events as a PNG.
>
> Use it when the user wants a visual timeline: history, project milestones, a biography, or a sequence of calls and messages. Don't use it for quantities over time; use a plotting tool for those.
>
> Usage: `timeliner [--style=style.json] desc.json out.png`. Use `-` for desc.json to read stdin, or for out.png to write the PNG to stdout.
>
> **Learn the formats by running it:**
> - `timeliner --help`: a complete guide.
> - `timeliner --print-example`: a working description file.
> - `timeliner --print-default-style`: every style key, with comments.
> - `timeliner --schema desc|style`: JSON Schema for each file.
>
> **The description file** (JSON5, so strict JSON also works) holds `events`, a list of `{label, start, stop?, ...}`, plus these optional keys:
> - `start`/`stop`: the timeline's ends. A missing end is drawn as an arrow.
> - `start_label`/`stop_label`: labels at the ends.
> - `datetime_format`: strftime format for dates.
> - `time_zone`: IANA zone to display times in.
> - `shows_stop`: draw each event's start–stop range on the line.
> - `event_text`: markdown template for each event's box.
>
> **Event text.** In `event_text`, `{key}` inserts any key of the event. Numbers take Python format specs (`{killed:,d}`) and dates take strftime (`{start:%b %Y}`). A paragraph that comes out empty is dropped, so events lacking a detail can set it to `""`.
>
> **Dates** can be a year (`1066`), a year-month, a date, or a date-time with or without a zone (`2024-03-05T14:32-05:00`). Years 0 and below are BCE (`-0043` is 44 BCE).
>
> **No title.** The image has none; put the title and caption in the surrounding document.
>
> **Output.** Errors, warnings, and a summary line go to stderr. Each error names the JSON path at fault, e.g. `events[3].start: invalid date "1862-13-01"`; fix the input and run again. Crowded timelines are still drawn, with a warning per problem and a hint line about how to make room. Look at the PNG to check it.

## What makes the tool easy for Claude to use

1. **Self-describing.** `--help`, `--print-example`, `--print-default-style`, and `--schema` let Claude learn the formats by running the tool, so the description can stay short. The printed default style is itself a valid style file to edit.
2. **Forgiving input, strict validation.** JSON5 tolerates small syntax slips. Unknown keys are errors with a "did you mean" hint (`marker_colour`), and keys that were removed say why (`title`), because silently ignoring a key is the worst failure for an agent.
3. **Actionable errors, all at once.** Every error gives the JSON path, the bad value, and what's expected. A problem that affects every event is reported once ("in all 12 events").
4. **Deterministic output.** The fonts are embedded and rendering is on the CPU. Layout search effort is capped by counting work, not time. The same input gives the same image everywhere, unless the style names an installed system font.
5. **A summary line,** e.g. `timeliner: wrote out.png (2376×821 px, 12 events, 10 jogs, 0 warnings)`, so the result can be checked before viewing the image.
6. **Best effort with clear warnings.** Crowding never stops the image from being written. Warnings name the events involved (`events[10] "Spotsylvania Court House"`), and a hint lists the style changes that make room.
7. **Familiar conventions.** CommonMark, Python format specs, strftime, CSS colors, and JSON Schema are all things Claude already knows.
