//! The Tasks view: what to do on the left, the one you are pointing at on the
//! right.
//!
//! Same split as the Vault view, for the same reason — the list answers "what
//! is there" and the pane answers "what is this", and needing a keystroke
//! between those two questions makes a list of twenty things unreadable.
//!
//! **A task is a card rather than a row.** The first version put the title,
//! the project and the priority marker on one line, which meant the title got
//! whatever was left: about twenty characters, so "Rotate refresh tokens on
//! use" arrived as "Rotate refresh token…". A task's title is the only part of
//! it you read while scanning, and it was the part being cut. Two lines and a
//! gap costs a third of the visible list and buys the whole title plus room to
//! say the status and the tags in words instead of in glyphs.

use crate::{
    app::App,
    editor::Editor,
    ui::{Theme, editor as editor_ui, keycap, powerline::Glyphs},
    vault::tasks::{Priority, Status, Task},
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

/// How much of the width the list takes, and the range it is held to.
///
/// Proportional rather than fixed: a fixed column is a third of a laptop
/// terminal and a fifth of a wide one, so titles were being cut short beside a
/// mostly empty description pane.
const LIST_SHARE: u16 = 42;
const LIST_MIN: u16 = 34;
const LIST_MAX: u16 = 66;

/// Rows a card occupies: the title, its details, and air.
///
/// The gap is outside the selection fill on purpose. Filling three rows makes
/// a block; filling two and leaving one makes a card with space around it,
/// which is what lets a list of them be read down rather than parsed.
const CARD_HEIGHT: usize = 3;

/// Where the details line starts, under the title rather than under the caret.
const INDENT: &str = "   ";

/// Rows the pane spends before the body: air, the stat bar, air.
///
/// The title is not up here any more. It is the body's first heading, which
/// means it is inside the part you can edit — so renaming a task is typing
/// over its heading, and the pane shows it exactly where markdown would.
///
/// A constant rather than something measured, because the event loop sizes the
/// editor to the body area *before* the frame is drawn. `ui::layout` exists
/// for the same reason.
const HEADER_HEIGHT: u16 = 3;

/// The list column's width for a given body.
///
/// Shared with [`description_area`] so the two cannot disagree about where the
/// pane begins.
fn list_width(area: Rect) -> u16 {
    (area.width * LIST_SHARE / 100).clamp(LIST_MIN, LIST_MAX).min(area.width)
}

/// The pane, given the whole body.
fn pane_of(area: Rect) -> Rect {
    let list = list_width(area);
    Rect { x: area.x + list, width: area.width.saturating_sub(list), ..area }
}

/// The text area inside the pane, one column in from its border on each side.
fn text_of(pane: Rect) -> Rect {
    let inner = Block::default().borders(Borders::ALL).inner(pane);
    Rect { x: inner.x + 1, width: inner.width.saturating_sub(2), ..inner }
}

/// The body's rectangle inside the pane, below the stat bar.
fn body_of(pane: Rect) -> Rect {
    let text = text_of(pane);
    Rect { y: text.y + HEADER_HEIGHT, height: text.height.saturating_sub(HEADER_HEIGHT), ..text }
}

/// Where the body is drawn, given the whole layout body, so the event loop can
/// size the editor to the same rectangle the renderer will use.
#[must_use]
pub fn description_area(area: Rect) -> Rect {
    body_of(pane_of(area))
}

pub fn render(frame: &mut Frame, area: Rect, app: &App, glyphs: Glyphs, theme: Theme) {
    let list = list_width(area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(list), Constraint::Min(0)])
        .split(area);

    // Only while a description is open. `app.editor` can hold a note the Vault
    // view is showing, and drawing that here would put somebody else's file in
    // the task pane.
    let editing = app.task_edit.as_ref().and(app.editor.as_ref());

    // A rename shows in the list as it is typed. The heading being edited is
    // the title, and watching one change while the other does not would say
    // they are two different things.
    let renaming = editing.zip(app.selected_task()).map(|(editor, task)| {
        crate::vault::tasks::title_from_body(&task.path, &editor.buffer.text())
    });

    render_list(frame, columns[0], app, renaming.as_deref(), theme);
    render_detail(frame, columns[1], app.selected_task(), editing, glyphs, theme);
}

