# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`timeliner` is a Rust CLI (edition 2024) that renders timeline PNGs, intended for use by Claude agents:

```
timeliner [--style=<style.json>] <desc.json> <output.png>
```

Status: milestone 1 of `docs/PLAN.md` is done: the tool loads and validates its input but doesn't render yet, and exits with code 2 after valid input. Add crates only when a milestone needs them.

## Architecture

- `main.rs`: the CLI (clap), reading files or stdin (`-`), JSON5 parsing (`parse_json5`), and error reporting.
- `diag.rs`: `Diagnostics` collects every error and warning instead of stopping at the first. Messages are `file: path: message`, where the path is a JSON path like `events[3].start`. Unknown keys get "did you mean" hints.
- `style.rs`: `DEFAULT_STYLE` is a JSON5 document and the single source of truth for style keys and defaults. A user's style file is laid over it key by key, so keys not in the default are rejected. All lengths are resolved to points.
- `desc.rs`: the description file. Events are validated, then cross-checked (zone mixing, range, ordering), then stably sorted by start. Extra event keys are kept in `Event::fields` for template substitution.
- `timepoint.rs`: date parsing. Every value gets an `instant` in nanoseconds since 1970 for sorting and placement. Zoneless values are measured as if they were UTC, which is valid because zoned and zoneless values can't be mixed. Year and year-month values sit at the midpoint of their period. Calendar math is our own (`days_from_civil`), so year 10000 boundaries work; jiff handles parsing, zones, and offsets.
- `units.rs`: length parsing (`pt`, `px`, `in`, `mm`, `cm`). A bare number is an error.
- `examples/civil-war.json5`: the reference example, also loaded by a unit test.

## Docs (read before working)

- `docs/SPEC.md`: CLI, input formats, rendering rules
- `docs/DESIGN.md`: crate choices (vello_cpu, parley, color, pulldown-cmark), pipeline, layout algorithm
- `docs/QUESTIONS.md`: open decisions. When one is answered, move it into SPEC or DESIGN
- `docs/PLAN.md`: milestones and status
- `docs/TOOL_DESCRIPTION.md`: how the tool is described to Claude agents. Keep it in sync with the CLI

## Commands

- Build: `cargo build`
- Run: `cargo run -- <args>`
- Test all: `cargo test`
- Run a single test: `cargo test <test_name>` (add `-- --exact` to match the full path exactly)
- Lint: `cargo clippy --all-targets`
- Format: `cargo fmt`
