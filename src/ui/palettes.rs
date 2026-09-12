//! The palettes Houston ships with.
//!
//! Separated from `theme` because that module is mechanism — roles, loading,
//! merging, the file format — and this is data. Mixing four hundred lines of
//! hex into it made the interesting part hard to find.
//!
//! **These are homages, not ports.** Houston has fifteen roles; a VS Code
//! theme has several hundred scopes. There is no mechanical translation, so
//! each of these is a reading of a well-known palette against *our* meanings —
//! which colour here says "this wants you", which says "this failed". Nobody
//! else's theme file is copied or redistributed. Where a project publishes a
//! named palette, the names below are the credit. See `ATTRIBUTIONS.md`.
//!
//! **Where a palette has been adjusted, it was for legibility, not taste.**
//! Houston's `dim` carries paths and branch names, so it has to be readable;
//! several originals use their comment grey there, which is dimmer than that
//! job allows. One Dark's `#5C6370` sits at 2.3:1 against its own background,
//! and is lightened here to clear 3:1. `palette_tests` is what found that, and
//! is what found the next one too: five headings below the 4.5:1 floor, with
//! Monokai's at 3.93 — which is what a heading that bleeds into the page
//! measures as, and it was reported by eye before anything measured it.
//!
//! Monokai's is the only one whose *hue* changed rather than its lightness.
//! Its heading was `#F92672`, which is also its `danger` and its `removed`:
//! three meanings on one colour, in a scheme where every hue is supposed to
//! carry exactly one. It takes the purple now, which it shares only with
//! `accent` — and accent lives in the chrome, so the two never appear side by
//! side the way a heading and a deleted line do.
//!
//! If you want the real thing exactly, a theme file overrides any of these by
//! name — see `theme::available`.

// A colour is six hex digits. Everywhere else in the world it is written that
// way, which is what makes these lines checkable against the palette they came
// from. `unreadable_literal` would have them as `0x0028_2A36`, which is
// unreadable in the only sense that matters here.
#![expect(clippy::unreadable_literal, reason = "a hex colour is read as six digits")]

use super::theme::{Kind, Named, Theme};
use ratatui::style::Color;

