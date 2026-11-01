---
name: timeliner
description: Render a timeline of dated events (history, milestones, biographies, call and message logs) as a PNG image with the timeliner command-line tool. Use when the user wants a visual timeline.
---

# timeliner

`timeliner [--style=style.json] desc.json out.png` draws a horizontal line with a marker for each event and a box of text per event, alternating above and below the line. Use `-` for `desc.json` to read stdin, or for `out.png` to write the PNG to stdout.

## Steps

1. Run `timeliner --help` once for the full format guide. `timeliner --print-example` prints a working description file you can adapt.
2. Write the description file (JSON5 or JSON):
   - `events` is required: a list of `{label, start, stop?, ...}`. Any other key on an event can be shown through the `event_text` template.
   - Set `start`/`stop` to frame the timeline. A missing end is drawn as an arrow.
   - Set `event_text` for what each box shows, e.g. `"**{label}**\\\n_{start}_\n\n{note}"`. In that template, `\\\n` is a markdown line break (a backslash before a newline); `\n\n` starts a new paragraph.
3. Render it, read the stderr summary, then **look at the PNG** to check it.
4. Fix any errors, which name the JSON path, and run again.

## Tips

- Don't put a title in the image (the keys don't exist). Give the title and caption in your reply or document instead.
- Dates: `1066`, `"1066-10"`, `"2024-03-05"`, `"2024-03-05T14:32-05:00"`; `-0043` is 44 BCE. Use `datetime_format` (strftime) and `time_zone` to control how they display.
- Numbers in templates take Python format specs: `{deaths:,d}`. Dates take strftime: `{start:%b %Y}`.
- An empty paragraph is dropped, so events missing a detail can set it to `""`.
- If the output warns about crowding or the image is very tall, there are three fixes:
  - Shorten `event_text`.
  - Add a style file with a smaller `event_text_size` (default `"10pt"`), a larger `max_event_width` (default `6`; boxes are at most `image_width / max_event_width` wide), or a larger `image_width`.
  - Narrow the date range.
- Style keys and defaults: `timeliner --print-default-style`. Only the keys you change are needed in a style file.
