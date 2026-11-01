# timeliner — Plan

Status: milestones 1–3 done (2026-11-01). Milestone 4 is next.

## Milestones

0. ✅ **Spec settled.** Answer `QUESTIONS.md` and fold the answers into `SPEC.md`.
1. ✅ **Skeleton.** Add crates. `clap` CLI with `-`/`--` handling. Load the desc and style files with JSON5 and serde, merge in the default style, and validate with clear errors. Parse units and colors.
   - Done when: bad input gives a precise error message, and good input is accepted.
2. ✅ **Template and rich text.** f-string substitution plus markdown to styled blocks (paragraphs, lists, bold, italic).
   - Done when: unit tests cover the Civil War template and the edge cases (unknown field, braces, `*` in values).
3. ✅ **Text measurement.** Embedded fonts, parley layout, box sizes.
4. **Layout engine.** Pure geometry, the invariant checker, and unit tests that use fake box sizes. This includes dense clusters and events at both ends.
5. **Rendering.** `vello_cpu` scene to PNG, then render the Civil War example end to end.
   - Done when: the output is checked visually by reading the PNG.
6. **Agent ergonomics.** Write `--help` with examples, `--print-default-style`, `--print-example`, `--schema`, and stderr summaries. Finalize `TOOL_DESCRIPTION.md`.
7. **Regression tests.** Golden image tests for a few example timelines in `examples/`.

## Docs

| File | Purpose |
|---|---|
| `docs/SPEC.md` | What the tool does: CLI, input formats, rendering rules |
| `docs/DESIGN.md` | How it's built: crate choices, pipeline, layout algorithm |
| `docs/QUESTIONS.md` | Open decisions. Once a question is answered, its answer moves into SPEC or DESIGN |
| `docs/PLAN.md` | This file: milestones and status |
| `docs/TOOL_DESCRIPTION.md` | Draft text that describes the tool to Claude agents |
