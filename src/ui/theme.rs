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
//! | added | *this arrived* | added diff lines |
//! | removed | *this went* | removed diff lines |
//!
//! The rule that keeps it honest: **if adding colour somewhere would need a
//! new hue, the answer is usually that it does not need colour.** Reach for
//! `dim` first.
//!
//! `added` and `removed` were added under that rule rather than around it. A
//! diff is the one place where two opposed colours *are* the notation, and the
//! obvious shortcut — reusing `running` and `danger`, which are already green
//! and red — would have given both a second meaning. A pane full of green
//! lines where green elsewhere means "an agent is working" is precisely the
//! confusion the rule exists to prevent. The session card takes the other
//! branch of the same rule and shows its `+12 −4` in `dim`.
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
    /// Lines a diff added.
    pub added: Color,
    /// Lines a diff removed.
    pub removed: Color,

    /// What a child process's *default* background paints as.
    ///
    /// Normally the theme's own `surface`, so a session matches every other
    /// pane and a theme actually changes how the app looks. Set it to
    /// `default` in a theme file to let your terminal's own background show
    /// through instead.
    ///
    /// **Superseded an earlier decision.** This defaulted to [`Color::Reset`]
    /// so sessions would inherit the terminal, which fixed one inconsistency
    /// and created a worse one: with the body unpainted too, a theme could
    /// only recolour text. Light mode on a dark terminal was the proof — dark
    /// text on a dark background. A theme has to own the surface to be a
    /// theme.
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
        super::palettes::default_theme()
    }
}

/// Whether a theme is meant for a dark terminal or a light one.
///
/// Categorising the picker rather than decorating it: with nineteen built-ins
/// an ungrouped list is a wall, and the first question anyone asks of a theme
/// is which half of that list it is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dark,
    Light,
    /// Loaded from `~/.houston/themes`. Kept separate so your own work is
    /// findable rather than filed among nineteen strangers.
    Yours,
}

impl Kind {
    #[must_use]
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Yours => "Yours",
        }
    }

    /// The order the picker shows the groups in.
    pub const ALL: [Self; 3] = [Self::Dark, Self::Light, Self::Yours];
}

/// One line of the theme picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// A group heading. Only emitted for groups that have members.
    Heading(Kind),
    /// The theme at this index in the list the entries were built from.
    Theme(usize),
}

/// Lays the picker out: themes in order, with a heading wherever the kind
/// changes.
///
/// Relies on `available` keeping built-ins in their declared order and
/// appending files after them, which puts the groups in `Kind::ALL` order
/// without a sort. If that ever stops being true this produces repeated
/// headings rather than wrong ones, which is the right way round to fail.
#[must_use]
pub fn picker_entries(themes: &[Named]) -> Vec<Entry> {
    let mut entries = Vec::with_capacity(themes.len() + Kind::ALL.len());
    let mut group = None;

    for (index, named) in themes.iter().enumerate() {
        if group != Some(named.kind) {
            entries.push(Entry::Heading(named.kind));
            group = Some(named.kind);
        }
        entries.push(Entry::Theme(index));
    }
    entries
}

/// The WCAG contrast ratio between two colours, 1.0 to 21.0.
///
/// Exists because nineteen palettes are nineteen chances to paste a value into
/// the wrong row, and the failure mode — text the same brightness as the
/// surface behind it — is invisible in a diff and obvious the moment somebody
/// picks that theme. A number can be asserted on; an eye cannot check
/// nineteen.
///
/// `None` for anything that is not a concrete RGB value, since an indexed or
/// reset colour means whatever the terminal says it means.
///
/// Test-only. It exists to check the shipped palettes, and nothing at runtime
/// has a question that a contrast ratio answers.
#[cfg(test)]
#[must_use]
pub fn contrast(a: Color, b: Color) -> Option<f64> {
    let luminance = |colour: Color| match colour {
        Color::Rgb(r, g, b) => {
            let channel = |value: u8| {
                let value = f64::from(value) / 255.0;
                if value <= 0.039_28 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
            };
            // The WCAG coefficients, written as `mul_add` because clippy's
            // `suboptimal_flops` insists. The formula is the standard one.
            Some(0.0722f64.mul_add(channel(b), 0.7152f64.mul_add(channel(g), 0.2126 * channel(r))))
        }
        _ => None,
    };

    let (a, b) = (luminance(a)?, luminance(b)?);
    let (lighter, darker) = if a > b { (a, b) } else { (b, a) };
    Some((lighter + 0.05) / (darker + 0.05))
}

