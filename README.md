<div align="center">

# Houston

### Run a pile of coding agents without losing track of them.

[![CI](https://github.com/Sugar-Coffee/houston-ai/actions/workflows/ci.yml/badge.svg)](https://github.com/Sugar-Coffee/houston-ai/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Sugar-Coffee/houston-ai?color=blue&label=release)](https://github.com/Sugar-Coffee/houston-ai/releases/latest)
[![Licence](https://img.shields.io/github/license/Sugar-Coffee/houston-ai?color=blue)](LICENSE)
[![Platform](https://img.shields.io/badge/macOS%20%7C%20Linux-lightgrey.svg)](#install)

```sh
curl -fsSL https://raw.githubusercontent.com/Sugar-Coffee/houston-ai/main/install.sh | sh
```

<sub>**Repo is private for now, so that URL 404s.** [Build from source](#install)
meanwhile. Delete this line when it goes public.</sub>

</div>

<!--
  `brew install vhs && vhs media/sessions.tape`, then uncomment.

<p align="center"><img src="media/sessions.gif" width="900"></p>
-->

---

You have Claude Code open in four terminal tabs. One is stuck on a permission
prompt, one finished eight minutes ago, one is still churning, and the fourth
is a shell you opened for something and forgot about.

Which one needs you? No way to tell without clicking through all four, and by
the time you have, another one has stopped.

Houston puts them in one window and tells you.

## Which agent needs you

Every session has a state, and that state comes from the agent's own **hooks**,
not from squinting at its output. Working, waiting on you, idle, done.

The board is that in one glance:

```
  Needs you          Working            Shells             Finished
  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
  │ ◆ auth-fix   │   │ ● api-rewrite│   │ $ dev-server │   │ × migrations │
  │   +142 −31   │   │   +18 −4     │   │              │   │              │
  └──────────────┘   └──────────────┘   └──────────────┘   └──────────────┘
```

Only "Needs you" gets the loud colour, because it is the only column that is
asking you for something.

When a hook stops arriving, Houston says **status frozen** rather than leaving
a stale value up pretending to be current. A shell gets no agent status at all,
because it does not have one.

## Your notes are in here too

Houston has a markdown vault built in. Plain files in a folder, so Obsidian can
stay open on the same directory and neither of you will notice.

The useful part is `c`. Press it in the vault and you get an agent **running in
the vault directory**, which means it has already read your `CLAUDE.md`, your
skills, your rules about where things live. No path to type, no context to
paste. It shows up knowing the place.

`y` copies a note's path. `i` drops `@that/path` straight into a running
agent's prompt without pressing Return, so you can finish the sentence.

## Quit it and nothing is lost

Close Houston, reopen it, and your sessions come back. Names, directories,
worktrees, and the **conversations themselves**: Claude Code resumes with
`--resume`, Codex from its rollout files.

Shells come back in the directory you left them in, not the one they started
in, because Houston reads where the process actually got to. It cannot bring
back the dev server that was running in there, and does not pretend it can.

## Agents that do not tread on each other

Start a session in its own git worktree, so three agents can work on one repo
without fighting. The Worktrees view tells you which ones are in use, which
have uncommitted work sitting in them, and which are orphaned because you
deleted the repo they came from.

Press `l` on one and Houston commits it, pushes it, opens a PR and removes the
tree. Press `v` to read the diff first, which you probably should.

## The editor

Modal, vim-shaped, and deliberately not a code editor. It edits prose.

The good bit is **jump mode**: press `f`, every word on screen grows a
two-letter tag, type one and you are there. The tags sit *on top of* the text
rather than being inserted into it, so nothing shifts under the word you were
aiming at while you decide.

<!--
  `vhs media/editor.tape`

<p align="center"><img src="media/editor.gif" width="900"></p>
-->

## Oh, and

- **Select text with the mouse.** Drag inside a pane, double-click a word,
  triple-click a line. It copies on release. Your terminal cannot do this,
  because it does not know a pane from a sidebar.
- **Nineteen themes.** Dracula, Tokyo Night, Catppuccin, Gruvbox, Rosé Pine,
  Kanagawa and friends. The picker repaints the whole app as you scroll it.
- **Nothing is deleted without asking**, and the question tells you what you
  are about to lose.
- **Paste is instant**, even a 200 line paste, because it is one write rather
  than 200 keystrokes.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/Sugar-Coffee/houston-ai/main/install.sh | sh
```

Works out your platform, grabs the right build, checks the checksum, drops it
in `~/.local/bin`. macOS (both chips) and Linux x86-64. Later on,
`houston update` does the same thing over the top.

> **While the repo is private** neither of those can reach GitHub, so build it
> yourself:
>
> ```sh
> git clone https://github.com/Sugar-Coffee/houston-ai
> cd houston-ai && cargo build --release && ./target/release/houston
> ```

## Setting up a vault worth having

On first run Houston makes one at `~/.houston/vault/` and touches nothing else.
Already have an Obsidian vault? Point Settings at it. Nothing is copied, moved
or converted.

A vault earns its keep when agents both *read* it and *write back to it*. That
takes a bit of shape. Roughly:

```
vault/
├── CLAUDE.md              how agents should use this vault
├── log.md                 global work log, newest first
│
├── Projects/
│   └── acme-api/
│       ├── index.md       what it is, where things live, who cares
│       ├── build-log.md   what happened here, newest first
│       ├── decisions/     one file per decision, numbered
│       └── research/      measurements, dated
│
├── Knowledge/             durable reference that outlives any project
├── Daily/                 one note per day
└── Archive/               finished, kept because search does not care
```

The bit that matters is that **each project carries its own log, decisions and
research**. An agent starting work reads `index.md` and `decisions/`. An agent
finishing work appends to `build-log.md`.

Two habits make the whole thing worth having:

Write the log entry in the same commit as the work, and only when it says
something the diff cannot. A dead end, a measurement that settled an argument,
a library that behaved unexpectedly.

Record the losing argument in a decision, not just the winning one. Somebody
will re-derive it otherwise, and they deserve to know it was already
considered.

Then tell your projects the vault exists. Something like this in a project's
`CLAUDE.md`:

```markdown
## Knowledge base

Notes for this project live in `~/houston-vault/Projects/acme-api/`.

Before starting: read `index.md`, and `decisions/` before proposing anything
architectural. Most big questions have been argued already.

When finishing: append to `build-log.md` in the same commit as the work. Only
write an entry if it says something the diff cannot.
```

That is the whole trick. The vault stops being somewhere you file things and
becomes somewhere your agents work.

## Keys

`tab` between views, `1`–`5` to jump. The bar at the bottom always shows what
the current context accepts, so there is nothing to memorise.

<details>
<summary>But here are the good ones</summary>

| | |
|---|---|
| `n` · `s` | new agent · new shell |
| `↵` · `ctrl-\` | attach · detach |
| `v` · `c` | review the diff · copy text out |
| `2` then `c` | agent in the vault |
| `2` then `/` | find a note (picking one shows you where it lives) |
| `f` in the editor | jump mode |
| `l` in Worktrees | land it |
| `q q` | quit, twice, so one keystroke cannot take down a workspace |

Everything else: [`docs/keybindings.md`](docs/keybindings.md).

</details>

## Status

Pre-release, and used every day by the person who wrote it. Rust,
[ratatui](https://ratatui.rs), and
[alacritty_terminal](https://github.com/alacritty/alacritty) doing the terminal
emulation. `unsafe_code = "forbid"`, clippy pedantic clean, ~430 tests, CI on
macOS and Linux.

Jump mode is [amp](https://github.com/jmacdonald/amp)'s idea.
[Chloe](https://github.com/KevinEdry/chloe) worked out that agent status should
come from hooks, and Houston does it the same way.

MIT. See [LICENSE](LICENSE).
