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

## Never kill by name

`pkill -f claude`, `killall node`, `pkill -f houston` — none of these. This
machine is somebody's workstation, and those patterns match their running work,
not just yours. It has already happened once here: cleaning up agents a test
had spawned killed every Claude Code session the user had open.

Kill by pid, from a list you collected yourself:

```sh
# The pids this command started, and nothing else.
kill $MY_PID
```

If a test spawns a process, prefer not spawning it: `session::restore_spec`
exists so the resume logic can be checked without launching a real agent.

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
- **Reconstruct the frame rather than stripping escapes.** ratatui re-emits only
the cells that changed, so text arrives split around cursor moves: "all notes"
comes out as `all n`, a jump to column 8, then `tes`. Grepping a
stripped capture reports that missing and it is on screen. Apply the cursor
positioning instead — a sixty-line Python script that handles `CSI H`, `\r`,
`\n` and skips OSC is enough, and it turns every one of these checks from a
guess into a look.

**A cumulative capture holds every frame ever drawn.** Finding a line in it
  says nothing about what is on screen now. Use a differential: capture with
  and without the action, and compare.
- **`FOO=bar keygen | script -c houston` sets the variable on the wrong side of
  the pipe.** Houston will not see it, and your test will silently run against
  the real vault.

**And a pty run is not a test, so none of the `$HOME` guards apply to it.**
`cfg!(test)` branches keep the suite away from `~/.houston`; the release
binary you drive through a pty has no such branch, because for it that
directory *is* the real one. A verification run that spawned two demo sessions
wrote both into the live session list, alongside work that was actually
running. Point it somewhere harmless:

```sh
export HOUSTON_STATE_DIR=$(mktemp -d)   # config, state.json, worktrees, socket
```

Set it in the environment *before* the pipe, not in front of the producer —
see the trap directly above this one.

**A slow command is not a hang.** `cargo clippy --all-targets` recompiles the
crate and takes minutes from cold. **Do not chain clippy and tests in one timed
command** — the rebuild eats the budget and the timeout looks like a deadlock.
Run them separately, and before calling anything stuck, check the *elapsed time
of the process itself* and whether `rustc` is running. A wrong diagnosis here
sends you rewriting code that works.

**But a test that only hangs with more than one thread is a real concurrency
bug**, not a flaky harness. Both mistakes were made here in the same hour, in
opposite directions.

**And a lingering test binary is usually an orphan.** When a timed command is
killed, `cargo` dies but the test binary it spawned can survive with no parent.
Check for a `cargo` parent before believing it is stuck:

```sh
ps -o ppid= -p $(pgrep -f 'deps/<crate>-' | head -1)   # ppid 1 means orphaned
```

**Never verify a rendering bug through the state that drives the render.** A
scrollback bug shipped twice because the indicator — which reads the state
directly — was correct the whole time. Assert on the frame.

**The same trap, one level up: unit tests can all pass while the binary is
broken.** `houston notify` sent `"conversation": null` for weeks with a green
suite, because the parser was fine and the *wiring* was not — `parse` read
stdin, draining the pipe, and then `dispatch` read what was left. Nothing that
tests a function in isolation can see that. When a feature crosses a process
boundary, test it across the boundary: `tests/hook_payload.rs` spawns the real
binary. Confirm such a test fails against the bug before trusting it.

**Driving an escape sequence into the pty proves you handle it, not that
the terminal sends it.** Mouse drag selection was written, unit-tested, and
verified by writing SGR drag sequences into a pty — all of it green, and none
of it reachable, because `MOUSE_ON` asked for `?1000h` (press and release) and
not `?1002h` (motion while a button is held). Real terminals sent a press and a
release and nothing in between. When a feature depends on the terminal being
asked for something, assert on the request as well as on the handling — see
`terminal::mouse_mode_tests`.

**A test that spawns an agent only passes where one is installed.**
`spawn_agent` falls back to a shell when no coding agent CLI is found, and a
shell displaces nobody — so the hook-collision test passed on a laptop with
Claude Code and failed the first time CI ran it. `spawn_agent_as` takes the
kind as a parameter for exactly this reason, the same way `restore_spec` is
split from `restore`.

You can see what CI will see without waiting for it — but trim the `PATH`,
do not empty the environment:

```sh
cargo test --no-run
CLEAN=$(echo "$PATH" | tr ':' '\n' | grep -vE 'homebrew|\.local/bin' | paste -sd: -)
env PATH="$CLEAN" target/debug/deps/houston-<hash>
```

`env -i` was the first version of this and it lies. It drops `$SHELL`, so
`provider::login_shell` picks a different shell, so every test that reads a
spawned shell's screen fails for a reason CI will never have.

## Two habits worth keeping

**A test that reads the machine it runs on is not a test.** Three tests
asserted against whatever `$HOME` happened to hold — a vault that exists, a
default that resolves — so they passed on the laptop and failed the first time
CI ran them. Reproduce that without waiting for CI:

```sh
cargo test --no-run
env HOME=$(mktemp -d) target/debug/deps/houston-<hash>
```

It is stricter than CI, which has a real home; treat the extra failures as
noise and look only for the ones CI reported.

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
