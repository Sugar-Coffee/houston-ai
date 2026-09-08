//! The persistent tab strip and keybind bar.

use super::Theme;
use crate::{
    app::{App, Tab},
    ui::keycap,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

/// Rows the header occupies: padding, the tabs, and a rule under them.
///
/// The padding is what makes it read as a header rather than as a status line.
/// It costs two rows of body height, which is why `layout` falls back to one
/// row on a short terminal.
pub const HEADER_HEIGHT: u16 = 3;

pub fn tab_strip(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    if area.height == 0 {
        return;
    }

    // Fill the whole header, so the padding rows belong to the bar rather than
    // being gaps that show the body through.
    frame.render_widget(Block::default().style(Style::default().bg(theme.raised)), area);

    let padded = area.height >= HEADER_HEIGHT;
    let tabs_row = if padded { area.y + 1 } else { area.y };

    frame.render_widget(
        Paragraph::new(tab_line(app, theme)).style(Style::default().bg(theme.raised)),
        Rect { y: tabs_row, height: 1, ..area },
    );

    // A rule between chrome and content. Without it the header and the body
    // read as one surface.
    if padded {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "─".repeat(area.width as usize),
                Style::default().fg(theme.highlight),
            ))),
            Rect { y: area.y + 2, height: 1, ..area },
        );
    }
}

fn tab_line<'a>(app: &App, theme: Theme) -> Line<'a> {
    // The wordmark is deliberately quiet. It is the one thing on screen you
    // never need to find, so it does not get a fill — the active tab does.
    let mut spans = vec![Span::styled(
        "  houston   ",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )];

    for (index, tab) in Tab::ALL.iter().enumerate() {
        let selected = *tab == app.tab;
        let label = format!("  {}·{}  ", index + 1, tab.title());

        // A filled pill, so "where am I" is answered by shape before colour.
        let style = if selected {
            Style::default().fg(theme.surface).bg(theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.dim)
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
    }

    if app.is_attached() {
        // Green: your keystrokes are reaching a live child.
        spans.push(Span::styled(
            "   ● attached",
            Style::default().fg(theme.running).add_modifier(Modifier::BOLD),
        ));
    }

    Line::from(spans)
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
            let binds = app.keybinds();
            let mut line = keycap::row(&binds, theme);
            // A leading space so the first cap is not flush against the edge.
            line.spans.insert(0, Span::raw(" "));
            line
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
