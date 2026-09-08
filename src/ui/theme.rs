//! The colour palette.
//!
//! Dracula-derived, but the palette is not the design — the **discipline** is.
//! Every hue has exactly one job, and is used for that job everywhere. That is
//! what stops seven colours reading as a rainbow: when you see orange, one
//! thing is true, and it is always the same thing.
//!
//! | hue | means | appears on |
//! |---|---|---|
//! | purple | *you are here* | selection, active tab, focus borders |
//! | orange | *this wants you* | an agent awaiting input, and nothing else |
//! | green | *this is healthy* | running sessions, insert mode |
//! | red | *this failed* | errors, non-zero exits |
//! | cyan | *you can follow this* | wikilinks and links |
//! | pink | *this is structure* | markdown headings |
//! | yellow | *this is literal* | code spans and fences |
//!
//! The rule that keeps it honest: **if adding colour somewhere would need a
//! new hue, the answer is usually that it does not need colour.** Reach for
//! `dim` first.

use ratatui::style::Color;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    // Surfaces, darkest first.
    /// The page.
    pub surface: Color,
    /// Bars and popups, lifted off the page.
    pub raised: Color,
    /// Subtle fills: the current line, a selected row.
    pub highlight: Color,

    // Text.
    pub text: Color,
    /// Secondary text: gutters, folders, chrome that should recede.
    pub dim: Color,

    // Interaction.
    /// Where you are. Selection, focus, the active tab.
    pub accent: Color,

    // States. Each of these means one thing.
    /// Something is waiting on you.
    pub attention: Color,
    /// Something is working.
    pub running: Color,
    /// Something failed.
    pub danger: Color,

    // Content.
    /// Markdown headings.
    pub heading: Color,
    /// Anything followable.
    pub link: Color,
    /// Code spans and fences.
    pub code: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dracula()
    }
}

impl Theme {
    /// The default. Dracula's palette, assigned by meaning rather than by
    /// which colours happened to be left over.
    #[must_use]
    pub const fn dracula() -> Self {
        Self {
            surface: Color::Rgb(0x28, 0x2A, 0x36),
            raised: Color::Rgb(0x34, 0x37, 0x46),
            highlight: Color::Rgb(0x44, 0x47, 0x5A),

            text: Color::Rgb(0xF8, 0xF8, 0xF2),
            dim: Color::Rgb(0x62, 0x72, 0xA4),

            accent: Color::Rgb(0xBD, 0x93, 0xF9),

            attention: Color::Rgb(0xFF, 0xB8, 0x6C),
            running: Color::Rgb(0x50, 0xFA, 0x7B),
            danger: Color::Rgb(0xFF, 0x55, 0x55),

            heading: Color::Rgb(0xFF, 0x79, 0xC6),
            link: Color::Rgb(0x8B, 0xE9, 0xFD),
            code: Color::Rgb(0xF1, 0xFA, 0x8C),
        }
    }
}
