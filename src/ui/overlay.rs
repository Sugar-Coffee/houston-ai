//! Modal overlays drawn on top of whatever view is active.
//!
//! Currently one: the session chooser, for deciding which agent a note goes
//! to. It is deliberately small and centred rather than a full-screen mode —
//! you are answering one question, and the context behind it stays visible.

use crate::{
    app::Picker,
    form::Form,
    session::Sessions,
    ui::{Theme, form as form_ui, keycap},
    worktree::Worktree,
};
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

/// The new-session dialog.
///
/// Sized to its content so it reads as a dialog rather than a second screen —
/// you are answering a handful of questions, and what is behind stays visible.
pub fn form(frame: &mut Frame, area: Rect, form: &Form, theme: Theme) {
    let completions =
        form.focused().map_or(0, |field| u16::try_from(field.completions.len()).unwrap_or(0));
    let visible =
        u16::try_from(form.fields.iter().filter(|field| field.visible).count()).unwrap_or(4);

    let height = (visible + completions.min(7) + 2).min(area.height);
    let width = 64.min(area.width);

    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    frame.render_widget(Clear, popup);
    form_ui::render(frame, popup, form, theme, " new session ");
}

/// The worktree manager.
pub fn worktrees(
    frame: &mut Frame,
    area: Rect,
    worktrees: &[Worktree],
    selected: usize,
    sessions: &Sessions,
    theme: Theme,
) {
    // The empty state is three lines tall, so sizing to the list alone would
    // clip the very message that says how to get out of it.
    let rows = u16::try_from(worktrees.len()).unwrap_or(1).max(3);
    let height = (rows + 2).min(area.height);
    let width = 78.min(area.width);

    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.raised))
        .title(Span::styled(
            format!(" worktrees {} ", worktrees.len()),
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if worktrees.is_empty() {
        let lines = vec![
            Line::from(Span::styled(
                "  None yet",
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            {
                let mut row = keycap::row(&[("n", "start a session and tick Worktree")], theme);
                row.spans.insert(0, Span::raw("  "));
                row
            },
        ];
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let lines: Vec<Line> = worktrees
        .iter()
        .enumerate()
        .map(|(index, worktree)| {
            let chosen = index == selected;
            let in_use = sessions.uses_worktree(&worktree.name);

            // State first: in-use and dirty are the two facts that decide
            // whether this is safe to remove.
            let (badge, badge_colour) = if in_use {
                ("● in use", theme.running)
            } else if worktree.dirty {
                ("◆ uncommitted", theme.attention)
            } else {
                ("· clean", theme.dim)
            };

            let name_style = if chosen {
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let repository = worktree
                .repository
                .file_name()
                .map_or_else(|| "—".to_string(), |name| name.to_string_lossy().into_owned());

            Line::from(vec![
                Span::styled(if chosen { "▸ " } else { "  " }, Style::default().fg(theme.accent)),
                Span::styled(format!("{:<26}", truncate(&worktree.name, 25)), name_style),
                Span::styled(
                    format!("{:<18}", truncate(&repository, 17)),
                    Style::default().fg(theme.link),
                ),
                Span::styled(format!("{badge:<15}"), Style::default().fg(badge_colour)),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit.saturating_sub(1)).chain(std::iter::once('…')).collect()
}
