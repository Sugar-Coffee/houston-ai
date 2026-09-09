<div align="center">

# Houston

**A workspace for developing with AI agents — in your terminal.**

Run several coding agents at once, see at a glance which one is waiting on you,
and keep the knowledge base they build in the same window.

</div>

<!-- Screenshots: sessions view, board, vault, editor. -->

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

### The knowledge base, as a first-class pane

Fuzzy-find across your notes, search inside them, follow `[[wikilinks]]` and
their backlinks, read them rendered. Press `y` to copy a note's path, or `i` to
drop `@path` straight into a running agent's prompt.

The point is not that agents can otherwise not read your notes — of course they
can. It is that the notes stop being a second application you tab away to, and
become a pane you work in while the agents run.

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

## Getting started

Needs a Rust 2024 toolchain.

```sh
git clone https://github.com/Sugar-Coffee/houston-ai
cd houston-ai
cargo run --release
```

`tab` moves between views, `1`–`4` jump to one, and **the bar along the bottom
always shows what the current context accepts**. You should not need to
memorise anything.

<details>
<summary>The keys worth knowing</summary>

**Anywhere**

| | |
|---|---|
| `tab` · `1`–`4` | next view · jump to one |
| `q q` | quit — twice, so one keystroke cannot take down a workspace |
| `ctrl-g` | show what your terminal is actually sending |

**Sessions**

| | |
|---|---|
| `n` · `s` | new agent (asks where, and whether to make a worktree) · new shell |
| `↵` · `ctrl-\` | attach · detach |
| `r` · `x` | rename · close |
| `u` `d` `g` `G` | scroll a session's output |
| `W` | the worktree manager |

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

Built with Rust, [ratatui](https://ratatui.rs) and
[alacritty_terminal](https://github.com/alacritty/alacritty).
`unsafe_code = "forbid"`, clippy pedantic clean, ~290 tests.

## Credits

Jump mode is [amp](https://github.com/jmacdonald/amp)'s idea.
[Chloe](https://github.com/KevinEdry/chloe) got hook-driven agent status right,
and Houston does it the same way.

## Licence

MIT. See [LICENSE](LICENSE).
