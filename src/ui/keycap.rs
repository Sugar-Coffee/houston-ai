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

/// **Caps show the binding exactly as you type it, and nothing else.**
///
/// This was upper-cased once, on the reasoning that a cap is a picture of a
/// key and the key on your keyboard says `D` whether or not you hold shift.
/// True, and beside the point. Case *is* the notation in a vim-shaped app:
/// `d` and `D` are different bindings, and so are `g`/`G` and `n`/`N`.
/// Upper-casing meant promoting that distinction to a `⇧` marker and then
/// asking you to translate `⇧D` back into "hold shift and press d" — a step
/// added to the one thing a footer exists to tell you.
///
/// The cap's *fill* is what makes a key read as a key. That was the whole
/// idea; upper-casing was piling on.
///
/// One key, padded so its background reads as a cap.
#[must_use]
pub fn cap<'a>(key: &str, theme: Theme) -> Span<'a> {
    Span::styled(
        format!(" {key} "),
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
    within(binds, usize::MAX, caps, theme)
}

/// The same, but stopping before the first bind that would not fit.
///
/// **A half-drawn bind is worse than a missing one.** The Tasks view ran out
/// of room at 130 columns and the bar ended `a  hide fi`, which is not a
/// keybind, it is a puzzle — and the one it cut in half was the one somebody
/// then asked me whether it did what it says. Whole binds or nothing, and the
/// list is ordered so that what goes first is what you were least likely to
/// need reminding of.
pub fn within<'a>(binds: &[(&str, &str)], width: usize, caps: Caps, theme: Theme) -> Line<'a> {
    let mut spans: Vec<Span<'a>> = Vec::with_capacity(binds.len() * 3);
    let mut used = 0;

    for (index, (key, label)) in binds.iter().enumerate() {
        let mut next: Vec<Span<'a>> = Vec::with_capacity(4);
        if index > 0 {
            next.push(Span::raw("   "));
        }
        if key.is_empty() {
            next.push(Span::styled(
                (*label).to_string(),
                Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
            ));
        } else {
            next.extend(pair(key, label, caps, theme));
        }

        let cost: usize = next.iter().map(Span::width).sum();
        if used + cost > width {
            break;
        }
        used += cost;
        spans.extend(next);
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

    /// A half-drawn bind is worse than a missing one: the Tasks bar ended
    /// `a  hide fi` at 130 columns, which is a puzzle rather than a keybind.
    #[test]
    fn a_bind_that_does_not_fit_is_dropped_whole() {
        let theme = Theme::default();
        let caps = Caps::plain(theme);
        let binds = [("tab", "view"), ("j/k", "select"), ("a", "show done")];

        let full = row(&binds, caps, theme).width();
        let trimmed = within(&binds, full - 3, caps, theme);

        assert!(trimmed.width() <= full - 3, "it fits the budget");
        let text: String = trimmed.spans.iter().map(|span| span.content.as_ref()).collect();
        assert!(text.contains("view"), "the first bind survives");
        assert!(!text.contains("show"), "and the last one is gone rather than cut");
        assert!(!text.contains("sho"), "no fragment of it either");
    }

    #[test]
    fn a_cap_is_padded_so_the_background_reads_as_a_key() {
        let cap = cap("n", Theme::default());
        assert_eq!(cap.content, " n ", "a bare letter would have no cap to see");
        assert_eq!(cap.style.bg, Some(Theme::default().highlight));
    }

    #[test]
    fn a_row_separates_its_pairs() {
        let line =
            row(&[("n", "agent"), ("s", "shell")], Caps::plain(Theme::default()), Theme::default());
        let text: String = line.spans.iter().map(|span| span.content.as_ref()).collect();

        assert!(text.contains(" n "));
        assert!(text.contains("agent"));
        assert!(text.contains(" s "));
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
mod cap_tests {
    use super::*;

    /// The property that replaced the upper-casing rule: what is drawn on a
    /// cap is what you press. No translation, in either direction.
    #[test]
    fn a_cap_shows_the_binding_verbatim() {
        for key in ["n", "N", "d", "D", "esc", "tab", "ctrl+\\", "shift+pgup", "g/G", "1-5"] {
            let drawn = cap(key, Theme::default()).content.into_owned();
            assert_eq!(drawn, format!(" {key} "), "{key:?} should be drawn as itself");
        }
    }

    /// The case that made upper-casing dangerous, and the reason caps stay
    /// verbatim: these are two different bindings.
    #[test]
    fn a_shifted_binding_is_distinguishable_from_its_lower_case_twin() {
        let plain = cap("d", Theme::default()).content.into_owned();
        let shifted = cap("D", Theme::default()).content.into_owned();

        assert_ne!(plain, shifted, "d removes a worktree and D used to force it");
        assert!(shifted.contains('D'), "and the capital is shown as a capital");
    }
}