/// A theme with the name it is chosen by.
#[derive(Debug, Clone)]
pub struct Named {
    pub name: String,
    pub theme: Theme,
    pub kind: Kind,
}

pub use super::palettes::{built_in, resolve_alias};

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
            // Keep the built-in's grouping. A retuned Dracula is still a dark
            // theme, and filing it under "Yours" would move it out of the
            // block where you go looking for it.
            existing.theme = theme.theme;
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

    Ok(Named { name, theme: file.into_theme()?, kind: Kind::Yours })
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
    added: Option<String>,
    removed: Option<String>,
    terminal_background: Option<String>,
}

impl File {
    fn into_theme(self) -> Result<Theme> {
        let base = Theme::dracula();
        let pick = |value: Option<String>, fallback: Color| -> Result<Color> {
            value.map_or(Ok(fallback), |text| parse_colour(&text))
        };

        // Resolved first: a file that names a surface but not a terminal
        // background should have its sessions match that surface, not
        // Dracula's.
        let surface = pick(self.surface, base.surface)?;

        Ok(Theme {
            surface,
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
            added: pick(self.added, base.added)?,
            removed: pick(self.removed, base.removed)?,
            terminal_background: pick(self.terminal_background, surface)?,
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
added     = "#50FA7B"   # a diff added this line
removed   = "#FF5555"   # a diff removed this line

# What a session's default background paints as. Defaults to `surface` above,
# so sessions match the rest of the app. Set it to "default" if you would
# rather your own terminal background showed through.
# terminal_background = "default"
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
        let palette = |wanted: &str| themes.iter().find(|t| t.name == wanted).unwrap().theme;
        assert_ne!(palette("Dracula").accent, palette("Nord").accent);
        assert_ne!(palette("Solarized Light").text, palette("Dracula").text);
    }

    #[test]
    fn the_light_theme_is_actually_light() {
        let light = built_in().into_iter().find(|t| t.name == "Solarized Light").unwrap().theme;
        let Color::Rgb(r, g, b) = light.surface else { panic!("expected rgb") };
        let brightness = u32::from(r) + u32::from(g) + u32::from(b);
        assert!(brightness > 500, "a light theme needs a light surface");

        let Color::Rgb(r, g, b) = light.text else { panic!("expected rgb") };
        assert!(u32::from(r) + u32::from(g) + u32::from(b) < 300, "and dark text");
    }

    /// A theme that cannot change the background is not a theme — light mode
    /// on a dark terminal was dark text on a dark background.
    #[test]
    fn every_theme_owns_its_background() {
        for theme in built_in() {
            assert_eq!(
                theme.theme.terminal_background, theme.theme.surface,
                "{} lets the terminal show through, so switching to it changes little",
                theme.name
            );
            assert_ne!(theme.theme.surface, Color::Reset, "{}", theme.name);
        }
    }

    #[test]
    fn a_theme_file_naming_only_a_surface_gets_matching_sessions() {
        let path = std::env::temp_dir().join("houston-theme-surface-only.toml");
        std::fs::write(&path, "name = \"Surface\"\nsurface = \"#101010\"\n").unwrap();

        let loaded = load(&path).unwrap();
        assert_eq!(
            loaded.theme.terminal_background,
            Color::Rgb(0x10, 0x10, 0x10),
            "sessions should match the surface the file asked for"
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_theme_can_still_ask_for_the_terminals_own_background() {
        let path = std::env::temp_dir().join("houston-theme-passthrough.toml");
        std::fs::write(&path, "terminal_background = \"default\"\n").unwrap();

        assert_eq!(load(&path).unwrap().theme.terminal_background, Color::Reset);

        std::fs::remove_file(&path).ok();
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

#[cfg(test)]
mod palette_tests {
    use super::*;

    /// The one thing a theme cannot get wrong.
    ///
    /// 4.5:1 is the WCAG AA threshold for body text. A palette below it is not
    /// a matter of taste — it is text you cannot read.
    #[test]
    fn every_built_in_theme_has_readable_body_text() {
        for named in built_in() {
            let ratio = contrast(named.theme.text, named.theme.surface)
                .expect("built-in themes are all concrete RGB");

            assert!(
                ratio >= 4.5,
                "{} puts text on surface at only {ratio:.1}:1 — below the 4.5:1 readable floor",
                named.name
            );
        }
    }

    /// Dim is *meant* to recede, so it gets a lower floor — but it still has
    /// to be legible, because paths and branches live there.
    #[test]
    fn dim_text_recedes_without_disappearing() {
        for named in built_in() {
            let ratio = contrast(named.theme.dim, named.theme.surface).unwrap();

            assert!(
                ratio >= 3.0,
                "{} has dim at {ratio:.1}:1, which is gone, not quiet",
                named.name
            );
            assert!(
                ratio < contrast(named.theme.text, named.theme.surface).unwrap(),
                "{} has dim no quieter than its body text",
                named.name
            );
        }
    }

    /// Cards and popups have to read as lifted off the page, and the selected
    /// row has to read as selected. Both are backgrounds, so both are judged
    /// against the surface rather than against text.
    #[test]
    fn surfaces_are_distinguishable_from_one_another() {
        for named in built_in() {
            let theme = named.theme;
            assert_ne!(theme.raised, theme.surface, "{} cannot lift a popup", named.name);
            assert_ne!(
                theme.highlight, theme.surface,
                "{} cannot show which row is selected",
                named.name
            );
        }
    }

    /// A diff has to read at a glance, and it reads by the two signs having
    /// visibly different colours.
    #[test]
    fn added_and_removed_are_told_apart() {
        for named in built_in() {
            assert_ne!(
                named.theme.added, named.theme.removed,
                "{} renders a diff in one colour",
                named.name
            );
        }
    }

    /// Sessions paint the theme's own surface unless a *file* says otherwise.
    #[test]
    fn a_built_in_theme_owns_its_session_background() {
        for named in built_in() {
            assert_eq!(
                named.theme.terminal_background, named.theme.surface,
                "{} would let the terminal show through",
                named.name
            );
        }
    }

    #[test]
    fn there_are_enough_themes_to_be_worth_a_picker_and_they_are_grouped() {
        let themes = built_in();
        assert!(themes.len() >= 12, "only {} built-ins", themes.len());

        assert!(themes.iter().any(|t| t.kind == Kind::Light), "there is a light option");
        assert!(themes.iter().any(|t| t.kind == Kind::Dark));

        let mut names: Vec<_> = themes.iter().map(|t| t.name.clone()).collect();
        names.sort();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "two themes share a name, so one is unreachable");
    }

    #[test]
    fn the_old_name_for_a_renamed_theme_still_resolves() {
        assert_eq!(resolve_alias("Light"), Some("Solarized Light"));
        assert!(resolve_alias("Dracula").is_none(), "a current name needs no alias");
    }

    #[test]
    fn contrast_is_measured_the_way_the_standard_defines_it() {
        let white = Color::Rgb(0xFF, 0xFF, 0xFF);
        let black = Color::Rgb(0x00, 0x00, 0x00);

        assert!((contrast(white, black).unwrap() - 21.0).abs() < 0.01, "the extreme is 21:1");
        assert!((contrast(white, white).unwrap() - 1.0).abs() < 0.01, "a colour on itself is 1:1");
        assert!(contrast(Color::Reset, black).is_none(), "an unresolved colour has no ratio");
    }
}

#[cfg(test)]
mod picker_tests {
    use super::*;

    fn named(name: &str, kind: Kind) -> Named {
        Named { name: name.to_string(), theme: Theme::default(), kind }
    }

    #[test]
    fn the_picker_heads_each_group_once() {
        let themes = vec![
            named("Dracula", Kind::Dark),
            named("Nord", Kind::Dark),
            named("GitHub Light", Kind::Light),
            named("Mine", Kind::Yours),
        ];

        let entries = picker_entries(&themes);

        assert_eq!(
            entries,
            vec![
                Entry::Heading(Kind::Dark),
                Entry::Theme(0),
                Entry::Theme(1),
                Entry::Heading(Kind::Light),
                Entry::Theme(2),
                Entry::Heading(Kind::Yours),
                Entry::Theme(3),
            ],
            "two dark themes share one heading, and every group gets its own"
        );
    }

    #[test]
    fn a_group_nobody_is_in_gets_no_heading() {
        let themes = vec![named("Dracula", Kind::Dark)];
        let entries = picker_entries(&themes);

        assert!(
            !entries.contains(&Entry::Heading(Kind::Yours)),
            "an empty 'Yours' heading would advertise a feature as a blank line"
        );
        assert_eq!(entries.len(), 2, "one heading, one theme");
    }

    #[test]
    fn the_real_built_ins_lay_out_as_dark_then_light() {
        let themes = built_in();
        let entries = picker_entries(&themes);

        let headings: Vec<_> = entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Heading(kind) => Some(*kind),
                Entry::Theme(_) => None,
            })
            .collect();

        assert_eq!(headings, vec![Kind::Dark, Kind::Light], "grouped, and each group once");
    }
}
