//! The Sessions view: a list of running children on the left, the focused
//! child's screen on the right.

use crate::{
    session::{Focus, Kind, Session, Sessions, State},
    ui::{Theme, keycap, terminal},
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

/// Width of the session list.
///
/// Sessions are renameable and each card carries its directory and branch, so
/// this is the column where you tell one agent's job from another's. Wide
/// enough for a real name — "payments auth refactor", not "payments au…" — and
/// for a path tail that identifies the project.
const LIST_WIDTH: u16 = 38;

/// Splits the view into (list, terminal).
///
/// Shared with the event loop, which needs the terminal rect to keep every
/// child's grid the same size as the area it is drawn into. If these two ever
/// disagree, output wraps in the wrong place.
pub fn split(area: Rect) -> (Rect, Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LIST_WIDTH), Constraint::Min(0)])
        .split(area);
    (columns[0], columns[1])
}

/// The inner area of the terminal pane, inside its border.
pub fn terminal_area(area: Rect) -> Rect {
    let (_, pane) = split(area);
    Block::default().borders(Borders::ALL).inner(pane)
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    sessions: &Sessions,
    renaming: Option<&str>,
    glyphs: crate::ui::powerline::Glyphs,
    theme: Theme,
) {
    let (list_area, pane_area) = split(area);
    render_list(frame, list_area, sessions, renaming, glyphs, theme);
    render_pane(frame, pane_area, sessions, theme);
}

/// What a card needs to know that is not the session.
///
/// A struct rather than four more parameters: `render_card` reached eight,
/// which is past the point where a call site tells you anything about what is
/// being passed.
#[derive(Clone, Copy)]
struct Chrome {
    glyphs: crate::ui::powerline::Glyphs,
    theme: Theme,
    /// Columns available to the card.
    width: usize,
}

/// Rows each session card occupies: name, directory, branch, and changes.
///
/// Fixed rather than varying with whether a session has uncommitted work. A
/// card that grew and shrank as its agent wrote files would make the whole
/// list jump under the cursor, and the windowing below divides by this to
/// decide what fits — two good reasons to pay one blank row for a clean tree.
const CARD_HEIGHT: usize = 4;

