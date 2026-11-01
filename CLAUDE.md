# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`timeliner` is a Rust CLI (edition 2024) that renders timeline PNGs, intended for use by Claude agents:

```
timeliner [--style=<style.json>] <desc.json> <output.png>
```

**Status: design phase.** Do not add crates or start implementing until Aaron explicitly says to. `src/main.rs` is still the `cargo new` stub.

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
