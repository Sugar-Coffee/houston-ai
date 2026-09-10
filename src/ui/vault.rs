//! The Vault view: notes on the left, the open note on the right.

use crate::{
    ui::{Theme, keycap},
    vault::{
        Browser,
        browser::{Entry, Mode},
    },
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

const LIST_WIDTH: u16 = 34;

pub fn render(frame: &mut Frame, area: Rect, browser: Option<&Browser>, theme: Theme) {
    let Some(browser) = browser else {
        render_missing(frame, area, theme);
        return;
    };

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LIST_WIDTH), Constraint::Min(0)])
        .split(area);

    render_list(frame, columns[0], browser, theme);
    render_note(frame, columns[1], browser, theme);
}

/// Shown when no vault is configured. Says how to fix it rather than just
/// reporting that something is missing.
fn render_missing(frame: &mut Frame, area: Rect, theme: Theme) {
    let lines = vec![
        Line::from(Span::styled(
            "No vault found",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        keycap::row(
            &[("5", "open Settings and point Houston at a folder")],
            keycap::Caps::plain(theme),
            theme,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "or set HOUSTON_VAULT",
            Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
        )),
    ];
    let padding = area.height.saturating_sub(5) / 2;
    let centred = Rect { y: area.y + padding, height: 5.min(area.height), ..area };
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), centred);
}

fn render_list(frame: &mut Frame, area: Rect, browser: &Browser, theme: Theme) {
    // While typing, the title is the query — so the search box and the list
    // header are the same thing and no vertical space is spent on a prompt.
    let title = match browser.mode() {
        Mode::Finding => format!(" find: {}▏", browser.query()),
        Mode::Searching => format!(" search: {}▏", browser.query()),
        Mode::Browsing => browser.source().label(),
    };
    let typing = browser.mode() != Mode::Browsing;
    // Yellow while typing a query, matching the editor's search mode.
    let title_style = if typing {
        Style::default().fg(theme.code).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.dim)
    };

    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(theme.dim))
        .title(Span::styled(title, title_style));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if browser.entries().is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  nothing matches",
                Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
            )),
            inner,
        );
        return;
    }

    // Keep the selection on screen without a scrollbar: window the list around
    // it. With 1,065 notes this matters from the first keypress.
    let height = inner.height as usize;
    let selected = browser.selected_index();
    let start = selected.saturating_sub(height.saturating_sub(1) / 2);
    let start = start.min(browser.entries().len().saturating_sub(height));

    let hits = browser.hits();
    let lines: Vec<Line> = browser
        .entries()
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
        .map(|(index, entry)| {
            let selected = index == browser.selected_index();

            let mut spans = vec![Span::styled(
                if selected { "▸ " } else { "  " },
                Style::default().fg(theme.accent),
            )];

            match entry {
                Entry::Folder { name, depth, expanded, .. } => {
                    spans.push(Span::raw("  ".repeat(*depth)));
                    // The triangle is the whole affordance: it says this row
                    // opens, and which way it currently is.
                    spans.push(Span::styled(
                        if *expanded { "▾ " } else { "▸ " },
                        Style::default().fg(theme.dim),
                    ));
                    spans.push(Span::styled(
                        truncate(name, 22_usize.saturating_sub(depth * 2)),
                        // Folders take `link`: they are the followable thing
                        // in this list, which is exactly what that hue means.
                        if selected {
                            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.link).add_modifier(Modifier::BOLD)
                        },
                    ));
                }
                Entry::Note { name, depth, .. } => {
                    // Two spaces where a folder's triangle would be, so names
                    // line up with the folders they sit under.
                    spans.push(Span::raw("  ".repeat(*depth)));
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        truncate(name, 22_usize.saturating_sub(depth * 2)),
                        if selected {
                            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.text)
                        },
                    ));
                }
            }

            // A full-text hit is more useful with its line number attached.
            if let Some(hit) = hits.get(index) {
                spans
                    .push(Span::styled(format!(":{}  ", hit.line), Style::default().fg(theme.dim)));
                spans.push(Span::styled(
                    truncate(&hit.text, 40),
                    Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
                ));
            }

            let line = Line::from(spans);
            if selected {
                keycap::fill(line, inner.width).style(keycap::selected_row(theme))
            } else {
                line
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_note(frame: &mut Frame, area: Rect, browser: &Browser, theme: Theme) {
    let title = browser
        .open()
        .and_then(|open| browser.vault.get(open.id))
        .map_or_else(|| " no note open ".to_string(), |note| format!(" {} ", note.relative));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.dim))
        .title(Span::styled(title, Style::default().fg(theme.dim).add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(open) = browser.open() else {
        let hint = vec![
            Line::from(Span::styled(
                format!("{} notes indexed", browser.vault.len()),
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            keycap::row(
                &[("↵", "open"), ("/", "find"), ("f", "search"), ("e", "edit")],
                keycap::Caps::plain(theme),
                theme,
            ),
            Line::from(""),
            keycap::row(
                &[("y", "yank path"), ("i", "send to session"), ("w", "wrap")],
                keycap::Caps::plain(theme),
                theme,
            ),
        ];
        let padding = inner.height.saturating_sub(5) / 2;
        let centred = Rect { y: inner.y + padding, height: 5.min(inner.height), ..inner };
        frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), centred);
        return;
    };

    if open.document.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  (empty note)",
                Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
            )),
            inner,
        );
        return;
    }

    let paragraph = Paragraph::new(open.document.lines.clone()).scroll((open.scroll, 0));
    let paragraph = if browser.wraps() { paragraph.wrap(Wrap { trim: false }) } else { paragraph };

    frame.render_widget(paragraph, inner);
}

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
    fn truncation_respects_a_character_budget() {
        assert_eq!(truncate("short", 10), "short");
        let long = truncate("a-really-long-note-name", 10);
        assert_eq!(long.chars().count(), 10);
        assert!(long.ends_with('…'));
    }
}
