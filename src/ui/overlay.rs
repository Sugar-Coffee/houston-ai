//! Modal overlays drawn on top of whatever view is active.
//!
//! Currently one: the session chooser, for deciding which agent a note goes
//! to. It is deliberately small and centred rather than a full-screen mode —
//! you are answering one question, and the context behind it stays visible.

use crate::{app::Picker, session::Sessions, ui::Theme};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

const WIDTH: u16 = 52;

pub fn picker(frame: &mut Frame, area: Rect, picker: &Picker, sessions: &Sessions, theme: Theme) {
    // Two rows of chrome plus one per session, capped so a long list cannot
    // outgrow the screen.
    let rows = u16::try_from(sessions.len()).unwrap_or(u16::MAX);
    let height = (rows + 2).min(area.height);
    let width = WIDTH.min(area.width);

    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    // Without this the view underneath shows through the gaps.
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.raised))
        .title(Span::styled(
            format!(" {} ", picker.prompt),
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines: Vec<Line> = sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let selected = index == picker.selected;
            let style = if selected {
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            // Same numbering as the sidebar, so the digit you press is the one
            // already in front of you.
            let ordinal = if index < 9 { format!("{} ", index + 1) } else { "  ".to_string() };

            Line::from(vec![
                Span::styled(if selected { "▸ " } else { "  " }, Style::default().fg(theme.accent)),
                Span::styled(ordinal, Style::default().fg(theme.dim)),
                Span::styled(session.display_name(), style),
                Span::styled(
                    format!("   {}", session.state.label()),
                    Style::default().fg(theme.dim),
                ),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}
