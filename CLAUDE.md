# Houston AI — agent instructions

## Before you touch anything

1. [`docs/vision.md`](docs/vision.md) — what this is, and what it deliberately
   is not. The "what it is not" list is the load-bearing half.
2. [`docs/architecture.md`](docs/architecture.md) — how the code fits together,
   and the invariants not to break.
3. [`docs/roadmap.md`](docs/roadmap.md) — what is built, what is next, what is
   parked.
4. [`docs/adr/`](docs/adr/) — **before proposing any architectural change.**
   Most big questions have already been argued and the reasoning recorded,
   including the arguments that lost.

## What this project is

A terminal workspace merging a markdown knowledge vault with a multiplexer for
coding-agent sessions. The vault half is a **context bridge** first (ADR-0004)
and a markdown editor second (ADR-0003). Houston is **not** an Obsidian
replacement, **not** a code editor, and **not** a general-purpose multiplexer.

Written greenfield (ADR-0002). Chloe lives at `reference/chloe/` as a reference
implementation — read it when stuck on terminal emulation, PTY handling or hook
ingress. Do not paste from it by reflex; if you lift code verbatim, follow the
attribution rules in ADR-0002.

## The documentation loop

This project is built across many sessions with no shared memory. The docs are
how the next session knows what happened. **They are part of the work, not a
write-up afterwards.**

### Every change

Append to [`docs/build-log.md`](docs/build-log.md) **in the same commit as the
code**, newest first. An entry is worth writing if it contains something the
next person cannot read off the diff:

- what you tried that did not work, and why
- a bug that a test could not have caught, and what did catch it
- a measurement that decided something
- a tool or library that behaved unexpectedly

Do not write an entry that just restates the commit message.

### Every non-obvious decision

A new numbered ADR in `docs/adr/`, using the shape of the existing ones:
Context, Decision, Rationale, Consequences. Record the argument that lost as
well as the one that won — a future agent will re-derive it otherwise and
should be able to see it was considered.

ADRs are immutable once **Accepted**. To change one, write a new ADR that
supersedes it and update the old one's status line. The exception is adding
*evidence* to an accepted decision, which strengthens rather than changes it.

### Whenever a claim is load-bearing

Measure it and put the numbers in `docs/research/`. "This is slow" needs a
benchmark. "Nobody uses this" needs a file count. Two features were cut and one
was promoted on the strength of `grep` counts — and the first version of those
counts was **wrong**, which is why the correction is recorded at the top of
`vault-profile.md` rather than quietly fixed.

### Keep these current or delete them

`README.md`, `docs/keybindings.md` and `docs/architecture.md` describe the app
as it is. A stale one is worse than none. If you add a key, it goes in
`keybindings.md` in the same change.

## Working agreements

- **Absolute dates.** `2026-09-08`, never "last week".
- **Scope discipline.** The roadmap has a `Parked` section. If a good idea does
  not serve the north star in `docs/vision.md`, park it rather than build it.
- **Do not reorder the roadmap.** Phase 5 (the bridge) ships before Phase 7
  (the editor) on purpose. The editor is bigger and more fun and will eat the
  project if allowed to jump the queue. See ADR-0003's risk section.
- **Report honestly.** If a test is flaky, say so and fix the flake — a test
  that passes on a re-run is a bug, not a pass. If something is half-built, the
  roadmap says `wip`, not `done`.

## Code conventions

- Rust 2024, toolchain 1.98.1. `unsafe_code = "forbid"`.
- **Done means:** `cargo test` green, `cargo clippy --all-targets` clean under
  `pedantic` + `nursery`, and `cargo fmt --check` clean. Not "compiles".
- Match the surrounding style rather than importing your own.
- Comments explain *why*, and are worth writing where the reason is not
  recoverable from the code. Do not narrate what the line does.
- Test names are sentences describing the guarantee. Assertions carry a message
  saying why it matters.

### Three traps, already paid for

- **`missing_const_for_fn` (nursery) lies.** It false-positives on any method
  returning a deref-coerced borrow — `&self.query` where the field is a
  `String`, `&self.tags` where it is a `Vec`. It suggests `const fn` and the
  borrow checker then refuses to compile it. Accept where it compiles, revert
  where it does not. This has cost four separate attempts; do not make it five.
- **`unsafe_code = "forbid"` blocks `std::env::set_var` in tests.** That is the
  lint working. Make the thing under test a pure function taking the value
  rather than reading the environment — see `config::resolve_vault`.
- **Never let a test resolve a path under `$HOME`.** One did, wrote the real
  `~/.houston/config.toml`, and repointed a live install at a temp directory it
  then deleted. Pass paths in — see `Config::save_to`.

### Verifying in a real terminal

Unit tests cannot see what a terminal does. For anything decoding input or
drawing, drive the release binary through a pty:

```sh
printf 'keys' | script -q /dev/null sh -c "stty rows 40 cols 120; exec ./target/release/houston"
```

Two things that have already caught people out:

- **Escape-stripped pty output proves presence, never absence.** ratatui only
  re-emits changed cells, so a string can be split across escape sequences and
  fail a naive `grep` while being perfectly visible on screen.
- **`FOO=bar keygen | script -c houston` sets the variable on the wrong side of
  the pipe.** Houston will not see it, and the test will silently run against
  your real vault.

## Tone for shared writing

PRs, commits and user-facing copy: natural and conversational, not corporate.
Internal docs — ADRs, research, the build log — can be structured and dense.
