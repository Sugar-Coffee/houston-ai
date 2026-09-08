# Houston AI

A long-lived terminal workspace where a markdown knowledge vault and a fleet of
coding agents live in the same keyboard-driven surface.

Two things that normally live in separate windows, merged so the seam between
them costs nothing: find a note, press `i`, and its path lands in a running
agent's prompt without touching the clipboard or leaving the app.

## What it does

- **Sessions** — spawn Claude Code, Codex, Gemini, opencode or a plain shell.
  Name them, jump between them, attach and detach.
- **A board** — which agents are working, and which are blocked on you. Driven
  by real agent hooks, not by guessing from what is on screen.
- **A vault** — fuzzy-find and full-text search across your notes, rendered
  markdown, `[[wikilink]]` navigation with backlinks.
- **An editor** — modal, markdown-first, with amp's jump mode. Soft-wraps,
  continues lists, follows links, ticks tasks.
- **The bridge** — send a note's path straight into a session, or yank it.

## Running it

Needs a Rust 2024 toolchain.

```sh
cargo run --release
```

On first launch Houston creates its own vault at `~/.houston/vault/` and
touches nothing else. Already have an Obsidian vault? Press `4` for Settings,
then `e`, and point it there — nothing is copied or moved.

`tab` moves between views, `1`–`4` jump to one, and the bar along the bottom
always shows what the current context accepts. Full list in
[`docs/keybindings.md`](docs/keybindings.md).

## Documentation

| | |
|---|---|
| [`docs/vision.md`](docs/vision.md) | what this is, and what it deliberately is not |
| [`docs/architecture.md`](docs/architecture.md) | how the code fits together |
| [`docs/keybindings.md`](docs/keybindings.md) | every key |
| [`docs/roadmap.md`](docs/roadmap.md) | what is built and what is next |
| [`docs/adr/`](docs/adr/) | why the code looks the way it does |
| [`docs/build-log.md`](docs/build-log.md) | what actually happened, dead ends included |

Working on it? Start with [`CLAUDE.md`](CLAUDE.md).

## Prior art

[Chloe](https://github.com/KevinEdry/chloe) by Kevin Edry (MIT) solves the
agent-multiplexer half of this problem, and solves it well. Houston is written
from scratch rather than forked — the domain models diverge too far — but
Chloe's source is kept on hand as a reference for the hard parts, and its
hook-driven approach to tracking agent state is a design copied outright. See
[ADR-0002](docs/adr/0002-greenfield-with-chloe-as-reference.md) and the
[teardown](docs/research/chloe-teardown.md).

The editor takes its cues from [amp](https://github.com/jmacdonald/amp),
particularly jump mode — though Houston edits prose rather than code, so the
resemblance stops there ([ADR-0003](docs/adr/0003-houston-includes-a-modal-markdown-editor.md)).

## Licence

MIT.
