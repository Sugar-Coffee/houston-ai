# Houston AI — agent instructions

## Before you do anything

Read [`docs/README.md`](docs/README.md), then `docs/vision.md` and
`docs/roadmap.md`. Check `docs/adr/` before proposing an architectural change —
most big questions have already been argued, with reasoning recorded.

## What this project is

A terminal workspace merging a markdown knowledge vault with a multiplexer for
coding-agent sessions. The vault half is a **context bridge** first (ADR-0004)
and an amp-style modal editor second (ADR-0003). Houston is **not** an Obsidian
replacement and **not** a general-purpose editor.

Written greenfield (ADR-0002). Chloe lives at `reference/chloe/` as a
reference implementation — read it when stuck on terminal emulation, PTY
handling or hook ingress. Do not paste from it by reflex; if you lift code
verbatim, follow the attribution rules in ADR-0002.

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
- **Do not reorder the roadmap.** Phase 5 (the bridge) ships before Phase 7
  (the editor) on purpose. The editor is bigger and more fun and will eat the
  project if allowed to jump the queue. See ADR-0003's risk section.

## Code conventions (once code exists)

- Rust 2024, toolchain 1.98.1. `unsafe_code = "forbid"`. Clippy `pedantic` +
  `nursery` clean before any change is considered done.
- Match the surrounding style rather than importing your own.

### Two clippy lints that lie to you

Both are `nursery`. Do not spend time re-deriving these.

- **`missing_const_for_fn`** false-positives on any method returning a
  deref-coerced borrow — `&self.query` where the field is a `String`,
  `&self.tags` where it is a `Vec`. It suggests `const fn`; the borrow checker
  then refuses to compile it. Accept the suggestion where it compiles, revert
  it where it does not.
- **`unsafe_code = "forbid"` blocks `std::env::set_var` in tests.** That is the
  lint working, not a problem to route around. Make the thing under test a pure
  function that takes the value instead of reading the environment — see
  `config::resolve_vault`.

## Tone for shared writing

PRs, commits and user-facing copy: natural and conversational, not corporate.
Internal docs (plans, ADRs, research) can be structured and dense.
