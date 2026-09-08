<div align="center">

# Houston

**A terminal workspace where your notes and your coding agents share one surface.**

Run several Claude Code sessions, see at a glance which ones are blocked on you,
and send a note straight into an agent's context without leaving the keyboard.

</div>

<!-- Screenshots go here: the sessions view, the board, the vault, the editor. -->

---

## Why

If you work with coding agents, you probably have two windows open. One holds
the agents. The other holds the notes that give them context — the project
docs, the decisions, the running log.

They cannot see each other. So every time an agent needs something from your
notes, you alt-tab, find the file, copy a path, tab back, paste. It is a small
tax, and you pay it constantly.

Houston puts both in one place and makes that seam free.

## What it does

### Sessions

Spawn Claude Code, Codex, Gemini, opencode or a plain shell. Name them, jump
between them, attach and detach. Each one shows where it is running and what
branch it is on, because with several agents going, *which one is this?* is
answered by the path far more often than by the name.

```
▶ 1 auth refactor          ●
    ~/Projects/acme
    ⑂ auth-refactor  worktree
```

Start a session in a **git worktree** and several agents can work the same
repository without treading on each other. Houston keeps them in one place and
gives you a manager to clean them up.

### A board that tells the truth

Which agents are working, which are blocked on you, which have finished their
turn. Driven by real agent hooks — not by guessing from what is on screen.

That distinction matters. A hook is a fact; a screen-scrape is a guess. Shell
sessions get no agent states at all rather than invented ones, and if Houston
loses its hooks it says **status frozen** rather than showing you a value that
stopped being true an hour ago.

### A vault

Point it at a folder of markdown — an Obsidian vault, or the one Houston
creates for you. Fuzzy-find across it, search inside it, follow `[[wikilinks]]`
and their backlinks, read it rendered.

It is a **context bridge** first and a reader second. Press `i` on a note and
its path lands in a running agent's prompt. No clipboard, no alt-tab.

### An editor

Modal, markdown-first, with amp's jump mode — press `f`, every word gets a
two-character tag, type one to teleport there.

Not a code editor: it soft-wraps prose, continues your lists, follows wikilinks
and ticks task boxes. It deliberately has no syntax highlighting, because
colouring markdown while you write it decorates without helping.

## Getting started

Needs a Rust 2024 toolchain.

```sh
git clone https://github.com/Sugar-Coffee/houston-ai
cd houston-ai
cargo run --release
```

On first launch Houston creates a vault at `~/.houston/vault/` and touches
nothing else. Already have an Obsidian vault? Press `4` for Settings and point
it there — nothing is copied or moved.

`tab` moves between views, `1`–`4` jump to one, and **the bar along the bottom
always shows what the current context accepts**. You should not need to
memorise anything.

<details>
<summary>The keys worth knowing</summary>

| | |
|---|---|
| `n` / `s` | new agent · new shell |
| `↵` / `ctrl-\` | attach · detach |
| `r` | rename a session |
| `W` | the worktree manager |
| `/` · `f` | find a note by name · search inside notes |
| `y` · `i` | copy a note's path · send it to a session |
| `e` | edit a note |
| `q q` | quit — twice, so one keystroke cannot take down a workspace |
| `ctrl-g` | show what your terminal is actually sending |

Full list: [`docs/keybindings.md`](docs/keybindings.md).

</details>

## Themes

Five ship — Dracula, Monokai, Nord, Light and Mono — and Settings switches
between them live.

Every hue means exactly one thing, everywhere: purple is *you are here*, orange
is *this wants you*, green is *live*, cyan is *followable*. That is what stops
seven colours reading as a rainbow, and it is why a new theme is a remap rather
than a redesign.

Your own themes are `.toml` files in `~/.houston/themes/`, and a documented
template is written there on first run. Every field is optional, so overriding
three colours takes three lines.

## Status

**Pre-release, and in daily use by its author.** It works, it is tested, and it
is not finished. Expect rough edges, and expect things to move.

Known gaps are tracked honestly in [`docs/roadmap.md`](docs/roadmap.md).

## How it is built

Rust, [ratatui](https://ratatui.rs), and
[alacritty_terminal](https://github.com/alacritty/alacritty) for the terminal
emulation. `unsafe_code = "forbid"`, clippy pedantic clean, ~290 tests.

The reasoning behind the design is written down rather than lost:

| | |
|---|---|
| [`docs/vision.md`](docs/vision.md) | what this is, and what it deliberately is not |
| [`docs/architecture.md`](docs/architecture.md) | how the code fits together |
| [`docs/adr/`](docs/adr/) | why it looks the way it does, losing arguments included |
| [`docs/build-log.md`](docs/build-log.md) | what actually happened, dead ends and all |

## Prior art

[Chloe](https://github.com/KevinEdry/chloe) by Kevin Edry (MIT) solves the
agent-multiplexer half of this problem, and solves it well. Houston is written
from scratch rather than forked — the domain models diverge too far — but its
hook-driven approach to tracking agent state is a design copied outright.

The editor takes its cues from [amp](https://github.com/jmacdonald/amp),
particularly jump mode.

## Licence

MIT. See [LICENSE](LICENSE).
