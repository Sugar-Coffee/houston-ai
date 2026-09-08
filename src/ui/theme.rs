//! The colour palette, and the themes that fill it in.
//!
//! Dracula-derived by default, but the palette is not the design — the
//! **discipline** is. Every hue has exactly one job, and is used for that job
//! everywhere. That is what stops seven colours reading as a rainbow: when you
//! see orange, one thing is true, and it is always the same thing.
//!
//! | hue | means | appears on |
//! |---|---|---|
//! | accent | *you are here* | selection, active tab, focus borders |
//! | attention | *this wants you* | an agent awaiting input, and nothing else |
//! | running | *this is healthy* | running sessions, insert mode |
//! | danger | *this failed* | errors, non-zero exits |
//! | link | *you can follow this* | wikilinks and links |
//! | heading | *this is structure* | markdown headings |
//! | code | *this is literal* | code spans and fences |
//!
//! The rule that keeps it honest: **if adding colour somewhere would need a
//! new hue, the answer is usually that it does not need colour.** Reach for
//! `dim` first.
//!
//! A theme fills these roles in. Views never name a colour — they name a role
//! — so a new theme is a remap rather than an audit of every call site.

use anyhow::{Context, Result, bail};
use ratatui::style::Color;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// What a child process's *default* background paints as.
    ///
    /// Usually [`Color::Reset`], which lets your own terminal background show
    /// through — a session then matches every other pane instead of sitting in
    /// a slightly different shade of dark. Themes that want to own the whole
    /// window can set it explicitly.
    pub terminal_background: Color,
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
            terminal_background: Color::Reset,
        }
    }

    #[must_use]
    pub const fn monokai() -> Self {
        Self {
            surface: Color::Rgb(0x27, 0x28, 0x22),
            raised: Color::Rgb(0x33, 0x34, 0x2C),
            highlight: Color::Rgb(0x49, 0x48, 0x3E),
            text: Color::Rgb(0xF8, 0xF8, 0xF2),
            dim: Color::Rgb(0x75, 0x71, 0x5E),
            accent: Color::Rgb(0xAE, 0x81, 0xFF),
            attention: Color::Rgb(0xFD, 0x97, 0x1F),
            running: Color::Rgb(0xA6, 0xE2, 0x2E),
            danger: Color::Rgb(0xF9, 0x26, 0x72),
            heading: Color::Rgb(0xF9, 0x26, 0x72),
            link: Color::Rgb(0x66, 0xD9, 0xEF),
            code: Color::Rgb(0xE6, 0xDB, 0x74),
            terminal_background: Color::Reset,
        }
    }

    #[must_use]
    pub const fn nord() -> Self {
        Self {
            surface: Color::Rgb(0x2E, 0x34, 0x40),
            raised: Color::Rgb(0x3B, 0x42, 0x52),
            highlight: Color::Rgb(0x4C, 0x56, 0x6A),
            text: Color::Rgb(0xEC, 0xEF, 0xF4),
            dim: Color::Rgb(0x61, 0x6E, 0x88),
            accent: Color::Rgb(0x88, 0xC0, 0xD0),
            attention: Color::Rgb(0xEB, 0xCB, 0x8B),
            running: Color::Rgb(0xA3, 0xBE, 0x8C),
            danger: Color::Rgb(0xBF, 0x61, 0x6A),
            heading: Color::Rgb(0xB4, 0x8E, 0xAD),
            link: Color::Rgb(0x81, 0xA1, 0xC1),
            code: Color::Rgb(0xD0, 0x87, 0x70),
            terminal_background: Color::Reset,
        }
    }

    /// For a light terminal. The roles keep their meanings; only the values
    /// change, which is the point of naming roles rather than colours.
    #[must_use]
    pub const fn light() -> Self {
        Self {
            surface: Color::Rgb(0xFD, 0xF6, 0xE3),
            raised: Color::Rgb(0xEE, 0xE8, 0xD5),
            highlight: Color::Rgb(0xDC, 0xD6, 0xC3),
            text: Color::Rgb(0x24, 0x2B, 0x33),
            dim: Color::Rgb(0x7A, 0x82, 0x8A),
            accent: Color::Rgb(0x6C, 0x3F, 0xB8),
            attention: Color::Rgb(0xB5, 0x62, 0x00),
            running: Color::Rgb(0x1F, 0x7A, 0x38),
            danger: Color::Rgb(0xC0, 0x28, 0x28),
            heading: Color::Rgb(0xA6, 0x22, 0x7E),
            link: Color::Rgb(0x1D, 0x66, 0x92),
            code: Color::Rgb(0x8A, 0x63, 0x00),
            terminal_background: Color::Reset,
        }
    }

    /// Greys plus a single accent.
    ///
    /// Deliberately breaks the one-hue-one-meaning rule, because the whole
    /// point is that there are no hues. State is carried by brightness instead,
    /// which is weaker — that is the trade you are choosing.
    #[must_use]
    pub const fn mono() -> Self {
        Self {
            surface: Color::Rgb(0x14, 0x15, 0x18),
            raised: Color::Rgb(0x1E, 0x20, 0x24),
            highlight: Color::Rgb(0x2E, 0x31, 0x36),
            text: Color::Rgb(0xE6, 0xE8, 0xEA),
            dim: Color::Rgb(0x6B, 0x70, 0x77),
            accent: Color::Rgb(0xE6, 0xE8, 0xEA),
            attention: Color::Rgb(0xFF, 0xFF, 0xFF),
            running: Color::Rgb(0xA8, 0xAE, 0xB5),
            danger: Color::Rgb(0x8A, 0x8F, 0x96),
            heading: Color::Rgb(0xE6, 0xE8, 0xEA),
            link: Color::Rgb(0xA8, 0xAE, 0xB5),
            code: Color::Rgb(0x9A, 0xA0, 0xA7),
            terminal_background: Color::Reset,
        }
    }
}