fn render_list(
    frame: &mut Frame,
    area: Rect,
    sessions: &Sessions,
    renaming: Option<&str>,
    glyphs: crate::ui::powerline::Glyphs,
    theme: Theme,
) {
    let heading = match sessions.len() {
        0 => " sessions ".to_string(),
        1 => " 1 session ".to_string(),
        count => format!(" {count} sessions "),
    };

    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme.dim))
        .title(Span::styled(heading, Style::default().fg(theme.dim)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if sessions.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  no sessions",
                Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
            ))),
            inner,
        );
        return;
    }

    let attached = sessions.focus() == Focus::Attached;
    let chrome = Chrome { glyphs, theme, width: inner.width as usize };

    // Window around the selection so a long list stays usable. Four rows per
    // card means a full-screen terminal shows roughly ten.
    let visible = (inner.height as usize / CARD_HEIGHT).max(1);
    let selected = sessions.selected_index();
    let start = selected.saturating_sub(visible.saturating_sub(1) / 2);
    let start = start.min(sessions.len().saturating_sub(visible));

    let mut lines = Vec::with_capacity(visible * CARD_HEIGHT);

    for (index, session) in sessions.iter().enumerate().skip(start).take(visible) {
        let chosen = index == selected;
        let card = render_card(session, index, chosen, attached, renaming, chrome);

        for line in card {
            lines.push(if chosen {
                keycap::fill(line, inner.width).style(keycap::selected_row(theme))
            } else {
                line
            });
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// The card's first row: which session this is, and how it is doing.
///
/// Split out because the card was over a hundred lines, and this is the half
/// with all the branching in it.
fn name_row<'a>(
    session: &Session,
    index: usize,
    chosen: bool,
    attached: bool,
    renaming: Option<&str>,
    chrome: Chrome,
) -> Line<'a> {
    let Chrome { theme, width, .. } = chrome;

    // The marker distinguishes "this is where the cursor is" from "your
    // keystrokes are going here", which are not the same thing.
    let marker = match (chosen, attached) {
        (true, true) => "▶ ",
        (true, false) => "· ",
        (false, _) => "  ",
    };

    let name_style = match (chosen, session.state) {
        // A session wanting you outranks the one you are looking at.
        (_, State::AwaitingInput) => {
            Style::default().fg(theme.attention).add_modifier(Modifier::BOLD)
        }
        (true, _) => Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        (false, State::Exited(_)) => Style::default().fg(theme.dim),
        (false, _) => Style::default().fg(theme.text),
    };

    // A frozen status outranks whatever it froze on: showing `●` for a session
    // that stopped reporting hours ago is worse than admitting we do not know.
    let badge = if session.status_is_stale() {
        Span::styled(" ?", Style::default().fg(theme.dim))
    } else {
        match session.state {
            State::Idle => Span::styled(" ○", Style::default().fg(theme.accent)),
            State::Running => Span::styled(" ●", Style::default().fg(theme.running)),
            State::AwaitingInput => Span::styled(
                " ◆",
                Style::default().fg(theme.attention).add_modifier(Modifier::BOLD),
            ),
            State::Exited(_) => Span::styled(" ×", Style::default().fg(theme.danger)),
        }
    };

    let kind = match session.kind {
        Kind::Agent { .. } => "",
        Kind::Shell => " $",
    };

    let ordinal = if index < 9 { format!("{} ", index + 1) } else { "  ".to_string() };

    // While renaming, the row itself becomes the field. No popup, and no
    // guessing which session you are renaming.
    if chosen && let Some(draft) = renaming {
        Line::from(vec![
            Span::styled(marker, Style::default().fg(theme.accent)),
            Span::styled(ordinal, Style::default().fg(theme.dim)),
            Span::styled(
                format!("{draft}▏"),
                Style::default().fg(theme.code).add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(marker, Style::default().fg(theme.accent)),
            Span::styled(ordinal, Style::default().fg(theme.dim)),
            Span::styled(truncate(&session.display_name(), width.saturating_sub(10)), name_style),
            Span::styled(kind, Style::default().fg(theme.dim)),
            badge,
        ])
    }
}

/// One session as four rows: what it is, where it runs, what branch it is on,
/// and what it has changed.
fn render_card<'a>(
    session: &Session,
    index: usize,
    chosen: bool,
    attached: bool,
    renaming: Option<&str>,
    chrome: Chrome,
) -> [Line<'a>; CARD_HEIGHT] {
    let Chrome { glyphs, theme, width } = chrome;
    let name = name_row(session, index, chosen, attached, renaming, chrome);

    // Where it runs. This is why the sidebar is worth three rows: with several
    // agents going, "which one is this" is answered by the path far more often
    // than by the name.
    let directory = crate::paths::contract_home(session.directory());
    let where_it_runs = Line::from(vec![
        Span::raw("    "),
        Span::styled(
            truncate_start(&directory, width.saturating_sub(5)),
            Style::default().fg(theme.dim),
        ),
    ]);

    // A worktree is worth saying out loud; a plain branch is context.
    let branch = match (&session.worktree, &session.branch) {
        (Some(worktree), _) => Line::from(vec![
            Span::raw("    "),
            Span::styled(format!("{} ", glyphs.branch), Style::default().fg(theme.link)),
            Span::styled(
                truncate(worktree, width.saturating_sub(15)),
                Style::default().fg(theme.link),
            ),
            Span::styled("  worktree", Style::default().fg(theme.dim)),
        ]),
        (None, Some(branch)) => Line::from(vec![
            Span::raw("    "),
            Span::styled(format!("{} ", glyphs.branch), Style::default().fg(theme.dim)),
            Span::styled(truncate(branch, width.saturating_sub(7)), Style::default().fg(theme.dim)),
        ]),
        (None, None) => Line::from(""),
    };

    // The answer to "what did this one actually do?", on a row of its own.
    //
    // This started dim and crammed onto the end of the branch row, on the
    // argument that green beside an attention-coloured spine would compete
    // with it. Given its own line that argument weakens: the number is no
    // longer fighting the branch for the same few columns, and `added` and
    // `removed` are already roles in their own right for the diff view. So it
    // reads the way a diff reads everywhere else.
    //
    // A clean tree still says nothing rather than "+0 −0". The absence of a
    // number is the fastest way to read "nothing yet".
    let changes = match &session.changes {
        Some(changes) if !changes.is_empty() => Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!("+{}", changes.insertions),
                Style::default().fg(theme.added).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!("−{}", changes.deletions),
                Style::default().fg(theme.removed).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  {}",
                    if changes.files == 1 {
                        "1 file".to_string()
                    } else {
                        format!("{} files", changes.files)
                    }
                ),
                Style::default().fg(theme.dim),
            ),
        ]),
        _ => Line::from(""),
    };

    [name, where_it_runs, branch, changes]
}

