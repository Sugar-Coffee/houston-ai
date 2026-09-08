//! The Sessions view: a list of running children on the left, the focused
//! child's screen on the right.

use crate::{
    session::{Focus, Kind, Sessions, State},
    ui::{Theme, terminal},
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

/// Width of the session list. Wide enough for a name plus a state badge.
const LIST_WIDTH: u16 = 26;

/// Splits the view into (list, terminal).
///
/// Shared with the event loop, which needs the terminal rect to keep every
/// child's grid the same size as the area it is drawn into. If these two ever
/// disagree, output wraps in the wrong place.
pub fn split(area: Rect) -> (Rect, Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LIST_WIDTH), Constraint::Min(0)])
        .split(area);
    (columns[0], columns[1])
}

/// The inner area of the terminal pane, inside its border.
pub fn terminal_area(area: Rect) -> Rect {
    let (_, pane) = split(area);
    Block::default().borders(Borders::ALL).inner(pane)
}

pub fn render(frame: &mut Frame, area: Rect, sessions: &Sessions, theme: Theme) {
    let (list_area, pane_area) = split(area);
    render_list(frame, list_area, sessions, theme);
    render_pane(frame, pane_area, sessions, theme);
}

fn render_list(frame: &mut Frame, area: Rect, sessions: &Sessions, theme: Theme) {
    let heading = match sessions.len() {
        0 => " sessions ".to_string(),
        1 => " 1 session ".to_string(),
        count => format!(" {count} sessions "),
    };

    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme.dim))
        .title(Span::styled(heading, Style::default().fg(theme.dim)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if sessions.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "no sessions",
                Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
            ))),
            inner,
        );
        return;
    }

    let attached = sessions.focus() == Focus::Attached;
    let lines: Vec<Line> = sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let selected = index == sessions.selected_index();

            // The marker distinguishes "this is where the cursor is" from
            // "your keystrokes are going here", which are not the same thing.
            let marker = match (selected, attached) {
                (true, true) => "▶ ",
                (true, false) => "· ",
                (false, _) => "  ",
            };

            let name_style = if selected {
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let badge = match session.state {
                State::Running => Span::styled(" ●", Style::default().fg(theme.dim)),
                State::AwaitingInput => Span::styled(" ◆", Style::default().fg(theme.accent)),
                State::Exited(_) => Span::styled(" ×", Style::default().fg(theme.dim)),
            };

            let kind = match session.kind {
                Kind::Agent { .. } => "",
                Kind::Shell => " $",
            };

            Line::from(vec![
                Span::styled(marker, Style::default().fg(theme.accent)),
                Span::styled(truncate(&session.display_name(), 16), name_style),
                Span::styled(kind, Style::default().fg(theme.dim)),
                badge,
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_pane(frame: &mut Frame, area: Rect, sessions: &Sessions, theme: Theme) {
    let attached = sessions.focus() == Focus::Attached;

    let title = sessions
        .selected()
        .map_or_else(|| " Houston ".to_string(), |session| format!(" {} ", session.display_name()));

    let border_style =
        if attached { Style::default().fg(theme.accent) } else { Style::default().fg(theme.dim) };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Span::styled(title, border_style.add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(session) = sessions.selected() else {
        let hint = vec![
            Line::from(Span::styled(
                "No sessions yet",
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "n  start an agent          s  start a shell",
                Style::default().fg(theme.dim),
            )),
        ];
        let padding = inner.height.saturating_sub(3) / 2;
        let centred = Rect { y: inner.y + padding, height: 3.min(inner.height), ..inner };
        frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), centred);
        return;
    };

    if let Ok(term) = session.pty().term().lock() {
        terminal::render(frame, inner, &term, theme, attached);
    }

    if let State::Exited(code) = session.state {
        let message = code.map_or_else(
            || " exited ".to_string(),
            |code| format!(" exited with {code} — x to close "),
        );
        let width = u16::try_from(message.len()).unwrap_or(inner.width).min(inner.width);
        let banner = Rect { x: inner.x, y: inner.y, width, height: 1.min(inner.height) };
        frame.render_widget(
            Paragraph::new(Span::styled(
                message,
                Style::default().fg(theme.surface).bg(theme.accent),
            )),
            banner,
        );
    }
}

/// Truncates to a cell budget, with an ellipsis when it bites.
fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit.saturating_sub(1)).chain(std::iter::once('…')).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names_are_left_alone() {
        assert_eq!(truncate("claude", 16), "claude");
        assert_eq!(truncate("exactly-sixteen!", 16), "exactly-sixteen!");
    }

    #[test]
    fn long_names_get_an_ellipsis_within_budget() {
        let truncated = truncate("a-very-long-session-name", 16);
        assert_eq!(truncated.chars().count(), 16);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        // Eight two-byte characters fit in a limit of eight cells.
        assert_eq!(truncate("αααααααα", 8).chars().count(), 8);
    }

    #[test]
    fn the_terminal_area_sits_inside_the_pane_border() {
        let area = Rect::new(0, 0, 100, 30);
        let (list, pane) = split(area);
        let inner = terminal_area(area);

        assert_eq!(list.width, LIST_WIDTH);
        assert!(inner.width < pane.width, "the border must cost width");
        assert!(inner.height < pane.height, "the border must cost height");
        assert!(inner.x >= pane.x && inner.y >= pane.y);
    }
}
