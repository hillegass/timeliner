# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`timeliner` is a Rust CLI (edition 2024) that renders timeline PNGs, intended for use by Claude agents:

```
timeliner [--style=<style.json>] <desc.json> <output.png>
```

Status: all milestones in `docs/PLAN.md` are done. Add crates only when a milestone needs them.

## Architecture

- `main.rs`: the CLI (clap), reading files or stdin (`-`), JSON5 parsing (`parse_json5`), and error reporting.
- `diag.rs`: `Diagnostics` collects every error and warning instead of stopping at the first. Messages are `file: path: message`, where the path is a JSON path like `events[3].start`. Unknown keys get "did you mean" hints.
- `style.rs`: `DEFAULT_STYLE` is a JSON5 document and the single source of truth for style keys and defaults. A user's style file is laid over it key by key, so keys not in the default are rejected. All lengths are resolved to points.
- `desc.rs`: the description file. Events are validated, then cross-checked (zone mixing, range, ordering), then stably sorted by start. Extra event keys are kept in `Event::fields` for template substitution.
- `timepoint.rs`: date parsing. Every value gets an `instant` in nanoseconds since 1970 for sorting and placement. Zoneless values are measured as if they were UTC, which is valid because zoned and zoneless values can't be mixed. Year and year-month values sit at the midpoint of their period. Calendar math is our own (`days_from_civil`), so year 10000 boundaries work; jiff handles parsing, zones, and offsets.
- `content.rs`: builds each event's `RichText` from `event_text`. A field error that affects every event is reported once ("in all N events").
- `richtext.rs`: `Template::parse` turns `event_text` markdown (pulldown-cmark) into blocks of styled runs, merging adjacent same-style text and splitting it into literal and field pieces. `render` substitutes fields afterward, so values can't change formatting. Unsupported markdown is flattened to plain text.
- `template.rs`: `{field:spec}` parsing and Python's format-spec mini-language. The test cases' expected values come from Python 3's `format()`; check new cases against Python the same way.
- `datefmt.rs`: date display. strftime via jiff, but `%Y` is replaced by our own year text so years ≤ 0 read "N BCE". Partial dates without an explicit spec show only their input precision.
- `text.rs`: parley text layout in points (scale 1). Fonts in `assets/fonts/` are compiled in with `include_bytes!` and registered with fontique, so they win over system fonts with the same name. System fonts are scanned only if the style names a non-embedded family, or if a first layout finds glyphs the embedded fonts lack (`Plan::has_missing_glyphs`), in which case `main` redoes the layout with them. `Fonts::rich` lays out each markdown block as its own parley layout (list markers are separate layouts in a hanging column) and widens the wrap width to the longest word when needed.
- `plan.rs`: measures the event text and rotated end labels, then calls the layout. Its `Plan` is everything rendering needs.
- `layout.rs`: pure geometry (see `docs/DESIGN.md`, "Layout algorithm"). Tests use made-up box sizes; `random_layouts_are_sound` checks the hard rules (no overlapping boxes, inside the margins, connectors orthogonal and meeting the box's left edge) on random input. Run `cargo test --release layout` to time it; `many_events` prints the time for 60 events.
- `render.rs`: draws a `Plan` with vello_cpu, in points under a `pixels_per_point` transform, and encodes the PNG. Text is drawn from parley glyph runs. End labels are rotated −90°. vello_cpu limits images to 65,535 px per side.
- `tests/cli.rs`: end-to-end tests that run the binary.
- `tests/golden.rs`: golden-image tests. Each case in `CASES` is rendered at 1 px/pt and compared with `tests/golden/<name>.png`, tolerating small per-pixel differences between CPUs. When output changes on purpose, regenerate with `TIMELINER_UPDATE_GOLDEN=1 cargo test --test golden`, look at the new PNGs, and commit them. Failures write the actual image and a red-pixel diff to `target/golden-failures/`.
- `schema.rs`: JSON Schemas printed by `--schema`. Tests check their keys against `DEFAULT_STYLE` and `desc::TOP_KEYS`, so add new keys in both places.
- `main.rs`'s `GUIDE` is the `--help` text agents learn from; keep it, `docs/TOOL_DESCRIPTION.md`, and `skill/timeliner/SKILL.md` in sync with the formats.
- `units.rs`: length parsing (`pt`, `px`, `in`, `mm`, `cm`). A bare number is an error.
- `examples/civil-war.json5`: the reference example, also loaded by a unit test.

## Docs (read before working)

- `docs/SPEC.md`: CLI, input formats, rendering rules
- `docs/DESIGN.md`: crate choices (vello_cpu, parley, color, pulldown-cmark), pipeline, layout algorithm
- `docs/QUESTIONS.md`: open decisions. When one is answered, move it into SPEC or DESIGN
- `docs/PLAN.md`: milestones and status
- `docs/TOOL_DESCRIPTION.md`: how the tool is described to Claude agents. Keep it in sync with the CLI

## Checking output visually

Render an example and read the PNG to look at it: `cargo run --release -- examples/civil-war.json5 /tmp/out.png`.

## Commands

- Build: `cargo build`
- Run: `cargo run -- <args>`
- Test all: `cargo test`
- Run a single test: `cargo test <test_name>` (add `-- --exact` to match the full path exactly)
- Update golden images after an intended output change: `TIMELINER_UPDATE_GOLDEN=1 cargo test --test golden`
- Lint: `cargo clippy --all-targets`
- Format: `cargo fmt`