/// A theme with the name it is chosen by.
#[derive(Debug, Clone)]
pub struct Named {
    pub name: String,
    pub theme: Theme,
}

/// The themes shipped with Houston, in the order Settings offers them.
#[must_use]
pub fn built_in() -> Vec<Named> {
    [
        ("Dracula", Theme::dracula()),
        ("Monokai", Theme::monokai()),
        ("Nord", Theme::nord()),
        ("Light", Theme::light()),
        ("Mono", Theme::mono()),
    ]
    .into_iter()
    .map(|(name, theme)| Named { name: name.to_string(), theme })
    .collect()
}

/// Every theme available: the built-ins, plus any `.toml` in `directory`.
///
/// A file whose name matches a built-in replaces it, so you can retune Dracula
/// without having to invent a name for your version.
#[must_use]
pub fn available(directory: &Path) -> Vec<Named> {
    let mut themes = built_in();

    let Ok(entries) = std::fs::read_dir(directory) else { return themes };
    let mut custom: Vec<Named> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "toml"))
        .filter_map(|entry| load(&entry.path()).ok())
        .collect();
    custom.sort_by(|a, b| a.name.cmp(&b.name));

    for theme in custom {
        if let Some(existing) = themes.iter_mut().find(|other| other.name == theme.name) {
            *existing = theme;
        } else {
            themes.push(theme);
        }
    }
    themes
}

/// Reads one theme file.
pub fn load(path: &Path) -> Result<Named> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let file: File = toml::from_str(&text)
        .with_context(|| format!("{} is not a valid theme", path.display()))?;

    let name = file.name.clone().unwrap_or_else(|| {
        path.file_stem().map_or_else(|| "custom".to_string(), |s| s.to_string_lossy().into_owned())
    });

    Ok(Named { name, theme: file.into_theme()? })
}

