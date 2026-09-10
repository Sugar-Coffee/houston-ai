<div align="center">

# Houston

**A workspace for developing with AI agents — in your terminal.**

Run several coding agents at once, see at a glance which one is waiting on you,
review what each of them changed, and keep the knowledge base they build in the
same window.

[![CI](https://github.com/Sugar-Coffee/houston-ai/actions/workflows/ci.yml/badge.svg)](https://github.com/Sugar-Coffee/houston-ai/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Sugar-Coffee/houston-ai?color=blue&label=release)](https://github.com/Sugar-Coffee/houston-ai/releases/latest)
[![Licence](https://img.shields.io/github/license/Sugar-Coffee/houston-ai?color=blue)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey.svg)](#install)

```sh
curl -fsSL https://raw.githubusercontent.com/Sugar-Coffee/houston-ai/main/install.sh | sh
```

</div>

<!--
  Screenshots to add, in this order. Each wants a 120×40 terminal or wider,
  a dark theme, and at least three sessions so the board has something to say.

  1. docs/media/sessions.png  — the sidebar with two agents and a shell, one of
     them in the "needs you" state, and the diff line showing on a card.
  2. docs/media/board.png     — all four columns populated.
  3. docs/media/worktrees.png — a few worktrees, ideally one orphaned.
  4. docs/media/vault.png     — a note open beside the list.
-->

---

## What this is

If you already run Claude Code in a pile of terminal tabs, with an Obsidian
vault open beside it holding your project docs, build logs and decisions —
Houston is that workflow, purpose-built.

It is closer to an IDE than to a terminal multiplexer. Not because it edits
code, but because it is the place you sit while agents do: your sessions, their
status, and the knowledge base they read from and write back to, all on one
keyboard-driven surface.

**Terminal tabs do not scale for this.** A tab is a rectangle. It cannot tell
you that one agent has been blocked on a permission prompt for ten minutes
while another finished and is waiting for a reply. You find out by clicking
through them.

And the notes half — the vault that makes agents useful across sessions instead
of starting cold every time — lives in an entirely different application.

Houston is one surface for both.

## What it does

### Run several agents, and know where they are

Spawn Claude Code, Codex, Gemini, opencode or a plain shell. Name them, jump
between them, attach and detach. Each card shows where it is running and what
branch it is on, because with several going, *which one is this?* is answered
by the path far more often than by the name.

```
▶ 1 auth refactor          ●
    ~/Projects/acme
    ⑂ auth-refactor  worktree
```

Start a session in a **git worktree** and several agents can work the same
repository without treading on each other. Houston keeps every worktree it
creates in one place and gives you a manager to clean them up.

**Sessions survive a restart.** Quit Houston and reopen it, and your sessions
come back — same names, same directories, same worktrees. Claude Code and Codex
sessions resume the actual conversation. Scrollback is deliberately not
restored: the process is gone either way, and a shell that reopens in the right
directory is honest about that.

### A board that tells the truth

Which agents are working, which are blocked on you, which have finished their
turn. Driven by real agent hooks — not by guessing from what is on screen.

That distinction is the whole point. A hook is a fact; a screen-scrape is a
guess. Shell sessions get no agent states at all rather than invented ones, and
if Houston loses its hooks it says **status frozen** rather than showing you a
value that stopped being true an hour ago.

### Close it, reopen it, carry on

Quit Houston and every session comes back: the names you gave them, the
directories they ran in, the worktrees they were using — and, for agents that
support it, **the conversation itself**. Claude Code sessions return via
`--resume`, Codex via its rollout files.

**And the right directory means where you got to**, not where the session
started. Houston reads each child's actual working directory, so a shell you
`cd`'d three levels deep comes back three levels deep.

The honest part: a shell cannot be resumed, so it comes back as a fresh shell.
Whatever was running in it — a dev server, a `tail -f` — is not. Houston does
not pretend otherwise, and does not try to replay scrollback it would only be
guessing at.

### Get text back out

Drag inside a session pane and it copies on release. Double-click a word,
triple-click a line. Exactly what you expect from a terminal — and crucially,
*scoped to the pane*, which your terminal cannot do: it selects its own rows,
and a Houston row is the sidebar and the pane side by side.

There is a keyboard mode too — `c`, then vim motions, `v` to select, `y` to
copy — for a precise selection spanning screens, or over SSH. Both use the VT
layer's own selection, so they understand wrapped lines and wide characters.

### Review what an agent actually changed

Every session card carries what its agent has done to the working tree: `+142`
in green, `−31` in red, on its own line. `v` opens the full diff in a pane.
Files the agent created are counted too, which matters more than it sounds —
writing new files is most of what a coding agent does, and a card reading `+0`
beside six of them would teach you to distrust the number.

### Worktrees, from first checkout to landed branch

Several agents on one repository need isolation, so Houston makes git worktrees
and then owns their whole life rather than creating them and walking away. The
Worktrees view says what each one is for:

- **in use** — a live session is working in it
- **uncommitted work** — nobody is in it, and there is work in it you have not committed
- **idle** — nobody is in it, and nothing would be lost by removing it
- **repository gone** — its repository has moved or been deleted, so git can no longer act on it

That last one is why it is a view rather than a list. An orphaned worktree
cannot be landed and cannot be removed by git; it just sits on disk.

`l` lands one: commit, push, open a pull request, remove the tree. The form
names the branch and the remote, because pushing and opening a PR are visible
to other people. Removing the directory is the one step that does not default
to on.

### The knowledge base, as a first-class pane

Fuzzy-find across your notes, search inside them, follow `[[wikilinks]]` and
their backlinks, read them rendered. Press `y` to copy a note's path, or `i` to
drop `@path` straight into a running agent's prompt.

The point is not that agents can otherwise not read your notes — of course they
can. It is that the notes stop being a second application you tab away to, and
become a pane you work in while the agents run.


The sidebar is a folder tree you can work in, not just read from: `n` makes a
note and `N` a folder, `r` renames or moves one, `x` deletes with a confirmation that
tells you how many notes a folder holds. A name is a path, so `projects/kickoff`
makes the folders on the way, and a new note opens straight in the editor.

Find (`/`) and search (`f`) still show a flat list. A tree is a worse answer to
"where is the note called X".
### An editor

Modal, markdown-first, with amp-style jump mode — press `f`, every word gets a
two-character tag, type one to teleport there.

Not a code editor. It soft-wraps prose, continues your lists, follows wikilinks
and ticks task boxes. It deliberately has **no syntax highlighting**, because
colouring markdown while you write it decorates without helping.

## Who it is not for

If you run one agent at a time and do not keep notes between sessions, this is
more machinery than you need. A terminal tab is fine.

Houston earns its keep when you have several agents going at once, and when you
have decided that what they learn is worth keeping.

## The vault

The vault is a folder of plain markdown. Nothing proprietary, no database —
open it in Obsidian, edit it in vim, put it under git. Houston indexes it,
searches it, renders it, and gets it into your agents.

On first launch it creates one at `~/.houston/vault/` and touches nothing else.
Already have an Obsidian vault? Press `4` for Settings and point it there —
nothing is copied or moved.

### Laying it out

A vault earns its keep when it is both a **knowledge base** and a **work log**:
when an agent can read what was decided six weeks ago and append what it just
learned. A layout that supports that:

```
vault/
├── CLAUDE.md              how agents should use this vault
├── log.md                 global work log, newest first
│
├── Daily/                 one note per day: what happened, what is next
│   └── 2026-09-09.md
│
├── Projects/
│   └── acme-api/
│       ├── index.md       what it is, where things live, who cares
│       ├── build-log.md   what happened on this project, newest first
│       ├── decisions/     one file per decision, numbered
│       │   └── 0001-postgres-over-dynamo.md
│       └── research/      measurements and findings, dated
│
├── Knowledge/             durable reference that outlives any project
│   ├── systems/           how the infrastructure actually works
│   ├── people/            who owns what
│   └── howto/
│
├── Workshop/              long-running thinking, half-formed on purpose
├── Inbox/                 unsorted, to be filed
└── Archive/               finished, kept because search does not care
```

The shape that matters is **project folders each carrying their own log,
decisions and research**, plus a global log across all of them. An agent
starting work reads `index.md` and `decisions/`; an agent finishing work
appends to `build-log.md`. Everything else is preference.

Two habits make it worth having at all:

- **Write the log entry in the same commit as the work**, not afterwards, and
  only when it says something the diff cannot — what you tried that failed, a
  measurement that decided something, a library that behaved unexpectedly.
- **Record the losing argument** in a decision, not just the winning one.
  Someone will re-derive it otherwise, and should be able to see it was already
  considered.

### Telling your projects about it

For agents to use the vault on their own — reading context before they start,
writing back what they learned — each project needs to know it exists. Add
something like this to your project's `CLAUDE.md`:

```markdown
## Knowledge base

This project's notes live in `~/houston-vault/Projects/acme-api/`.

**Before starting:** read `index.md`, and `decisions/` before proposing any
architectural change — most big questions have been argued already.

**When finishing:** append to `build-log.md` in the same commit as the work.
Newest first, dated absolutely. Write an entry only if it says something the
diff cannot: a dead end, a measurement that decided something, a tool that
behaved unexpectedly.

**New non-obvious decision:** a numbered file in `decisions/`, recording the
argument that lost as well as the one that won.

**Any load-bearing claim:** measure it, and put the numbers in `research/`.
"This is slow" needs a benchmark. "Nobody uses this" needs a count.
```

Adjust the paths and the rules to taste. The point is that the vault stops
being somewhere *you* file things and becomes somewhere your agents work.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/Sugar-Coffee/houston-ai/main/install.sh | sh
```

Detects your platform, downloads the matching build from the latest release,
**verifies its checksum**, and installs to `~/.local/bin`. macOS on Apple
silicon or Intel, and Linux on x86-64.

<details>
<summary>Other ways</summary>

Somewhere else on disk:

```sh
curl -fsSL https://raw.githubusercontent.com/Sugar-Coffee/houston-ai/main/install.sh \
  | HOUSTON_INSTALL_DIR=/usr/local/bin sh
```

A specific version:

```sh
curl -fsSL https://raw.githubusercontent.com/Sugar-Coffee/houston-ai/main/install.sh \
  | HOUSTON_VERSION=v0.1.0 sh
```

From source, which needs a Rust 2024 toolchain:

```sh
cargo install --git https://github.com/Sugar-Coffee/houston-ai
```

Or to hack on it:

```sh
git clone https://github.com/Sugar-Coffee/houston-ai
cd houston-ai
cargo run --release
```

### Updating

Houston checks GitHub once a day, on a background thread, and says so quietly
in the tab strip if there is a newer version. Then:

```sh
houston update
```

It installs over whichever copy you are running — including one from
`cargo install` — rather than leaving a second binary somewhere else on your
`PATH`. Nothing downloads or replaces itself without you asking: a workspace
holding half a dozen live agent sessions is the wrong place for a surprise.

**Piping a script into a shell is a thing worth being suspicious of.**
[Read it first](install.sh) — it is a hundred lines of POSIX sh, and it is
short on purpose so that reading it is realistic.

</details>

## Getting started

`tab` moves between views, `1`–`5` jump to one, and **the bar along the bottom
always shows what the current context accepts**. You should not need to
memorise anything.

<details>
<summary>The keys worth knowing</summary>

**Anywhere**

| | |
|---|---|
| `tab` · `1`–`5` | next view · jump to one |
| `q q` | quit — twice, so one keystroke cannot take down a workspace |
| `ctrl-g` | show what your terminal is actually sending |

**Sessions**

| | |
|---|---|
| `n` · `s` | new agent (asks where, and whether to make a worktree) · new shell |
| `↵` · `ctrl-\` | attach · detach |
| `v` · `c` | review its diff · copy text out of its output |
| `r` · `x` | rename · close |
| `u` `d` `g` `G` | scroll a session's output |

**Worktrees**

| | |
|---|---|
| `↵` · `v` | go to the session using it · review its diff |
| `l` | land — commit, push, open a pull request, remove |
| `d` | remove — asks first if there is uncommitted work in it |

**Vault**

| | |
|---|---|
| `/` · `f` | find by name · search inside notes |
| `↵` · `e` | open · edit |
| `y` · `i` | copy the path · send it to a session |
| `l` · `b` | links out · backlinks |

**Editor**

| | |
|---|---|
| `i` `a` `o` | insert here · after · on a new line |
| `f` | jump mode — type a tag to teleport |
| `[` `]` · `↵` | previous/next heading · follow the link under the cursor |
| `t` · `s` | toggle a task · save |

</details>

## Themes

Nineteen ship. Press `↵` on the Theme row in Settings and the app repaints as
you move through the list, so you pick by looking rather than by reading a
name and hoping. `esc` puts back what you had.

Dark: Dracula, Monokai, Nord, One Dark, Tokyo Night, Catppuccin Mocha, Gruvbox
Dark, Solarized Dark, Rosé Pine, Kanagawa, Everforest, Ayu Dark, Night Owl,
GitHub Dark, Mono. Light: Solarized Light, Catppuccin Latte, GitHub Light,
Gruvbox Light.

These are homages rather than ports — Houston has fifteen colour roles and a
syntax theme has hundreds of scopes, so each one is a reading of a familiar
palette against *our* meanings. A few are nudged for legibility, and a test
enforces a contrast floor so no palette ships with text you cannot read.
[`ATTRIBUTIONS.md`](ATTRIBUTIONS.md) has the credits and the details.

Every hue means exactly one thing, everywhere: purple is *you are here*, orange
is *this wants you*, green is *live*, cyan is *followable*. That is what stops
seven colours reading as a rainbow, and it is why a new theme is a remap rather
than a redesign.

### Powerline separators

Turn on **Powerline separators** in Settings and the tab strip becomes flowing
arrow-shaped segments, branch names take the powerline branch glyph, and a
session's diff becomes a green-into-red segment pair.

It needs a powerline-patched font, which is a smaller ask than it sounds —
[powerline/fonts](https://github.com/powerline/fonts) has patched a couple of
dozen ordinary families, and a [Nerd Font](https://www.nerdfonts.com/) works
too. **Houston will tell you whether you have one.** The setting scans your
installed fonts, prints the glyphs so you can see them for yourself, and offers
an Install row that downloads one patched font if you do not.

One thing it cannot do for you: change which font your terminal uses. That
lives in the terminal's own settings and is different for every one of them, so
after installing you still have to point iTerm2 (or Ghostty, or WezTerm) at it.
The keybindings doc lists where to find that for the common ones.

It is off by default. A terminal without the glyphs renders replacement boxes,
and that does not degrade into "plain", it degrades into "broken".

Your own themes are `.toml` files in `~/.houston/themes/`, and a documented
template is written there on first run. Every field is optional, so overriding
three colours takes three lines. A file named after a built-in replaces it, so
if you want one of the palettes above exactly as its authors made it, that is
where to put it.

## Status

**Pre-release, and in daily use by its author.** It works, it is tested, and it
is not finished. Expect rough edges, and expect things to move.

Built with Rust, [ratatui](https://ratatui.rs) and
[alacritty_terminal](https://github.com/alacritty/alacritty).
`unsafe_code = "forbid"`, clippy pedantic and nursery clean, ~390 tests, CI on
macOS and Linux.

Nothing destructive happens without a question that names what would be lost,
and no keybinding is a lone capital — a capital on its own is either something
dangerous hiding behind shift or a shortcut for something already reachable.

## Credits

Jump mode is [amp](https://github.com/jmacdonald/amp)'s idea.
[Chloe](https://github.com/KevinEdry/chloe) got hook-driven agent status right,
and Houston does it the same way.

## Licence

MIT. See [LICENSE](LICENSE).
