//! The Board: which agents are working, and which are waiting on you.
//!
//! One glance should answer "who needs me?". Everything else is secondary,
//! which is why exactly one column ever takes the accent colour.
//!
//! Shell sessions are shown, but never given an agent's states. Houston has no
//! way to know whether a shell is waiting for input, and a board that guesses
//! is worse than one that admits ignorance.

use crate::{
    session::{Kind, Sessions, State},
    ui::{Theme, keycap},
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

/// The columns, in the order they are read.
pub const COLUMNS: [(&str, Column); 4] = [
    ("Needs you", Column::AwaitingInput),
    ("Working", Column::Running),
    ("Shells", Column::Shell),
    ("Finished", Column::Exited),
];

/// Rows a card occupies: name, detail, and a blank between cards.
const CARD_HEIGHT: u16 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    AwaitingInput,
    Running,
    Shell,
    Exited,
}

impl Column {
    /// Which column a session belongs in.
    pub const fn of(kind: &Kind, state: State) -> Self {
        match (kind, state) {
            (_, State::Exited(_)) => Self::Exited,
            (Kind::Shell, _) => Self::Shell,
            (Kind::Agent { .. }, State::AwaitingInput) => Self::AwaitingInput,
            (Kind::Agent { .. }, State::Running) => Self::Running,
        }
    }
}

/// The session indices in a column, in sidebar order.
///
/// Shared with the event loop so keyboard navigation and rendering can never
/// disagree about what is where.
pub fn members(sessions: &Sessions, column: Column) -> Vec<usize> {
    sessions
        .iter()
        .enumerate()
        .filter(|(_, session)| Column::of(&session.kind, session.state) == column)
        .map(|(index, _)| index)
        .collect()
}

pub fn render(frame: &mut Frame, area: Rect, sessions: &Sessions, theme: Theme) {
    if sessions.is_empty() {
        let lines = vec![
            Line::from(Span::styled(
                "Nothing running",
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            keycap::row(&[("1", "go to Sessions and start one")], theme),
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
    let members = members(sessions, column);

    // Each column carries its own state colour, and an empty one recedes to
    // dim — so "who needs me?" is answered by colour before anything is read.
    let urgent = column == Column::AwaitingInput && !members.is_empty();
    let accent = if members.is_empty() {
        theme.dim
    } else {
        match column {
            Column::AwaitingInput => theme.attention,
            Column::Running => theme.running,
            Column::Shell => theme.link,
            Column::Exited => theme.dim,
        }
    };

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

    for (slot, index) in members.iter().enumerate() {
        let Ok(offset) = u16::try_from(slot) else { break };
        let top = offset * CARD_HEIGHT;
        if top + 2 > inner.height {
            break;
        }

        let card = Rect { y: inner.y + top, height: 2, ..inner };
        render_card(frame, card, sessions, *index, urgent, theme);
    }
}

/// One session, drawn as a two-line card with a coloured spine.
fn render_card(
    frame: &mut Frame,
    area: Rect,
    sessions: &Sessions,
    index: usize,
    urgent: bool,
    theme: Theme,
) {
    let Some(session) = sessions.iter().nth(index) else { return };
    let selected = index == sessions.selected_index();

    let state_colour = match session.state {
        State::AwaitingInput => theme.attention,
        State::Running if matches!(session.kind, Kind::Shell) => theme.link,
        State::Running => theme.running,
        State::Exited(Some(0) | None) => theme.dim,
        State::Exited(_) => theme.danger,
    };

    // The spine shows state; the name shows selection. Two questions, two
    // places, so neither has to lose to the other.
    let spine_colour = state_colour;
    let name_style = if selected {
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
    } else if urgent {
        Style::default().fg(theme.attention)
    } else {
        Style::default().fg(theme.text)
    };

    // A spine rather than a full border: four columns of boxed cards inside a
    // boxed column is more lines than information.
    let spine = Span::styled("▌", Style::default().fg(spine_colour));

    let detail = match session.kind {
        Kind::Agent { provider } => provider.to_string(),
        Kind::Shell => "shell".to_string(),
    };
    let detail = match session.state {
        State::Exited(Some(code)) if code != 0 => format!("{detail} · exit {code}"),
        State::Exited(_) => format!("{detail} · exited"),
        _ => detail,
    };

    let width = area.width.saturating_sub(3) as usize;
    let lines = vec![
        Line::from(vec![
            spine.clone(),
            Span::raw(" "),
            Span::styled(truncate(&session.display_name(), width), name_style),
        ]),
        Line::from(vec![
            spine,
            Span::raw(" "),
            Span::styled(truncate(&detail, width), Style::default().fg(theme.dim)),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines), area);
}

fn truncate(text: &str, limit: usize) -> String {
    if limit == 0 {
        return String::new();
    }
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit.saturating_sub(1)).chain(std::iter::once('…')).collect()
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

    #[test]
    fn truncation_never_exceeds_its_budget() {
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("abcdefghij", 5).chars().count(), 5);
        assert_eq!(truncate("anything", 0), "", "a zero-width card renders nothing");
    }
}