fn render_list(frame: &mut Frame, area: Rect, app: &App, renaming: Option<&str>, theme: Theme) {
    let (tasks, selected, showing_done) = (&app.tasks, app.task_selected, app.tasks_show_finished);

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
        // A sort and a filter are modes, and a mode you cannot see is a mode
        // you will be surprised by. Both live on the bottom edge, out of the
        // way of the count but never hidden.
        .title_bottom(Span::styled(
            format!(
                " by {}{} ",
                app.task_order.label(),
                if showing_done { " · including finished" } else { "" }
            ),
            Style::default().fg(theme.dim),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if tasks.is_empty() {
        frame.render_widget(Paragraph::new(empty_state(showing_done, theme)), inner);
        return;
    }

    let visible = (inner.height as usize / CARD_HEIGHT).max(1);
    let start = selected
        .saturating_sub(visible.saturating_sub(1) / 2)
        .min(tasks.len().saturating_sub(visible));

    let mut lines = Vec::with_capacity(visible * CARD_HEIGHT);
    for (index, task) in tasks.iter().enumerate().skip(start).take(visible) {
        let chosen = index == selected;
        let title = if chosen { renaming.unwrap_or(&task.title) } else { &task.title };
        for line in card(task, title, chosen, inner.width as usize, theme) {
            lines.push(if chosen {
                keycap::fill(line, inner.width).style(keycap::selected_row(theme))
            } else {
                line
            });
        }
        lines.push(Line::from(""));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Priority takes the attention hue rather than a new one. "This wants you" is
/// what `attention` already means on the session cards, and a high-priority
/// task is the same claim about a different object — see `ui::theme`.
const fn priority_colour(task: &Task, theme: Theme) -> Color {
    match (task.status, task.priority) {
        (Status::Done | Status::Cancelled, _) | (_, Priority::Low) => theme.dim,
        (_, Priority::High) => theme.attention,
        (_, Priority::Normal) => theme.text,
    }
}

/// Three weights, not four colours.
///
/// Live, waiting, and over. `backlog` takes ordinary text because it is real
/// work you have agreed to, just not now — dimming it alongside the finished
/// ones would file it with the things nobody is going to look at again. The
/// word is right there for the rest of the distinction, which is what words
/// are for.
const fn status_colour(status: Status, theme: Theme) -> Color {
    match status {
        Status::Open => theme.running,
        Status::Backlog => theme.text,
        Status::Done | Status::Cancelled => theme.dim,
    }
}

/// One task as two lines: what it is, then everything about it.
fn card<'a>(task: &Task, title: &str, chosen: bool, width: usize, theme: Theme) -> [Line<'a>; 2] {
    // Struck through once it is decided, either way. A cancelled task is not
    // a failure worth a colour; it is simply no longer on the list, which is
    // the same thing a finished one is.
    let title_style = if task.status.is_finished() {
        Style::default().fg(theme.dim).add_modifier(Modifier::CROSSED_OUT)
    } else {
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    };

    // The title gets the line to itself, minus the caret. It is the only part
    // of a task you read while scanning, so it is the last thing to be cut.
    let heading = Line::from(vec![
        Span::styled(if chosen { " \u{25b8} " } else { INDENT }, Style::default().fg(theme.accent)),
        Span::styled(truncate(title, width.saturating_sub(4)), title_style),
    ]);

    let mut details = vec![
        Span::raw(INDENT),
        Span::styled(
            format!("{} {}", task.priority.marker(), task.priority.key()),
            Style::default().fg(priority_colour(task, theme)),
        ),
        Span::raw("  "),
        Span::styled(
            task.status.key(),
            Style::default().fg(status_colour(task.status, theme)).add_modifier(Modifier::BOLD),
        ),
    ];

    // Project and tags in the order you ask for them, and dropped rather than
    // squeezed when the column is narrow — a half-written tag is noise.
    let mut room = width.saturating_sub(details.iter().map(Span::width).sum::<usize>());

    if let Some(project) = &task.project {
        let text = format!("  {project}");
        if text.chars().count() <= room {
            room -= text.chars().count();
            details.push(Span::styled(text, Style::default().fg(theme.link)));
        }
    }

    for tag in &task.tags {
        let text = format!("  #{tag}");
        if text.chars().count() > room {
            break;
        }
        room -= text.chars().count();
        details.push(Span::styled(text, Style::default().fg(theme.dim)));
    }

    [heading, Line::from(details)]
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
            "  Nothing live — press a to include finished ones.",
            Style::default().fg(theme.dim),
        )));
    }

    lines.push(Line::from(""));
    let mut row = keycap::row(&[("n", "new task")], keycap::Caps::plain(theme), theme);
    row.spans.insert(0, Span::raw("  "));
    lines.push(row);
    lines
}

/// A block in the stat bar: some text, and the colour it sits on.
struct Chip {
    label: String,
    background: Color,
    text: Color,
}

