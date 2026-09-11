//! The Tasks view: what to do on the left, the one you are pointing at on the
//! right.
//!
//! Same split as the Vault view, for the same reason — the list answers "what
//! is there" and the pane answers "what is this", and needing a keystroke
//! between those two questions makes a list of twenty things unreadable.

use crate::{
    ui::{Theme, keycap},
    vault::tasks::{Priority, Status, Task},
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

/// How much of the width the list takes, and the range it is held to.
///
/// Proportional rather than fixed: a fixed 40 columns is a third of a laptop
/// terminal and a fifth of a wide one, so titles were being cut short beside a
/// mostly empty description pane.
const LIST_SHARE: u16 = 38;
const LIST_MIN: u16 = 32;
const LIST_MAX: u16 = 58;

/// Everything on a row that is not the title: the marker, the gap, the project
/// column and both borders.
const ROW_FURNITURE: usize = 16;

/// A row in the left column. Headings are not selectable, which is the only
/// reason the list is not just the task vector.
enum Row<'a> {
    Heading(&'a str),
    Task(usize),
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &[Task],
    selected: usize,
    showing_done: bool,
    theme: Theme,
) {
    let list_width = (area.width * LIST_SHARE / 100).clamp(LIST_MIN, LIST_MAX).min(area.width);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(list_width), Constraint::Min(0)])
        .split(area);

    render_list(frame, columns[0], tasks, selected, showing_done, theme);
    render_detail(frame, columns[1], tasks.get(selected), theme);
}

/// Groups the list under its own sort order, so the shape of the sort is
/// visible rather than something you have to infer from the markers.
fn rows<'a>(tasks: &[Task]) -> Vec<Row<'a>> {
    let mut rows = Vec::with_capacity(tasks.len() + 4);
    let mut group: Option<(Status, Priority)> = None;

    for (index, task) in tasks.iter().enumerate() {
        let here = (task.status, task.priority);
        if group != Some(here) {
            rows.push(Row::Heading(match here {
                (Status::Done, _) => "done",
                (_, Priority::High) => "high",
                (_, Priority::Normal) => "normal",
                (_, Priority::Low) => "low",
            }));
            group = Some(here);
        }
        rows.push(Row::Task(index));
    }
    rows
}