/// A theme as written on disk.
///
/// Every field is optional and falls back to Dracula's, so a file can override
/// three colours without restating the other ten.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    name: Option<String>,
    surface: Option<String>,
    raised: Option<String>,
    highlight: Option<String>,
    text: Option<String>,
    dim: Option<String>,
    accent: Option<String>,
    attention: Option<String>,
    running: Option<String>,
    danger: Option<String>,
    heading: Option<String>,
    link: Option<String>,
    code: Option<String>,
    terminal_background: Option<String>,
}

impl File {
    fn into_theme(self) -> Result<Theme> {
        let base = Theme::dracula();
        let pick = |value: Option<String>, fallback: Color| -> Result<Color> {
            value.map_or(Ok(fallback), |text| parse_colour(&text))
        };

        Ok(Theme {
            surface: pick(self.surface, base.surface)?,
            raised: pick(self.raised, base.raised)?,
            highlight: pick(self.highlight, base.highlight)?,
            text: pick(self.text, base.text)?,
            dim: pick(self.dim, base.dim)?,
            accent: pick(self.accent, base.accent)?,
            attention: pick(self.attention, base.attention)?,
            running: pick(self.running, base.running)?,
            danger: pick(self.danger, base.danger)?,
            heading: pick(self.heading, base.heading)?,
            link: pick(self.link, base.link)?,
            code: pick(self.code, base.code)?,
            terminal_background: pick(self.terminal_background, base.terminal_background)?,
        })
    }
}

/// Parses a colour: `#RRGGBB`, a 0–255 palette index, or `default`.
///
/// `default` means the terminal's own colour — useful for backgrounds, so a
/// theme can leave your terminal's look alone rather than painting over it.
fn parse_colour(text: &str) -> Result<Color> {
    let text = text.trim();

    if text.eq_ignore_ascii_case("default") || text.eq_ignore_ascii_case("terminal") {
        return Ok(Color::Reset);
    }

    if let Some(hex) = text.strip_prefix('#') {
        if hex.len() != 6 {
            bail!("'{text}' should be #RRGGBB");
        }
        let channel = |range: std::ops::Range<usize>| {
            u8::from_str_radix(&hex[range], 16).context("not hexadecimal")
        };
        return Ok(Color::Rgb(channel(0..2)?, channel(2..4)?, channel(4..6)?));
    }

    if let Ok(index) = text.parse::<u8>() {
        return Ok(Color::Indexed(index));
    }

    bail!("'{text}' is not a colour — use #RRGGBB, a number 0-255, or 'default'")
}

/// The template written into the themes directory, so the format is
/// discoverable without reading the source.
pub const TEMPLATE: &str = r##"# A Houston theme.
#
# Copy this file, rename it, and change what you like. Every field is optional
# and falls back to Dracula's, so overriding three colours is fine.
#
# Colours are "#RRGGBB", a palette index 0-255, or "default" for your
# terminal's own colour.
#
# Naming a theme the same as a built-in replaces it.

name = "My Theme"

# Surfaces, darkest first.
surface   = "#282A36"   # the page
raised    = "#343746"   # bars and popups
highlight = "#44475A"   # selected rows, filled keycaps

# Text.
text = "#F8F8F2"
dim  = "#6272A4"        # gutters, paths, chrome that should recede

# Each of these means exactly one thing, everywhere.
accent    = "#BD93F9"   # you are here
attention = "#FFB86C"   # this wants you
running   = "#50FA7B"   # live and healthy
danger    = "#FF5555"   # failed
heading   = "#FF79C6"   # markdown structure
link      = "#8BE9FD"   # followable
code      = "#F1FA8C"   # literal