/// Draws the chips as one flowing run, or as separated words without the font.
///
/// The separator is drawn in the *outgoing* chip's background on the
/// *incoming* chip's background — the same rule as the tab strip, and getting
/// those two the wrong way round is the classic seam.
fn stat_bar<'a>(chips: &[Chip], glyphs: Glyphs, theme: Theme) -> Line<'a> {
    if !glyphs.segmented {
        let mut spans = Vec::with_capacity(chips.len() * 2);
        for (index, chip) in chips.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled("  ·  ", Style::default().fg(theme.dim)));
            }
            spans.push(Span::styled(
                chip.label.clone(),
                Style::default().fg(chip.background).add_modifier(Modifier::BOLD),
            ));
        }
        return Line::from(spans);
    }

    let mut spans = Vec::with_capacity(chips.len() * 2 + 2);
    for (index, chip) in chips.iter().enumerate() {
        // The bar's own background stands in for a chip that is not there.
        let before =
            index.checked_sub(1).map_or(theme.surface, |previous| chips[previous].background);
        let separator = if index == 0 { glyphs.notch } else { glyphs.cap };
        spans.push(Span::styled(
            separator,
            Style::default()
                .fg(if index == 0 { chip.background } else { before })
                .bg(if index == 0 { theme.surface } else { chip.background }),
        ));
        spans.push(Span::styled(
            format!(" {} ", chip.label),
            Style::default().fg(chip.text).bg(chip.background).add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(last) = chips.last() {
        spans
            .push(Span::styled(glyphs.cap, Style::default().fg(last.background).bg(theme.surface)));
    }
    Line::from(spans)
}

/// Everything known about the task, as one bar.
///
/// Status, priority, project and tags each get their own block, in the order
/// you would ask for them. A block that has nothing to say is absent rather
/// than empty — "no project" is not a fact worth a segment.
fn chips(task: &Task, theme: Theme) -> Vec<Chip> {
    let mut chips = vec![
        Chip {
            label: task.status.key().to_string(),
            background: status_colour(task.status, theme),
            text: theme.surface,
        },
        Chip {
            label: format!("{} {}", task.priority.marker(), task.priority.key()),
            background: priority_colour(task, theme),
            text: theme.surface,
        },
    ];

    if let Some(project) = &task.project {
        chips.push(Chip { label: project.clone(), background: theme.link, text: theme.surface });
    }

    if !task.tags.is_empty() {
        // The one block that is not a status, so it takes a surface tone and
        // keeps ordinary text on it rather than inverting like the rest.
        chips.push(Chip {
            label: task.tags.iter().map(|tag| format!("#{tag}")).collect::<Vec<_>>().join(" "),
            background: theme.highlight,
            text: theme.text,
        });
    }

    chips
}

fn render_detail(
    frame: &mut Frame,
    area: Rect,
    task: Option<&Task>,
    editing: Option<&Editor>,
    glyphs: Glyphs,
    theme: Theme,
) {
    let name = task.map_or_else(
        || " no task ".to_string(),
        |task| {
            task.path
                .file_name()
                .map_or_else(String::new, |name| format!(" Tasks/{} ", name.to_string_lossy()))
        },
    );

    // Editing recolours the pane's own border rather than opening a box inside
    // it. The mode colour is the one you read without looking down, and a
    // second border around the description would say "different window" about
    // something that is part of this one.
    let accent = editing.map_or(theme.dim, |editor| editor_ui::mode_colour(editor, theme));

    let title = if editing.is_some_and(|editor| editor.buffer.modified) {
        format!("{} \u{25cf} ", name.trim_end())
    } else {
        name
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(title, Style::default().fg(accent).add_modifier(Modifier::BOLD)));

    if let Some(editor) = editing {
        block = block.title_bottom(Span::styled(
            format!(" {} \u{2014} esc to finish ", editor_ui::mode_label(editor)),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    }

    // A column of air on the left, so the heading and the description are not
    // flush against the border. Done with the rect rather than by prefixing
    // every line, because the markdown renderer produces lines of its own.
    let inner = text_of(area);
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

    // Exactly HEADER_HEIGHT rows, and the event loop relies on that to size
    // the editor before this runs.
    let header = vec![Line::from(""), stat_bar(&chips(task, theme), glyphs, theme), Line::from("")];
    frame.render_widget(
        Paragraph::new(header),
        Rect { height: HEADER_HEIGHT.min(inner.height), ..inner },
    );

    let body = body_of(area);
    if body.height == 0 {
        return;
    }

    if let Some(editor) = editing {
        editor_ui::render_text(frame, body, editor, theme);
        return;
    }

    // Parsed here rather than cached, unlike a note: `markdown::parse` is
    // cached in the editor because the real vault has a 184 KB log in it, and
    // a task body is a paragraph.
    let mut lines = crate::vault::markdown::parse(task.body(), theme).lines;

    if task.description().trim().is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "No description. Press e to write one.",
            Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
        )));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body);
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit.saturating_sub(1)).chain(std::iter::once('…')).collect()
}
