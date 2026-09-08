//! The persistent tab strip and keybind bar.

use super::Theme;
use crate::app::{App, Tab};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
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

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.surface)),
        area,
    );
}

pub fn placeholder(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let (title, detail) = app.tab.placeholder();
    let lines = vec![
        Line::from(Span::styled(
            title,
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(detail, Style::default().fg(theme.dim))),
    ];

    let vertical_padding = area.height.saturating_sub(3) / 2;
    let centred = Rect { y: area.y + vertical_padding, height: 3.min(area.height), ..area };

    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center).style(Style::default().fg(theme.text)),
        centred,
    );
}

pub fn keybind_bar(frame: &mut Frame, area: Rect, theme: Theme) {
    let keys = [("tab", "next view"), ("1-4", "jump to view"), ("q", "quit")];

    let mut spans = Vec::new();
    for (key, label) in keys {
        spans.push(Span::styled(
            format!(" {key} "),
            Style::default().fg(theme.surface).bg(theme.dim).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!(" {label}   "), Style::default().fg(theme.dim)));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.surface)),
        area,
    );
}
