# Describing timeliner to Claude

## Draft description

This text could go in a system prompt, a `SKILL.md`, or a tool definition's `description`. Update it once the open questions are decided.

> **timeliner**: renders a horizontal timeline chart as a PNG.
>
> Use it when the user wants a visual timeline of dated events (history, project milestones, biographies). Don't use it for charts of quantities over time; use a plotting tool for those.
>
> Usage: `timeliner [--style=style.json] desc.json out.png`. Use `-` for the input to read from stdin, or for the output to write the PNG to stdout.
>
> `desc.json` has optional `start`/`stop` (ISO dates, the ends of the timeline; a missing end is drawn as an arrow), optional `start_label`/`stop_label`, `datetime_format` (strftime), and `events`. Each event is `{"label", "start", "stop"?, ...}` plus any extra fields you want to show. Set `shows_stop: true` (top level, or per event) to draw each event's start–stop range on the line.
>
> The text in each event's box comes from the `event_text` template in desc.json (default: the bold label, then the date). It is markdown (bold, italic, `` `code` ``, `-` and `1.` lists) with Python-style fields such as `{label}`, `{start}`, and `{casualties:,d}`. Every field the template uses must exist on every event.
>
> Run `timeliner --print-example` for a complete working example and `timeliner --print-default-style` for every style key. The image is 2376 px wide by default and exactly as tall as its contents. Read the PNG afterwards to check it.
>
> The image has no title; put the title and caption in the surrounding document.
>
> Errors name the JSON path at fault (e.g. `events[3].start: invalid date "1862-13-01"`). Fix the input and run it again.

## What makes the tool easier for Claude to use

1. **Self-describing CLI.** `--help` with a complete example, `--print-example`, `--print-default-style`, and `--schema` (JSON Schema for both files). Claude can find out how to use the tool by running it, instead of relying on a long prompt.
2. **Forgiving input, strict validation.** Accept JSON5 so small syntax slips don't fail the run. Reject unknown keys (`marker_colour`) with a "did you mean" hint, since silently ignoring them is the worst failure mode for an agent.
3. **Actionable errors.** Every error gives the JSON path, the bad value, and what's expected. Use one line per error and report all errors at once, not just the first.
4. **Deterministic output.** Embedded fonts and no GPU, so the same input gives the same pixels on every machine.
5. **Success summary on stderr.** For example: `timeliner: wrote out.png (2376×821 px, 12 events, 10 jogs, 0 warnings)`. It lets the agent check the result without opening the image, while still encouraging a visual check.
6. **Best effort with clear warnings.** If events are too crowded for clean connectors, still write the image, but print one warning per problem that names the events and gives a hint (e.g. use `event_text_place: "alternating"` or increase `image_width`). Claude can then decide whether to adjust and run again.
7. **Familiar conventions.** CommonMark markdown, Python format specs, strftime dates, CSS colors. Claude already knows all of these, so the description can stay short.
8. **Ship it as a Claude Code skill.** A `SKILL.md` containing the description above plus one example lets Claude load it only when timelines come up.
