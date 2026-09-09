# Where the themes come from

Houston ships nineteen colour schemes. Fifteen of them are named after
well-known palettes, and this file says where each one came from.

## What these are

**Interpretations, not ports.** Houston has fifteen colour roles. A VS Code or
Vim theme has hundreds of syntax scopes, and there is no mechanical way to turn
one into the other. Each palette below was read against *Houston's* meanings —
which colour says "this agent wants you", which says "this failed", which
carries a file path that has to stay legible — and the result is a reading, not
a conversion.

Concretely, that means:

- No theme file from any of these projects is copied, bundled, or redistributed
  here. The values were transcribed by hand.
- Several palettes are **adjusted for legibility**. Houston's `dim` role
  carries paths and branch names, so it has a 3:1 contrast floor; a few
  originals use their comment grey there, which is dimmer than that job allows.
  One Dark's `#5C6370` is the clearest case, sitting at 2.3:1 against its own
  background. `palette_tests` in `src/ui/theme.rs` enforces the floor.
- Where a palette has fewer distinct hues than Houston has roles, roles are
  doubled up or a near neighbour is chosen. Rosé Pine and Mono both do this.

If you want any of these exactly as its authors made it, write a theme file:
a `.toml` in `~/.houston/themes/` whose name matches a built-in replaces it.
See `houston --help` and the template written into that directory on first run.

## Credits

| Houston theme | After | Project |
|---|---|---|
| Dracula | Dracula | dracula/dracula-theme |
| Monokai | Monokai | Wimer Hazenberg's TextMate theme |
| Nord | Nord | nordtheme/nord |
| One Dark | One Dark | Atom's default dark syntax theme |
| Tokyo Night | Tokyo Night | enkia/tokyo-night-vscode-theme |
| Catppuccin Mocha | Catppuccin Mocha | catppuccin/catppuccin |
| Catppuccin Latte | Catppuccin Latte | catppuccin/catppuccin |
| Gruvbox Dark | gruvbox dark | morhetz/gruvbox |
| Gruvbox Light | gruvbox light | morhetz/gruvbox |
| Solarized Dark | Solarized dark | Ethan Schoonover's Solarized |
| Solarized Light | Solarized light | Ethan Schoonover's Solarized |
| Rosé Pine | Rosé Pine | rose-pine/rose-pine-theme |
| Kanagawa | Kanagawa (wave) | rebelot/kanagawa.nvim |
| Everforest | Everforest dark | sainnhe/everforest |
| Ayu Dark | Ayu dark | ayu-theme/ayu-colors |
| Night Owl | Night Owl | sdras/night-owl-vscode-theme |
| GitHub Dark | GitHub dark | GitHub's Primer palette |
| GitHub Light | GitHub light | GitHub's Primer palette |
| Mono | — | Houston's own; greys plus one accent |

Each of these projects publishes its palette openly, and most under a
permissive licence. This file exists to credit them properly. It is a record of
provenance rather than a legal review: if you are redistributing Houston in a
context where the distinction matters, check the current licence of anything
you care about at its source.

Corrections welcome — if you maintain one of these and want the credit worded
differently, or want your name off it, open an issue.
