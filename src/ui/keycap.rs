//! Keys drawn as keys.
//!
//! A key rendered as a bare letter reads as prose — "n start an agent" is a
//! sentence you have to parse. The same key on a raised background reads as a
//! thing you press. It is the cheapest way to make a terminal look like an
//! interface rather than like output.
//!
//! The cap is a *shape*, not a state, so it always uses the neutral raised
//! surface. Giving keys a meaningful colour would break the rule in ADR-0008
//! that a hue means one thing.

use crate::ui::Theme;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

/// A key as it should be printed on a cap.
///
/// **Upper case throughout, because a cap is a picture of a key** and the key
/// on your keyboard says `D` whether or not you hold shift. The footer used to
/// show `n`, `esc` and `W` side by side, which is three conventions in one row.
///
/// That leaves the problem the old spelling was quietly solving: `d` and `D`
/// are *different bindings* — remove a worktree, and remove it anyway — and so
/// are `g`/`G` and `w`/`W`. Upper-casing them blindly would print the footer
/// as an instruction to force-delete. So the case that used to carry "hold
/// shift" is promoted to a symbol that says it outright, and written-out
/// modifiers collapse to the same symbols:
///
/// | binding | shows as |
/// |---|---|
/// | `d` | `D` |
/// | `D` | `⇧D` |
/// | `g/G` | `G/⇧G` |
/// | `ctrl+\` | `^\` |
/// | `shift+pgup` | `⇧PGUP` |
/// | `esc` | `ESC` |
///
/// Digits and symbols are left alone: `1-5`, `[ ]` and `↵` have no case to
/// normalise, and inventing one for them would be noise.
#[must_use]
pub fn label(key: &str) -> String {
    // Written-out modifiers first, so their own letters are not mistaken for
    // key names once everything is upper-cased.
    let key = key.replace("ctrl+", "^").replace("shift+", "\u{21e7}");

    let mut out = String::with_capacity(key.len());
    for character in key.chars() {
        if character.is_ascii_uppercase() {
            // Upper case in the binding means the binding wants shift held.
            out.push('\u{21e7}');
            out.push(character);
        } else if character.is_ascii_lowercase() {
            out.push(character.to_ascii_uppercase());
        } else {
            out.push(character);
        }
    }
    out
}

/// One key, padded so its background reads as a cap.
#[must_use]
pub fn cap<'a>(key: &str, theme: Theme) -> Span<'a> {
    Span::styled(
        format!(" {} ", label(key)),
        Style::default().fg(theme.text).bg(theme.highlight).add_modifier(Modifier::BOLD),
    )
}

/// How a row of keys is drawn, and what it is drawn on.
///
/// The background is here because a powerline separator needs to know what it
/// is flowing *into*. Get it wrong and the arrow sits on the wrong colour,
/// which reads as a notch cut out of the bar rather than as one shape.
#[derive(Clone, Copy)]
pub struct Caps {
    pub glyphs: crate::ui::powerline::Glyphs,
    pub background: ratatui::style::Color,
}

impl Caps {
    /// Plain caps on a surface. For the incidental rows — an empty column, a
    /// vault that has not been pointed anywhere — where the styling would be
    /// decoration on a screen you see once.
    #[must_use]
    pub const fn plain(theme: Theme) -> Self {
        Self { glyphs: crate::ui::powerline::PLAIN, background: theme.surface }
    }
}

/// A key followed by what it does.
///
/// With powerline on, the cap flows into its description instead of sitting
/// beside it — the same shape the tab strip uses, and the reason the footer
/// reads as a row of keys rather than a sentence with bold bits.
#[must_use]
pub fn pair<'a>(key: &str, label: &str, caps: Caps, theme: Theme) -> Vec<Span<'a>> {
    let mut spans = vec![cap(key, theme)];

    if caps.glyphs.is_flowing() {
        spans.push(Span::styled(
            caps.glyphs.cap,
            Style::default().fg(theme.highlight).bg(caps.background),
        ));
        spans.push(Span::styled(label.to_string(), Style::default().fg(theme.dim)));
    } else {
        spans.push(Span::styled(format!(" {label}"), Style::default().fg(theme.dim)));
    }
    spans
}

/// A row of key/label pairs, spaced apart.
///
/// A pair whose key is empty renders as plain text, for the aside that
/// sometimes follows the keys — "all other keys go to the session".
#[must_use]
pub fn row<'a>(binds: &[(&str, &str)], caps: Caps, theme: Theme) -> Line<'a> {
    let mut spans = Vec::with_capacity(binds.len() * 3);

    for (index, (key, label)) in binds.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        if key.is_empty() {
            spans.push(Span::styled(
                (*label).to_string(),
                Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
            ));
        } else {
            spans.extend(pair(key, label, caps, theme));
        }
    }

    Line::from(spans)
}

