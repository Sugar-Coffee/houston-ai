//! The persistent tab strip and keybind bar.

use super::Theme;
use crate::app::{App, Tab};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub fn tab_strip(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let mut spans = vec![Span::styled(
        " houston ",
        Style::default().fg(theme.surface).bg(theme.accent).add_modifier(Modifier::BOLD),
    )];

    for (index, tab) in Tab::ALL.iter().enumerate() {
        let selected = *tab == app.tab;
        let style = if selected {
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.dim)
        };
        spans.push(Span::styled(format!("  {}·{}", index + 1, tab.title()), style));
    }

    if app.is_attached() {
        // Green: your keystrokes are reaching a live child.
        spans.push(Span::styled(
            "   ● attached",
            Style::default().fg(theme.running).add_modifier(Modifier::BOLD),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.surface)),
        area,
    );
}

/// The bottom bar: either the current view's keys, or a transient notice.
pub fn keybind_bar(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    // An armed quit is a question, so it takes the attention colour too.
    if app.quit_armed {
        let warning = Line::from(vec![
            Span::styled(
                " q ",
                Style::default().fg(theme.surface).bg(theme.attention).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  press again to quit  ",
                Style::default().fg(theme.attention).add_modifier(Modifier::BOLD),
            ),
            Span::styled("esc", Style::default().fg(theme.dim)),
            Span::styled("  stay", Style::default().fg(theme.dim)),
        ]);
        frame.render_widget(Paragraph::new(warning).style(Style::default().bg(theme.raised)), area);
        return;
    }

    let line = app.notice.as_ref().map_or_else(
        || {
            let mut spans = Vec::new();
            for (key, label) in app.keybinds() {
                if !key.is_empty() {
                    spans.push(Span::styled(
                        format!(" {key} "),
                        Style::default()
                            .fg(theme.text)
                            .bg(theme.highlight)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                spans.push(Span::styled(format!(" {label}  "), Style::default().fg(theme.dim)));
            }
            Line::from(spans)
        },
        |notice| {
            // A notice is Houston telling you something it could not do, so it
            // borrows the attention colour rather than the selection one.
            Line::from(Span::styled(
                format!(" {notice} "),
                Style::default().fg(theme.surface).bg(theme.attention).add_modifier(Modifier::BOLD),
            ))
        },
    );

    frame.render_widget(Paragraph::new(line).style(Style::default().bg(theme.raised)), area);
}
