# Houston — working agreements

Rules for anyone, human or agent, changing this codebase.

## What this project is

A terminal workspace merging a markdown knowledge vault with a multiplexer for
coding-agent sessions. The vault half is a **context bridge** first and a
markdown editor second.

It is **not** an Obsidian replacement, **not** a code editor, and **not** a
general-purpose terminal multiplexer. Those three sentences have prevented more
bad features than any amount of planning.

## Done means done

- `cargo test` green
- `cargo clippy --all-targets` clean under `pedantic` + `nursery`
- `cargo fmt --check` clean

Not "it compiles".

## Code conventions

- Rust 2024. `unsafe_code = "forbid"`.
- Match the surrounding style rather than importing your own.
- Comments explain **why**, and only where the reason is not recoverable from
  the code. Do not narrate what the line does.
- Test names are sentences describing the guarantee. Assertions carry a message
  saying why it matters.
- Absolute dates in any note or comment. `2026-09-09`, never "last week".

## Things that are true about this codebase

Each of these was paid for once. Do not pay again.

**Input focus is an enum, not a set of booleans.** `App::focus()` decides where
every keystroke goes, in one place. Add a variant; never add another boolean.
The predecessor was three predicates, and forgetting one meant `q` quitting the
app while someone typed a name.

**There is exactly one `terminal.draw()`,** in the frame tick. Events set
`app.dirty`; the tick draws. An agent emitting thousands of lines a second must
cost one repaint per frame, not one per line.

**`ui::layout` is the only source of screen geometry.** The event loop uses it
to size PTY grids and the editor viewport, so a layout change cannot
desynchronise them.

**Agent state comes from hooks, never from parsing terminal output.** A hook is
a fact; a screen-scrape is a guess. Shell sessions get no agent states at all
rather than invented ones. In `hooks::EVENTS`, two entries carry the design:
`Stop` means *idle* — it fires when a turn ends — and `PostToolUse` is what
clears an attention state, because nothing fires when a permission is approved.

**Scrollback lines have negative line numbers.** `display_iter` starts at
`Line(-display_offset - 1)`, so a grid line maps to a viewport row by adding
the offset back on. `u16::try_from` on the raw line silently drops every
history row, which looks like the bottom of the screen emptying out.

**Selection is a filled row; state is a colour.** Two questions on two
properties, so a row can be both selected and urgent without either losing.

**Every hue means exactly one thing** — see `ui::theme`. If a new element seems
to need a new hue, it almost certainly does not need colour. Reach for `dim`.

## Three traps

- **`missing_const_for_fn` (nursery) lies.** It false-positives on any method
  returning a deref-coerced borrow — `&self.query` on a `String` field,
  `&self.tags` on a `Vec`. It suggests `const fn`; the borrow checker then
  refuses. Accept where it compiles, revert where it does not. This has cost
  four attempts. Do not make it five.
- **`unsafe_code = "forbid"` blocks `std::env::set_var` in tests.** That is the
  lint working. Make the thing under test a pure function taking the value
  rather than reading the environment.
- **Never let a test resolve a path under `$HOME`.** One did, wrote the real
  `~/.houston/config.toml`, and repointed a live install at a temp directory it
  then deleted. Pass paths in — see `Config::save_to` and `worktree::create_in`.

## Verifying in a real terminal

Unit tests cannot see what a terminal does. For anything decoding input or
drawing, drive the release binary through a pty:

```sh
printf 'keys' | script -q /dev/null sh -c "stty rows 40 cols 120; exec ./target/release/houston"
```

Three things that have already caught people out:

- **Escape-stripped pty output proves presence, never absence.** ratatui only
  re-emits changed cells, so a string can be split across escapes and fail a
  naive `grep` while being perfectly visible.
- **A cumulative capture holds every frame ever drawn.** Finding a line in it
  says nothing about what is on screen now. Use a differential: capture with
  and without the action, and compare.
- **`FOO=bar keygen | script -c houston` sets the variable on the wrong side of
  the pipe.** Houston will not see it, and your test will silently run against
  the real vault.

**Never verify a rendering bug through the state that drives the render.** A
scrollback bug shipped twice because the indicator — which reads the state
directly — was correct the whole time. Assert on the frame.

## Two habits worth keeping

**Evidence over assertion.** "This is slow" needs a benchmark; "nobody uses
this" needs a count. Two features were cut and one was promoted on the strength
of `grep` counts over a real vault — and the first version of those counts was
wrong, which is the other half of the lesson.

**A test that passes on a re-run is a bug, not a pass.** Find the shared state.

## Working notes

`docs/` holds the vision, ADRs, research and build log. It is **gitignored** —
kept locally for whoever is developing, not published. If it is present, read
it before proposing an architectural change; most big questions have been
argued already, with the losing arguments recorded.

## Tone

PRs, commits and user-facing copy: natural and conversational, not corporate.
Internal notes can be dense.
