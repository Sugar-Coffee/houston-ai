# Houston AI

A long-lived terminal workspace where a markdown knowledge vault and a fleet of
coding agents live in the same keyboard-driven surface.

**Status: planning.** No code yet. Start with [`docs/`](docs/).

## The idea

Two apps that cannot see each other, merged into one:

- An **Obsidian vault** holding the project knowledge — workshops, docs, daily
  logs, decisions.
- A **session multiplexer** for Claude Code and friends — spawn, name, jump
  between, and see at a glance which agents are blocked on you.

The point is the seam between them. Finding a note and getting it into an
agent's context should cost nothing: yank the path, or push `@path` straight
into the focused session, without leaving the keyboard or the app.

See [`docs/vision.md`](docs/vision.md) for what this is, and more importantly
what it deliberately is not.

## Prior art

Houston is planned as a hard fork of [Chloe](https://github.com/KevinEdry/chloe)
by Kevin Edry (MIT), which solves the agent-multiplexer half well. The
terminal emulation, PTY handling, event loop and hook-driven agent state
tracking are its work. See
[`docs/adr/0002-fork-chloe.md`](docs/adr/0002-fork-chloe.md) and
[`docs/research/chloe-teardown.md`](docs/research/chloe-teardown.md).

Navigation feel takes cues from [amp](https://github.com/jmacdonald/amp).

## Licence

MIT. Chloe's original notice is preserved in `LICENSE-CHLOE` once the fork
lands.
