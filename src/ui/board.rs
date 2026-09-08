//! The Board: which agents are working, and which are waiting on you.
//!
//! One glance should answer "who needs me?". Everything else is secondary.
//!
//! Shell sessions are shown, but never given an agent's states. Houston has no
//! way to know whether a shell is waiting for input, and a board that guesses
//! is worse than one that admits ignorance.

use crate::{
    session::{Kind, Sessions, State},
    ui::Theme,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

/// The columns, in the order they are read.
const COLUMNS: [(&str, Column); 4] = [
    ("Needs you", Column::AwaitingInput),
    ("Working", Column::Running),
    ("Shells", Column::Shell),
    ("Finished", Column::Exited),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    AwaitingInput,
    Running,
    Shell,
    Exited,
}

impl Column {
    /// Which column a session belongs in.
    const fn of(kind: &Kind, state: State) -> Self {
        match (kind, state) {
            (_, State::Exited(_)) => Self::Exited,
            (Kind::Shell, _) => Self::Shell,
            (Kind::Agent { .. }, State::AwaitingInput) => Self::AwaitingInput,
            (Kind::Agent { .. }, State::Running) => Self::Running,
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, sessions: &Sessions, theme: Theme) {
    if sessions.is_empty() {
        let lines = vec![
            Line::from(Span::styled(
                "Nothing running",
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Start a session on the Sessions view and it will appear here",
                Style::default().fg(theme.dim),
            )),
        ];
        let padding = area.height.saturating_sub(3) / 2;
        let centred = Rect { y: area.y + padding, height: 3.min(area.height), ..area };
        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), centred);
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 4); 4])
        .split(area);

    for (index, (title, column)) in COLUMNS.iter().enumerate() {
        render_column(frame, columns[index], sessions, title, *column, theme);
    }
}

fn render_column(
    frame: &mut Frame,
    area: Rect,
    sessions: &Sessions,
    title: &str,
    column: Column,
    theme: Theme,
) {
    let members: Vec<_> = sessions
        .iter()
        .enumerate()
        .filter(|(_, session)| Column::of(&session.kind, session.state) == column)
        .collect();

    // The column that needs attention is the only one that gets the accent.
    let urgent = column == Column::AwaitingInput && !members.is_empty();
    let accent = if urgent { theme.accent } else { theme.dim };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            format!(" {title} {} ", members.len()),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = members
        .iter()
        .map(|(index, session)| {
            let selected = *index == sessions.selected_index();
            let style = if urgent {
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let mut spans = vec![
                Span::styled(if selected { "▸ " } else { "  " }, Style::default().fg(theme.accent)),
                Span::styled(session.display_name(), style),
            ];

            if let State::Exited(Some(code)) = session.state
                && code != 0
            {
                spans.push(Span::styled(format!(" ({code})"), Style::default().fg(theme.dim)));
            }

            Line::from(spans)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exited_agent_is_finished_regardless_of_its_last_hook() {
        let agent = Kind::Agent { provider: "Claude Code" };
        assert_eq!(Column::of(&agent, State::Exited(Some(1))), Column::Exited);
        assert_eq!(Column::of(&Kind::Shell, State::Exited(None)), Column::Exited);
    }

    #[test]
    fn shells_never_claim_an_agent_state() {
        // Even if something set AwaitingInput on a shell, it stays in Shells:
        // Houston cannot know whether a shell is waiting, and must not pretend.
        assert_eq!(Column::of(&Kind::Shell, State::AwaitingInput), Column::Shell);
        assert_eq!(Column::of(&Kind::Shell, State::Running), Column::Shell);
    }

    #[test]
    fn agents_split_by_whether_they_need_you() {
        let agent = Kind::Agent { provider: "Claude Code" };
        assert_eq!(Column::of(&agent, State::AwaitingInput), Column::AwaitingInput);
        assert_eq!(Column::of(&agent, State::Running), Column::Running);
    }
}
