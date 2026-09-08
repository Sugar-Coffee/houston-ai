# Houston AI — agent instructions

## Before you do anything

Read [`docs/README.md`](docs/README.md), then `docs/vision.md` and
`docs/roadmap.md`. Check `docs/adr/` before proposing an architectural change —
most big questions have already been argued, with reasoning recorded.

## What this project is

A terminal workspace merging a markdown knowledge vault with a multiplexer for
coding-agent sessions. The vault half is a **context bridge**, not a document
reader (ADR-0004). Houston is **not** a text editor (ADR-0003) and **not** an
Obsidian replacement.

## Working agreements

- **Document as you go.** Append to `docs/build-log.md` in the same change that
  does the work, not afterwards. Include dead ends — they are the expensive
  part.
- **New decision, new ADR.** Sequentially numbered, in `docs/adr/`. Never edit
  an accepted ADR; supersede it.
- **Evidence over assertion.** "This is slow" needs a benchmark. "Nobody uses
  this" needs a file count. See `docs/research/vault-profile.md` for the
  standard.
- **Absolute dates.** `2026-09-08`, never "last week".
- **Scope discipline.** The roadmap has a `Parked` section. If a good idea does
  not serve the north star in `docs/vision.md`, park it there rather than
  building it.

## Code conventions (once code exists)

- Rust 2024. `unsafe_code = "forbid"`. Clippy `pedantic` + `nursery` clean.
- Inherited from the Chloe fork — match the surrounding style rather than
  importing your own.

## Tone for shared writing

PRs, commits and user-facing copy: natural and conversational, not corporate.
Internal docs (plans, ADRs, research) can be structured and dense.
