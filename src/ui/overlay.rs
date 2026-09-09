//! Modal overlays drawn on top of whatever view is active.
//!
//! Currently one: the session chooser, for deciding which agent a note goes
//! to. It is deliberately small and centred rather than a full-screen mode —
//! you are answering one question, and the context behind it stays visible.

use std::collections::VecDeque;

use crate::{
    app::Picker,
    form::Form,
    session::Sessions,
    ui::{Theme, form as form_ui, keycap},
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
    form_ui::render(frame, popup, form, theme, &format!(" {} ", form.title));
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit.saturating_sub(1)).chain(std::iter::once('…')).collect()
}

/// The input inspector: what the terminal is actually sending.
///
/// Its whole job is to answer "are mouse events arriving?", which nothing else
/// in the app can tell you.
pub fn inspector(frame: &mut Frame, area: Rect, log: &VecDeque<String>, theme: Theme) {
    let height = (u16::try_from(log.len()).unwrap_or(4).max(4) + 3).min(area.height);
    let width = 58.min(area.width);

    // Bottom-right, so it does not cover what you are testing.
    let popup = Rect {
        x: area.x + area.width.saturating_sub(width),
        y: area.y + area.height.saturating_sub(height),
        width,
        height,
    };

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.code))
        .style(Style::default().bg(theme.raised))
        .title(Span::styled(
            " input  ctrl+g to close ",
            Style::default().fg(theme.code).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if log.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  nothing yet — press a key or scroll",
                Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
            )),
            inner,
        );
        return;
    }

    let lines: Vec<Line> = log
        .iter()
        .map(|entry| {
            // Mouse events are the reason this exists, so they stand out.
            let colour = if entry.starts_with("mouse") { theme.running } else { theme.dim };
            Line::from(Span::styled(format!(" {entry}"), Style::default().fg(colour)))
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

/// A session's uncommitted work, read-only.
///
/// Nearly full-screen rather than a small popup: a diff you have to scroll
/// three lines at a time is a diff you will read in another window instead,
/// which is the exact problem this exists to remove.
pub fn diff(frame: &mut Frame, area: Rect, view: &crate::diff::View, theme: Theme) {
    let width = area.width.saturating_sub(4).max(20);
    let height = area.height.saturating_sub(2).max(6);

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
            format!(" {} · {} ", truncate(&view.title, 40), view.changes.describe()),
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            format!(" {} of {} ", view.scroll + 1, view.lines.len()),
            Style::default().fg(theme.dim),
        ));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines: Vec<Line> = view
        .visible(inner.height as usize)
        .iter()
        .map(|line| {
            let style = match crate::diff::classify(line) {
                crate::diff::Row::Added => Style::default().fg(theme.added),
                crate::diff::Row::Removed => Style::default().fg(theme.removed),
                // The hunk header is the only navigational aid in a long diff,
                // so it takes the accent to be findable while scrolling fast.
                crate::diff::Row::Hunk => Style::default().fg(theme.accent),
                crate::diff::Row::File => {
                    Style::default().fg(theme.heading).add_modifier(Modifier::BOLD)
                }
                crate::diff::Row::Context => Style::default().fg(theme.dim),
            };
            Line::from(Span::styled(line.clone(), style))
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

/// The theme picker: every theme, grouped, previewing as you move.
///
/// Sized to the list where it fits and windowed where it does not, because a
/// short terminal is not a reason to hide half the themes.
pub fn themes(
    frame: &mut Frame,
    area: Rect,
    picker: &crate::app::ThemePicker,
    available: &[crate::ui::theme::Named],
    theme: Theme,
) {
    use crate::ui::theme::Entry;

    let entries = crate::ui::theme::picker_entries(available);
    let width = 40.min(area.width);
    let height = (u16::try_from(entries.len()).unwrap_or(10) + 2).min(area.height);

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
            " theme ",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " ↑↓ preview · ↵ keep · esc revert ",
            Style::default().fg(theme.dim),
        ));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    // Keep the selected row on screen. The window follows the selection rather
    // than the other way round, so the list never jumps under a held key.
    let rows = inner.height as usize;
    let chosen = entries
        .iter()
        .position(|entry| matches!(entry, Entry::Theme(index) if *index == picker.selected))
        .unwrap_or(0);
    let start = chosen.saturating_sub(rows / 2).min(entries.len().saturating_sub(rows));

    let lines: Vec<Line> = entries
        .iter()
        .skip(start)
        .take(rows)
        .map(|entry| match entry {
            Entry::Heading(kind) => Line::from(Span::styled(
                format!("  {}", kind.heading()),
                Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
            )),
            Entry::Theme(index) => {
                let named = &available[*index];
                let chosen = *index == picker.selected;

                // A swatch of the theme's own accent, so the list shows what
                // it is offering rather than only naming it.
                let swatch = Span::styled("  ██ ", Style::default().fg(named.theme.accent));
                let name = Span::styled(
                    named.name.clone(),
                    if chosen {
                        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text)
                    },
                );

                let line = Line::from(vec![
                    Span::styled(if chosen { "▸" } else { " " }, Style::default().fg(theme.accent)),
                    swatch,
                    name,
                ]);
                if chosen { line.style(keycap::selected_row(theme)) } else { line }
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

/// A question that has to be answered before something irreversible happens.
///
/// Deliberately small and central. It is the only thing on screen that is not
/// a view, and the one moment where reading before pressing matters.
pub fn confirm(frame: &mut Frame, area: Rect, confirm: &crate::app::Confirm, theme: Theme) {
    let width = 62.min(area.width);
    let height = 7.min(area.height);

    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    frame.render_widget(Clear, popup);

    // Attention, not danger. Danger means something failed; nothing has failed
    // here, and colouring a question as an error would be crying wolf.
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.attention))
        .style(Style::default().bg(theme.raised));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines = vec![
        Line::from(Span::styled(
            format!("  {}", confirm.question),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if !confirm.detail.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", confirm.detail),
            Style::default().fg(theme.attention),
        )));
        lines.push(Line::from(""));
    }

    let mut keys = keycap::row(
        &[("y", "yes"), ("n", "no")],
        keycap::Caps { glyphs: crate::ui::powerline::PLAIN, background: theme.raised },
        theme,
    );
    keys.spans.insert(0, Span::raw("  "));
    lines.push(keys);

    frame.render_widget(Paragraph::new(lines), inner);
}