fn render_list(
    frame: &mut Frame,
    area: Rect,
    tasks: &[Task],
    selected: usize,
    showing_done: bool,
    theme: Theme,
) {
    let open = tasks.iter().filter(|task| task.status == Status::Open).count();
    let heading = match open {
        0 if tasks.is_empty() => " tasks ".to_string(),
        0 => " nothing open ".to_string(),
        1 => " 1 open ".to_string(),
        _ => format!(" {open} open "),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.dim))
        .title(Span::styled(
            heading,
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            if showing_done { " showing done " } else { "" },
            Style::default().fg(theme.dim),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if tasks.is_empty() {
        frame.render_widget(Paragraph::new(empty_state(showing_done, theme)), inner);
        return;
    }

    let rows = rows(tasks);
    let cursor = rows
        .iter()
        .position(|row| matches!(row, Row::Task(index) if *index == selected))
        .unwrap_or(0);

    let visible = inner.height as usize;
    let start = cursor
        .saturating_sub(visible.saturating_sub(1) / 2)
        .min(rows.len().saturating_sub(visible));

    let lines: Vec<Line<'_>> = rows
        .iter()
        .skip(start)
        .take(visible)
        .map(|row| match row {
            Row::Heading(label) => Line::from(Span::styled(
                format!(" {label}"),
                Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
            )),
            Row::Task(index) => {
                let line = entry(&tasks[*index], inner.width as usize, theme);
                if *index == selected {
                    keycap::fill(line, inner.width).style(keycap::selected_row(theme))
                } else {
                    line
                }
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Priority takes the attention hue rather than a new one. "This wants you" is
/// what `attention` already means on the session cards, and a high-priority
/// task is the same claim about a different object — see `ui::theme`.
const fn priority_colour(task: &Task, theme: Theme) -> ratatui::style::Color {
    match (task.status, task.priority) {
        (Status::Done, _) | (_, Priority::Low) => theme.dim,
        (_, Priority::High) => theme.attention,
        (_, Priority::Normal) => theme.text,
    }
}

fn entry<'a>(task: &Task, width: usize, theme: Theme) -> Line<'a> {
    let colour = priority_colour(task, theme);

    let title = if task.status == Status::Done {
        Style::default().fg(theme.dim).add_modifier(Modifier::CROSSED_OUT)
    } else {
        Style::default().fg(theme.text)
    };

    // Padded rather than just truncated, so the projects form a column. A
    // ragged right edge on twenty rows reads as noise, and which project a
    // task belongs to is the question you scan for.
    let budget = width.saturating_sub(ROW_FURNITURE).max(8);
    let name = truncate(&task.title, budget);
    let padding = " ".repeat(budget.saturating_sub(name.chars().count()));

    let mut spans = vec![
        Span::styled(format!(" {} ", task.priority.marker()), Style::default().fg(colour)),
        Span::styled(name, title),
    ];

    if let Some(project) = &task.project {
        spans.push(Span::raw(padding));
        spans.push(Span::styled(
            format!("  {}", truncate(project, 10)),
            Style::default().fg(theme.link),
        ));
    }

    Line::from(spans)
}

fn empty_state<'a>(showing_done: bool, theme: Theme) -> Vec<Line<'a>> {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Nothing to do",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    // Two different silences. One of them is only a filter.
    if showing_done {
        lines.push(Line::from(Span::styled(
            "  Tasks/ is empty. Agents can write here too.",
            Style::default().fg(theme.dim),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  Nothing open — press a to include done ones.",
            Style::default().fg(theme.dim),
        )));
    }

    lines.push(Line::from(""));
    let mut row = keycap::row(&[("n", "new task")], keycap::Caps::plain(theme), theme);
    row.spans.insert(0, Span::raw("  "));
    lines.push(row);
    lines
}

fn render_detail(frame: &mut Frame, area: Rect, task: Option<&Task>, theme: Theme) {
    let title = task.map_or_else(
        || " no task ".to_string(),
        |task| {
            task.path
                .file_name()
                .map_or_else(String::new, |name| format!(" Tasks/{} ", name.to_string_lossy()))
        },
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.dim))
        .title(Span::styled(title, Style::default().fg(theme.dim).add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(task) = task else {
        let hint = Line::from(Span::styled(
            "One markdown file per task, in the vault's Tasks/ folder",
            Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
        ));
        let padding = inner.height.saturating_sub(1) / 2;
        frame.render_widget(
            Paragraph::new(hint).alignment(Alignment::Center),
            Rect { y: inner.y + padding, height: 1.min(inner.height), ..inner },
        );
        return;
    };

    let mut lines = vec![
        Line::from(Span::styled(
            task.title.clone(),
            Style::default().fg(theme.heading).add_modifier(Modifier::BOLD),
        )),
        metadata(task, theme),
        Line::from(""),
    ];

    // Parsed here rather than cached, unlike a note: `markdown::parse` is
    // cached in the editor because the real vault has a 184 KB log in it, and
    // a task description is a paragraph.
    let description = task.description();
    if description.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            "No description. Press e to write one.",
            Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
        )));
    } else {
        lines.extend(crate::vault::markdown::parse(description, theme).lines);
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// Status, priority, project and tags on one line, in that order.
fn metadata<'a>(task: &Task, theme: Theme) -> Line<'a> {
    let (label, colour) = match task.status {
        Status::Open => ("open", theme.running),
        Status::Done => ("done", theme.dim),
    };

    let mut spans = vec![
        Span::styled(label.to_string(), Style::default().fg(colour).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("  {} {}", task.priority.marker(), task.priority.key()),
            Style::default().fg(priority_colour(task, theme)),
        ),
    ];

    if let Some(project) = &task.project {
        spans.push(Span::styled(format!("  {project}"), Style::default().fg(theme.link)));
    }
    for tag in &task.tags {
        spans.push(Span::styled(format!("  #{tag}"), Style::default().fg(theme.dim)));
    }

    Line::from(spans)
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit.saturating_sub(1)).chain(std::iter::once('…')).collect()
}
