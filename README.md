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

[Chloe](https://github.com/KevinEdry/chloe) by Kevin Edry (MIT) solves the
agent-multiplexer half of this problem, and solves it well. Houston is written
from scratch rather than forked — the domain models diverge too far — but
Chloe's source is kept on hand as a reference for the hard parts, and its
hook-driven approach to tracking agent state is a design we're copying outright.
See [`docs/adr/0002-greenfield-with-chloe-as-reference.md`](docs/adr/0002-greenfield-with-chloe-as-reference.md)
and [`docs/research/chloe-teardown.md`](docs/research/chloe-teardown.md).

The editor and its navigation model take their cues from
[amp](https://github.com/jmacdonald/amp) — particularly jump mode.

## Licence

MIT.