fn render_pane(frame: &mut Frame, area: Rect, sessions: &Sessions, theme: Theme) {
    let attached = sessions.focus() == Focus::Attached;

    let title = sessions.selected().map_or_else(
        || " Houston ".to_string(),
        |session| format!(" {} · {} ", session.display_name(), session.state.label()),
    );

    // Scrolled back, you are not looking at live output. Saying so is the
    // difference between "the agent has stopped" and "you scrolled up".
    let scrolled = sessions.selected().map_or(0, Session::scrollback_offset);

    // Green border while attached: keystrokes are going to the child.
    let border_style =
        if attached { Style::default().fg(theme.running) } else { Style::default().fg(theme.dim) };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Span::styled(title, border_style.add_modifier(Modifier::BOLD)));

    let block = if scrolled > 0 {
        block.title_bottom(Span::styled(
            format!(" ↑ {scrolled} lines back · type to return "),
            Style::default().fg(theme.attention).add_modifier(Modifier::BOLD),
        ))
    } else {
        block
    };

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(session) = sessions.selected() else {
        let hint = vec![
            Line::from(Span::styled(
                "No sessions yet",
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            keycap::row(&[("n", "start an agent"), ("s", "start a shell")], theme),
            Line::from(""),
            keycap::row(&[("W", "worktrees")], theme),
        ];
        let padding = inner.height.saturating_sub(5) / 2;
        let centred = Rect { y: inner.y + padding, height: 5.min(inner.height), ..inner };
        frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), centred);
        return;
    };

    if let Ok(term) = session.pty().term().lock() {
        terminal::render(frame, inner, &term, theme, attached);
    }

    if let State::Exited(code) = session.state {
        let message = code.map_or_else(
            || " exited ".to_string(),
            |code| format!(" exited with {code} — x to close "),
        );
        let width = u16::try_from(message.len()).unwrap_or(inner.width).min(inner.width);
        let banner = Rect { x: inner.x, y: inner.y, width, height: 1.min(inner.height) };
        frame.render_widget(
            Paragraph::new(Span::styled(
                message,
                Style::default().fg(theme.surface).bg(theme.danger).add_modifier(Modifier::BOLD),
            )),
            banner,
        );
    }
}

/// Truncates from the *front*, keeping the end.
///
/// For paths, the tail is what identifies it — `…/payments/api` says far more
/// than `/home/example/Proj…`.
fn truncate_start(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit || limit == 0 {
        return text.to_string();
    }
    std::iter::once('…').chain(text.chars().skip(count - limit.saturating_sub(1))).collect()
}

/// Truncates to a cell budget, with an ellipsis when it bites.
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
    fn short_names_are_left_alone() {
        assert_eq!(truncate("claude", 16), "claude");
        assert_eq!(truncate("exactly-sixteen!", 16), "exactly-sixteen!");
    }

    #[test]
    fn long_names_get_an_ellipsis_within_budget() {
        let truncated = truncate("a-very-long-session-name", 16);
        assert_eq!(truncated.chars().count(), 16);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        // Eight two-byte characters fit in a limit of eight cells.
        assert_eq!(truncate("αααααααα", 8).chars().count(), 8);
    }

    #[test]
    fn paths_are_truncated_from_the_front_so_the_tail_survives() {
        // The end of a path is what tells you which project it is.
        let truncated = truncate_start("~/Projects/payments/api", 12);
        assert_eq!(truncated.chars().count(), 12);
        assert!(truncated.starts_with('…'));
        assert!(truncated.ends_with("api"), "the identifying half must survive");

        assert_eq!(truncate_start("~/short", 20), "~/short", "short paths are left alone");
        assert_eq!(truncate_start("anything", 0), "anything", "a zero budget cannot ellipsize");
    }

    #[test]
    fn the_terminal_area_sits_inside_the_pane_border() {
        let area = Rect::new(0, 0, 100, 30);
        let (list, pane) = split(area);
        let inner = terminal_area(area);

        assert_eq!(list.width, LIST_WIDTH);
        assert!(inner.width < pane.width, "the border must cost width");
        assert!(inner.height < pane.height, "the border must cost height");
        assert!(inner.x >= pane.x && inner.y >= pane.y);
    }
}
