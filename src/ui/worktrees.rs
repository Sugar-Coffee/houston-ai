//! The worktree manager, as a view rather than a popup.
//!
//! It was a modal overlay first, which was the right size for "what exists?"
//! and the wrong size for what it grew into. A worktree now carries a branch,
//! a tracking count, a diff, a status and a path, and there is nowhere to put
//! five facts about ten worktrees inside a box floating over something else.
//!
//! The other half of the argument is that this is not a passing question. You
//! come here to decide what to land and what to throw away, which is work, and
//! work belongs in a view you can sit in.

use crate::{
    session::Sessions,
    ui::{keycap, theme::Theme},
    worktree::{Status, Worktree},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

/// Rows per worktree: what it is, and where it came from.
const ENTRY_HEIGHT: usize = 2;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    worktrees: Option<&Vec<Worktree>>,
    selected: usize,
    sessions: &Sessions,
    glyphs: crate::ui::powerline::Glyphs,
    theme: Theme,
) {
    let count = worktrees.map_or(0, Vec::len);
    let heading = match count {
        0 => " worktrees ".to_string(),
        1 => " 1 worktree ".to_string(),
        _ => format!(" {count} worktrees "),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.dim))
        .style(Style::default().bg(theme.surface))
        .title(Span::styled(
            heading,
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(worktrees) = worktrees.filter(|list| !list.is_empty()) else {
        frame.render_widget(Paragraph::new(empty_state(worktrees.is_none(), theme)), inner);
        return;
    };

    let visible = (inner.height as usize / ENTRY_HEIGHT).max(1);
    let start = selected
        .saturating_sub(visible.saturating_sub(1) / 2)
        .min(worktrees.len().saturating_sub(visible));

    let mut lines = Vec::with_capacity(visible * ENTRY_HEIGHT);

    for (index, worktree) in worktrees.iter().enumerate().skip(start).take(visible) {
        let chosen = index == selected;
        let in_use = sessions.uses_worktree(&worktree.name);

        for line in entry(worktree, chosen, in_use, glyphs, theme) {
            lines.push(if chosen {
                keycap::fill(line, inner.width).style(keycap::selected_row(theme))
            } else {
                line
            });
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// What to say when there is nothing to show.
///
/// Distinguishes "none exist" from "we could not look", because the second is
/// a problem and the first is Tuesday.
fn empty_state<'a>(unread: bool, theme: Theme) -> Vec<Line<'a>> {
    if unread {
        return vec![Line::from(Span::styled(
            "  Could not read ~/.houston/worktrees — press r to try again",
            Style::default().fg(theme.danger),
        ))];
    }

    vec![
        Line::from(""),
        Line::from(Span::styled(
            "  No worktrees yet",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        {
            let mut row = keycap::row(
                &[("n", "start a session on the Sessions view and tick Worktree")],
                keycap::Caps::plain(theme),
                theme,
            );
            row.spans.insert(0, Span::raw("  "));
            row
        },
    ]
}

/// One worktree as two rows.
fn entry<'a>(
    worktree: &Worktree,
    chosen: bool,
    in_use: bool,
    glyphs: crate::ui::powerline::Glyphs,
    theme: Theme,
) -> [Line<'a>; ENTRY_HEIGHT] {
    let status = worktree.status(in_use);

    // Status decides the colour, because it is the only thing here that can
    // want something from you.
    let colour = match status {
        Status::Orphaned => theme.danger,
        Status::InUse => theme.running,
        Status::Abandoned => theme.attention,
        Status::Idle => theme.dim,
    };

    let name_style = if chosen {
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    let tracking = match (worktree.ahead, worktree.behind) {
        (0, 0) => String::new(),
        (ahead, 0) => format!("↑{ahead}"),
        (0, behind) => format!("↓{behind}"),
        (ahead, behind) => format!("↑{ahead} ↓{behind}"),
    };

    // The status is the whole reason for the row, so with powerline on it
    // takes the shape that says "this is a badge" rather than sitting in the
    // same weight as the name beside it.
    let status_spans = if glyphs.is_flowing() {
        vec![
            Span::styled(glyphs.notch, Style::default().fg(colour)),
            Span::styled(
                format!("{} {} ", status.marker(), status.label()),
                Style::default().fg(theme.surface).bg(colour).add_modifier(Modifier::BOLD),
            ),
            Span::styled(glyphs.cap, Style::default().fg(colour)),
            Span::styled(
                " ".repeat(18usize.saturating_sub(status.label().chars().count())),
                Style::default(),
            ),
        ]
    } else {
        vec![
            Span::styled(format!("{} ", status.marker()), Style::default().fg(colour)),
            Span::styled(format!("{:<20}", status.label()), Style::default().fg(colour)),
        ]
    };

    let mut first = vec![
        Span::styled(if chosen { " ▸ " } else { "   " }, Style::default().fg(theme.accent)),
        Span::styled(format!("{:<28}", truncate(&worktree.name, 27)), name_style),
    ];
    first.extend(status_spans);
    first.push(Span::styled(tracking, Style::default().fg(theme.dim)));
    let first = Line::from(first);

    let repository = worktree
        .repository
        .file_name()
        .map_or_else(|| "—".to_string(), |name| name.to_string_lossy().into_owned());

    let mut second = vec![
        Span::raw("     "),
        Span::styled(format!("{} ", glyphs.branch), Style::default().fg(theme.link)),
        Span::styled(
            format!("{:<24}", truncate(worktree.branch.as_deref().unwrap_or("—"), 23)),
            Style::default().fg(theme.link),
        ),
        Span::styled(format!("{:<20}", truncate(&repository, 19)), Style::default().fg(theme.dim)),
    ];

    // The diff, in the same colours it has everywhere else in Houston.
    if let Some(changes) = worktree.changes.filter(|changes| !changes.is_empty()) {
        second.push(Span::styled(
            format!("+{}", changes.insertions),
            Style::default().fg(theme.added),
        ));
        second.push(Span::raw(" "));
        second.push(Span::styled(
            format!("−{}", changes.deletions),
            Style::default().fg(theme.removed),
        ));
    }

    [first, Line::from(second)]
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit.saturating_sub(1)).chain(std::iter::once('…')).collect()
}