/// A hex literal as a colour.
///
/// `rgb(0x1A1B26)` is the value a theme author actually has in front of them.
/// The three-byte form says the same thing in three times the width, and with
/// nineteen palettes to check by eye that stopped being a fair trade.
const fn rgb(hex: u32) -> Color {
    #[expect(clippy::cast_possible_truncation, reason = "the masks are the truncation")]
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Builds a theme, defaulting `terminal_background` to the surface.
///
/// Every built-in wants that; only a theme *file* has reason to say otherwise,
/// so it is not worth a nineteenth repeated line.
#[expect(clippy::too_many_arguments, reason = "fifteen roles, named by position once")]
const fn theme(
    surface: u32,
    raised: u32,
    highlight: u32,
    text: u32,
    dim: u32,
    accent: u32,
    attention: u32,
    running: u32,
    danger: u32,
    heading: u32,
    link: u32,
    code: u32,
    added: u32,
    removed: u32,
) -> Theme {
    Theme {
        surface: rgb(surface),
        raised: rgb(raised),
        highlight: rgb(highlight),
        text: rgb(text),
        dim: rgb(dim),
        accent: rgb(accent),
        attention: rgb(attention),
        running: rgb(running),
        danger: rgb(danger),
        heading: rgb(heading),
        link: rgb(link),
        code: rgb(code),
        added: rgb(added),
        removed: rgb(removed),
        terminal_background: rgb(surface),
    }
}

/// One built-in: what it is called, whether it is dark, and its palette.
struct Builtin {
    name: &'static str,
    kind: Kind,
    theme: Theme,
}

/// Every theme shipped with Houston, in the order the picker offers them.
///
/// Ordered by kind, then roughly by how often you meet the palette elsewhere.
/// `Mono` sits at the end of the dark block because it is a deliberate
/// exception rather than a choice most people want first.
#[rustfmt::skip]
const BUILT_IN: &[Builtin] = &[
    // ── Dark ─────────────────────────────────────────────────────────────
    //                    surface   raised    highlt    text      dim       accent    attentn   running   danger    heading   link      code      added     removed
    Builtin { name: "Dracula",          kind: Kind::Dark, theme: theme(
                          0x282A36, 0x343746, 0x44475A, 0xF8F8F2, 0x6272A4, 0xBD93F9, 0xFFB86C, 0x50FA7B, 0xFF5555, 0xFF79C6, 0x8BE9FD, 0xF1FA8C, 0x50FA7B, 0xFF5555) },
    Builtin { name: "Monokai",          kind: Kind::Dark, theme: theme(
                          0x272822, 0x33342C, 0x49483E, 0xF8F8F2, 0x75715E, 0xAE81FF, 0xFD971F, 0xA6E22E, 0xF92672, 0xAE81FF, 0x66D9EF, 0xE6DB74, 0xA6E22E, 0xF92672) },
    Builtin { name: "Nord",             kind: Kind::Dark, theme: theme(
                          0x2E3440, 0x3B4252, 0x434C5E, 0xECEFF4, 0x7B88A1, 0x88C0D0, 0xEBCB8B, 0xA3BE8C, 0xBF616A, 0xB793B0, 0x81A1C1, 0x8FBCBB, 0xA3BE8C, 0xBF616A) },
    Builtin { name: "One Dark",         kind: Kind::Dark, theme: theme(
                          0x282C34, 0x31353F, 0x3E4451, 0xABB2BF, 0x747B88, 0xC678DD, 0xD19A66, 0x98C379, 0xE06C75, 0x61AFEF, 0x56B6C2, 0xE5C07B, 0x98C379, 0xE06C75) },
    Builtin { name: "Tokyo Night",      kind: Kind::Dark, theme: theme(
                          0x1A1B26, 0x24283B, 0x292E42, 0xC0CAF5, 0x6B7394, 0xBB9AF7, 0xFF9E64, 0x9ECE6A, 0xF7768E, 0x7AA2F7, 0x7DCFFF, 0xE0AF68, 0x9ECE6A, 0xF7768E) },
    Builtin { name: "Catppuccin Mocha", kind: Kind::Dark, theme: theme(
                          0x1E1E2E, 0x26273A, 0x313244, 0xCDD6F4, 0x7F849C, 0xCBA6F7, 0xFAB387, 0xA6E3A1, 0xF38BA8, 0x89B4FA, 0x89DCEB, 0xF9E2AF, 0xA6E3A1, 0xF38BA8) },
    Builtin { name: "Gruvbox Dark",     kind: Kind::Dark, theme: theme(
                          0x282828, 0x3C3836, 0x504945, 0xEBDBB2, 0x928374, 0xD3869B, 0xFE8019, 0xB8BB26, 0xFB4934, 0xFABD2F, 0x83A598, 0x8EC07C, 0xB8BB26, 0xFB4934) },
    Builtin { name: "Solarized Dark",   kind: Kind::Dark, theme: theme(
                          0x002B36, 0x073642, 0x14505D, 0x93A1A1, 0x6E8B8B, 0x6C71C4, 0xCB4B16, 0x859900, 0xDC322F, 0x3295DA, 0x2AA198, 0xB58900, 0x859900, 0xDC322F) },
    Builtin { name: "Rosé Pine",        kind: Kind::Dark, theme: theme(
                          0x191724, 0x1F1D2E, 0x26233A, 0xE0DEF4, 0x8580A0, 0xC4A7E7, 0xF6C177, 0x3E8FAF, 0xEB6F92, 0xEBBCBA, 0x9CCFD8, 0x908CAA, 0x3E8FAF, 0xEB6F92) },
    Builtin { name: "Kanagawa",         kind: Kind::Dark, theme: theme(
                          0x1F1F28, 0x2A2A37, 0x363646, 0xDCD7BA, 0x8A8780, 0x957FB8, 0xFFA066, 0x98BB6C, 0xE82424, 0x7E9CD8, 0x7FB4CA, 0xE6C384, 0x98BB6C, 0xC34043) },
    Builtin { name: "Everforest",       kind: Kind::Dark, theme: theme(
                          0x2D353B, 0x343F44, 0x3D484D, 0xD3C6AA, 0x9DA9A0, 0xD699B6, 0xE69875, 0xA7C080, 0xE67E80, 0x7FBBB3, 0x83C092, 0xDBBC7F, 0xA7C080, 0xE67E80) },
    Builtin { name: "Ayu Dark",         kind: Kind::Dark, theme: theme(
                          0x0D1017, 0x131721, 0x1B1F2B, 0xBFBDB6, 0x6C7380, 0xD2A6FF, 0xFF8F40, 0xAAD94C, 0xF07178, 0x59C2FF, 0x95E6CB, 0xFFB454, 0xAAD94C, 0xF07178) },
    Builtin { name: "Night Owl",        kind: Kind::Dark, theme: theme(
                          0x011627, 0x0B2942, 0x1D3B53, 0xD6DEEB, 0x7A8C8C, 0xC792EA, 0xF78C6C, 0xADDB67, 0xEF5350, 0x82AAFF, 0x7FDBCA, 0xECC48D, 0xADDB67, 0xEF5350) },
    Builtin { name: "GitHub Dark",      kind: Kind::Dark, theme: theme(
                          0x0D1117, 0x161B22, 0x21262D, 0xC9D1D9, 0x8B949E, 0xBC8CFF, 0xD29922, 0x3FB950, 0xF85149, 0x79C0FF, 0x58A6FF, 0xFFA657, 0x3FB950, 0xF85149) },
    Builtin { name: "Mono",             kind: Kind::Dark, theme: theme(
                          0x141518, 0x1E2024, 0x2E3136, 0xE6E8EA, 0x8A9099, 0xE6E8EA, 0xFFFFFF, 0xA8AEB5, 0x8A8F96, 0xE6E8EA, 0xA8AEB5, 0x9AA0A7, 0xD8DEE5, 0x6A6F76) },

    // ── Light ────────────────────────────────────────────────────────────
    Builtin { name: "Solarized Light",  kind: Kind::Light, theme: theme(
                          0xFDF6E3, 0xEEE8D5, 0xDCD6C3, 0x242B33, 0x6B7278, 0x6C3FB8, 0xB56200, 0x1F7A38, 0xC02828, 0xA6227E, 0x1D6692, 0x8A6300, 0x1F7A38, 0xC02828) },
    Builtin { name: "Catppuccin Latte", kind: Kind::Light, theme: theme(
                          0xEFF1F5, 0xE6E9EF, 0xCCD0DA, 0x4C4F69, 0x7C7F93, 0x8839EF, 0xFE640B, 0x40A02B, 0xD20F39, 0x145FF5, 0x0797C4, 0xDF8E1D, 0x40A02B, 0xD20F39) },
    Builtin { name: "GitHub Light",     kind: Kind::Light, theme: theme(
                          0xFFFFFF, 0xF6F8FA, 0xEAEEF2, 0x1F2328, 0x656D76, 0x8250DF, 0x9A6700, 0x1A7F37, 0xCF222E, 0x0550AE, 0x0969DA, 0xBC4C00, 0x1A7F37, 0xCF222E) },
    Builtin { name: "Gruvbox Light",    kind: Kind::Light, theme: theme(
                          0xFBF1C7, 0xEBDBB2, 0xD5C4A1, 0x3C3836, 0x6F6559, 0xB16286, 0xAF3A03, 0x79740E, 0x9D0006, 0x956110, 0x076678, 0x427B58, 0x79740E, 0x9D0006) },
];

/// The default, used when nothing has been chosen and when a name is unknown.
#[must_use]
pub const fn default_theme() -> Theme {
    BUILT_IN[0].theme
}

/// Every theme shipped with Houston.
#[must_use]
pub fn built_in() -> Vec<Named> {
    BUILT_IN
        .iter()
        .map(|entry| Named { name: entry.name.to_string(), theme: entry.theme, kind: entry.kind })
        .collect()
}

/// Older names for themes that have since been renamed.
///
/// `Light` became `Solarized Light` when three more light themes arrived and
/// a bare "Light" stopped saying which one. Someone's config still says the
/// old name; silently falling back to Dracula would look like the setting had
/// been forgotten.
#[must_use]
pub fn resolve_alias(name: &str) -> Option<&'static str> {
    match name {
        "Light" => Some("Solarized Light"),
        _ => None,
    }
}