/// Pads a line with spaces so a row background fills its pane.
///
/// Without this a highlighted row stops at the end of its text, which reads as
/// a coloured word rather than as a selected row.
#[must_use]
pub fn fill(mut line: Line<'_>, width: u16) -> Line<'_> {
    let used: usize = line.spans.iter().map(|span| span.content.chars().count()).sum();
    let padding = (width as usize).saturating_sub(used);
    if padding > 0 {
        line.spans.push(Span::raw(" ".repeat(padding)));
    }
    line
}

/// The background a selected row gets.
///
/// Selection is a *filled row*; state is a colour. Keeping those two jobs on
/// separate properties means a row can be both selected and urgent without
/// either having to win.
#[must_use]
pub fn selected_row(theme: Theme) -> Style {
    Style::default().bg(theme.highlight)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cap_is_padded_so_the_background_reads_as_a_key() {
        let cap = cap("n", Theme::default());
        assert_eq!(cap.content, " N ", "a bare letter would have no cap to see");
        assert_eq!(cap.style.bg, Some(Theme::default().highlight));
    }

    #[test]
    fn a_row_separates_its_pairs() {
        let line =
            row(&[("n", "agent"), ("s", "shell")], Caps::plain(Theme::default()), Theme::default());
        let text: String = line.spans.iter().map(|span| span.content.as_ref()).collect();

        assert!(text.contains(" N "));
        assert!(text.contains("agent"));
        assert!(text.contains(" S "));
        assert!(text.contains("   "), "pairs are spaced apart, not run together");
    }

    #[test]
    fn an_empty_key_renders_as_an_aside_with_no_cap() {
        let line = row(
            &[("", "all other keys go to the session")],
            Caps::plain(Theme::default()),
            Theme::default(),
        );

        assert_eq!(line.spans.len(), 1);
        assert!(line.spans[0].style.bg.is_none(), "an aside is not a key");
    }

    #[test]
    fn an_empty_row_is_empty_rather_than_a_stray_separator() {
        assert!(row(&[], Caps::plain(Theme::default()), Theme::default()).spans.is_empty());
    }

    #[test]
    fn filling_pads_a_line_out_to_the_pane_width() {
        let line = fill(Line::from(Span::raw("abc")), 10);
        let width: usize = line.spans.iter().map(|span| span.content.chars().count()).sum();
        assert_eq!(width, 10, "a selected row must reach the edge, not stop at its text");
    }

    #[test]
    fn filling_a_line_that_is_already_too_long_does_not_pad_it() {
        let line = fill(Line::from(Span::raw("a much longer line")), 5);
        let width: usize = line.spans.iter().map(|span| span.content.chars().count()).sum();
        assert_eq!(width, 18, "never truncate here — clipping is the renderer's job");
    }
}

#[cfg(test)]
mod label_tests {
    use super::*;

    #[test]
    fn an_ordinary_key_is_printed_the_way_it_is_on_the_keyboard() {
        assert_eq!(label("n"), "N");
        assert_eq!(label("q"), "Q");
        assert_eq!(label("hjkl"), "HJKL");
        assert_eq!(label("i/a/o"), "I/A/O");
    }

    /// The reason this cannot be a blind `to_uppercase`.
    ///
    /// `d` removes a worktree and `D` removes one with uncommitted work in it.
    /// A footer that printed both as `D` would be instructing you to destroy
    /// somebody's unfinished work.
    #[test]
    fn a_binding_that_needs_shift_says_so_rather_than_relying_on_case() {
        assert_eq!(label("d"), "D", "the plain binding");
        assert_eq!(label("D"), "⇧D", "and the destructive one, told apart");
        assert_ne!(label("d"), label("D"), "these are different keys and must look different");

        assert_eq!(label("g/G"), "G/⇧G", "top and bottom, in one bind");
        assert_eq!(label("w"), "W");
        assert_eq!(label("W"), "⇧W");
    }

    #[test]
    fn written_out_modifiers_become_the_same_symbols() {
        assert_eq!(label("ctrl+\\"), "^\\", "^ is what a terminal has always called ctrl");
        assert_eq!(label("shift+pgup"), "⇧PGUP");
        assert_eq!(
            label("shift+pgdn"),
            "⇧PGDN",
            "and the modifier is not doubled by the letters that spelled it"
        );
    }

    #[test]
    fn named_keys_read_as_names() {
        assert_eq!(label("esc"), "ESC");
        assert_eq!(label("tab"), "TAB");
    }

    #[test]
    fn things_with_no_case_are_left_exactly_as_they_are() {
        assert_eq!(label("1-5"), "1-5");
        assert_eq!(label("[ ]"), "[ ]");
        assert_eq!(label("↵"), "↵");
        assert_eq!(label("/"), "/");
        assert_eq!(label(""), "", "the empty key is the aside, not a cap");
    }

    /// Whatever the transform does, it must not reach for a codepoint no
    /// installed font claims — see `ui::powerline`.
    #[test]
    fn nothing_printed_on_a_cap_is_a_private_use_codepoint() {
        for key in ["d", "D", "ctrl+\\", "shift+pgup", "g/G", "esc", "↵", "1-5"] {
            for character in label(key).chars() {
                let point = character as u32;
                assert!(
                    !(0xE000..=0xF8FF).contains(&point),
                    "{key:?} produced U+{point:X}, which renders as a question mark"
                );
            }
        }
    }
}