# What a session's default background paints as. "default" lets your own
# terminal background show through, which is usually what you want.
terminal_background = "default"
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse_in_every_accepted_form() {
        assert_eq!(parse_colour("#FF8800").unwrap(), Color::Rgb(0xFF, 0x88, 0x00));
        assert_eq!(parse_colour("  #ff8800  ").unwrap(), Color::Rgb(0xFF, 0x88, 0x00));
        assert_eq!(parse_colour("42").unwrap(), Color::Indexed(42));
        assert_eq!(parse_colour("default").unwrap(), Color::Reset);
        assert_eq!(parse_colour("Terminal").unwrap(), Color::Reset);
    }

    #[test]
    fn a_bad_colour_is_an_error_that_says_what_is_wrong() {
        for bad in ["#FFF", "#GGGGGG", "puce", "", "300"] {
            let error = parse_colour(bad).unwrap_err().to_string();
            assert!(!error.is_empty(), "{bad} should be rejected with a reason");
        }
    }

    #[test]
    fn every_built_in_theme_is_distinct_and_named() {
        let themes = built_in();
        assert!(themes.len() >= 4);

        let mut names: Vec<&str> = themes.iter().map(|t| t.name.as_str()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "theme names must be unique");

        // Roles must actually differ, or a theme is only nominally a theme.
        assert_ne!(Theme::dracula().accent, Theme::nord().accent);
        assert_ne!(Theme::light().text, Theme::dracula().text);
    }

    #[test]
    fn the_light_theme_is_actually_light() {
        let Color::Rgb(r, g, b) = Theme::light().surface else { panic!("expected rgb") };
        let brightness = u32::from(r) + u32::from(g) + u32::from(b);
        assert!(brightness > 500, "a light theme needs a light surface");

        let Color::Rgb(r, g, b) = Theme::light().text else { panic!("expected rgb") };
        assert!(u32::from(r) + u32::from(g) + u32::from(b) < 300, "and dark text");
    }

    /// A session should match the rest of the app rather than sitting in a
    /// slightly different shade — which is what a hardcoded surface did.
    #[test]
    fn themes_leave_the_terminal_background_alone_by_default() {
        for theme in built_in() {
            assert_eq!(
                theme.theme.terminal_background,
                Color::Reset,
                "{} paints over the user's terminal background",
                theme.name
            );
        }
    }

    #[test]
    fn a_theme_file_overrides_only_what_it_names() {
        let path = std::env::temp_dir().join("houston-theme-partial.toml");
        std::fs::write(&path, "name = \"Partial\"\naccent = \"#123456\"\n").unwrap();

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.name, "Partial");
        assert_eq!(loaded.theme.accent, Color::Rgb(0x12, 0x34, 0x56));
        assert_eq!(loaded.theme.text, Theme::dracula().text, "the rest falls back");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_theme_file_without_a_name_is_named_after_the_file() {
        let path = std::env::temp_dir().join("houston-theme-unnamed.toml");
        std::fs::write(&path, "accent = \"#123456\"\n").unwrap();

        assert_eq!(load(&path).unwrap().name, "houston-theme-unnamed");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_malformed_theme_is_reported_rather_than_silently_ignored() {
        let path = std::env::temp_dir().join("houston-theme-bad.toml");
        std::fs::write(&path, "accent = \"puce\"\n").unwrap();
        assert!(load(&path).is_err());

        std::fs::write(&path, "not = \"a known field\"\n").unwrap();
        assert!(load(&path).is_err(), "a typo in a field name should not pass silently");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_custom_theme_can_replace_a_built_in_by_name() {
        let directory = std::env::temp_dir().join("houston-themes-test");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("mine.toml"), "name = \"Dracula\"\naccent = \"#000000\"\n")
            .unwrap();

        let themes = available(&directory);
        let dracula = themes.iter().find(|t| t.name == "Dracula").unwrap();

        assert_eq!(dracula.theme.accent, Color::Rgb(0, 0, 0), "the file wins");
        assert_eq!(
            themes.iter().filter(|t| t.name == "Dracula").count(),
            1,
            "and does not duplicate"
        );

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn the_template_is_a_valid_theme() {
        // It is written into the themes directory, so it must load.
        let file: File = toml::from_str(TEMPLATE).expect("the template must parse");
        assert!(file.into_theme().is_ok());
    }
}
