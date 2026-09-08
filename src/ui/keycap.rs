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

/// One key, padded so its background reads as a cap.
#[must_use]
pub fn cap<'a>(key: &str, theme: Theme) -> Span<'a> {
    Span::styled(
        format!(" {key} "),
        Style::default().fg(theme.text).bg(theme.highlight).add_modifier(Modifier::BOLD),
    )
}

/// A key followed by what it does.
#[must_use]
pub fn pair<'a>(key: &str, label: &str, theme: Theme) -> Vec<Span<'a>> {
    vec![cap(key, theme), Span::styled(format!(" {label}"), Style::default().fg(theme.dim))]
}

/// A row of key/label pairs, spaced apart.
///
/// A pair whose key is empty renders as plain text, for the aside that
/// sometimes follows the keys — "all other keys go to the session".
#[must_use]
pub fn row<'a>(binds: &[(&str, &str)], theme: Theme) -> Line<'a> {
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
            spans.extend(pair(key, label, theme));
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
        assert_eq!(cap.content, " n ", "a bare letter would have no cap to see");
        assert_eq!(cap.style.bg, Some(Theme::default().highlight));
    }

    #[test]
    fn a_row_separates_its_pairs() {
        let line = row(&[("n", "agent"), ("s", "shell")], Theme::default());
        let text: String = line.spans.iter().map(|span| span.content.as_ref()).collect();

        assert!(text.contains(" n "));
        assert!(text.contains("agent"));
        assert!(text.contains(" s "));
        assert!(text.contains("   "), "pairs are spaced apart, not run together");
    }

    #[test]
    fn an_empty_key_renders_as_an_aside_with_no_cap() {
        let line = row(&[("", "all other keys go to the session")], Theme::default());

        assert_eq!(line.spans.len(), 1);
        assert!(line.spans[0].style.bg.is_none(), "an aside is not a key");
    }

    #[test]
    fn an_empty_row_is_empty_rather_than_a_stray_separator() {
        assert!(row(&[], Theme::default()).spans.is_empty());
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
