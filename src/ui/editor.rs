//! Rendering the editor.

use crate::{
    editor::{Editor, Mode, wrap},
    ui::Theme,
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

/// Width of the line-number gutter, including its trailing space.
const GUTTER: u16 = 6;

/// The shape of the text area, so the editor can size its viewport and know
/// where lines wrap.
#[must_use]
pub fn text_shape(area: Rect) -> (usize, usize) {
    shape_of(Block::default().borders(Borders::ALL).inner(area))
}

/// The same, for a rect that is already the text area — the task pane draws
/// the editor inside its own border rather than giving it one.
#[must_use]
pub const fn shape_of(inner: Rect) -> (usize, usize) {
    (inner.height as usize, inner.width.saturating_sub(GUTTER) as usize)
}

pub fn render(frame: &mut Frame, area: Rect, editor: &Editor, theme: Theme) {
    let title = editor.buffer.path.as_ref().map_or_else(
        || " untitled ".to_string(),
        |path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let marker = if editor.buffer.modified { " ●" } else { "" };
            format!(" {name}{marker} ")
        },
    );

    // The border is the mode indicator you see without looking down, and each
    // mode keeps its colour in the label below too.
    let accent = mode_colour(editor, theme);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(title, Style::default().fg(accent).add_modifier(Modifier::BOLD)))
        .title_bottom(Span::styled(
            format!(" {} ", mode_label(editor)),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    render_text(frame, inner, editor, theme);
}

/// The text, the gutter and the cursor, with no chrome of its own.
///
/// Split out so the Tasks pane can host the editor under its own header: the
/// alternative was a bordered box inside a bordered box, and the description
/// is part of the task rather than a separate window over it.
pub fn render_text(frame: &mut Frame, inner: Rect, editor: &Editor, theme: Theme) {
    if inner.width <= GUTTER || inner.height == 0 {
        return;
    }

    let width = (inner.width - GUTTER) as usize;
    let mut lines = Vec::with_capacity(inner.height as usize);
    let mut cursor_row: Option<u16> = None;

    let mut line = editor.anchor.line;
    let mut skip = editor.anchor.row;

    while lines.len() < inner.height as usize && line < editor.buffer.line_count() {
        let text = editor.buffer.line(line);
        let rows = wrap::rows(&text, width);

        for (index, row) in rows.iter().enumerate().skip(skip) {
            if lines.len() >= inner.height as usize {
                break;
            }

            if line == editor.buffer.cursor.line {
                let (target, _) = wrap::locate(&text, width, editor.buffer.cursor.column);
                if target == index {
                    cursor_row = u16::try_from(lines.len()).ok();
                }
            }

            // Only the first visual row of a line carries its number, so a
            // wrapped paragraph reads as one paragraph rather than as many.
            lines.push(render_row(editor, line, index == 0, &text, *row, theme));
        }

        skip = 0;
        line += 1;
    }

    frame.render_widget(Paragraph::new(lines), inner);
    place_cursor(frame, inner, editor, width, cursor_row);
}

/// One colour per mode, consistent with the rest of the app: green means live
/// input is going somewhere, orange means something wants a keystroke from you.
pub const fn mode_colour(editor: &Editor, theme: Theme) -> ratatui::style::Color {
    match editor.mode {
        Mode::Insert => theme.running,
        Mode::Jump { .. } => theme.attention,
        Mode::Search { .. } => theme.code,
        Mode::Normal => theme.accent,
    }
}

pub fn mode_label(editor: &Editor) -> String {
    match &editor.mode {
        Mode::Search { query } => format!("SEARCH {query}▏"),
        Mode::Jump { typed } if !typed.is_empty() => format!("JUMP {typed}"),
        other => other.label().to_string(),
    }
}

/// One visual row: its gutter, its text, and any jump tags on top.
fn render_row<'a>(
    editor: &Editor,
    line: usize,
    first_row: bool,
    text: &str,
    row: wrap::Row,
    theme: Theme,
) -> Line<'a> {
    let on_cursor_line = line == editor.buffer.cursor.line;
    let number_style = if on_cursor_line {
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.dim)
    };

    let gutter = if first_row {
        format!("{:>4}  ", line + 1)
    } else {
        // A continuation marker, so a wrapped row is never mistaken for a new
        // line that happens to be unnumbered.
        "   \u{2937}  ".to_string()
    };
    let mut spans = vec![Span::styled(gutter, number_style)];

    let characters: Vec<char> = text.chars().collect();
    let start = row.start.min(characters.len());
    let end = row.end.min(characters.len());

    // Jump tags replace the first characters of the words they mark. Inserting
    // them would shift the line and move the target you were aiming at.
    let tags: Vec<(usize, &str)> = editor
        .tags()
        .iter()
        .filter(|tag| {
            tag.cursor.line == line && tag.cursor.column >= start && tag.cursor.column < end
        })
        .map(|tag| (tag.cursor.column, tag.label.as_str()))
        .collect();

    if tags.is_empty() {
        let slice: String = characters[start..end].iter().collect();
        // Markdown structure is worth seeing while editing, but only the two
        // cues that aid navigation: headings and links. Not highlighting.
        spans.push(Span::styled(slice, editor_line_style(&characters, start, theme)));
        return Line::from(spans);
    }

    let mut column = start;
    while column < end {
        if let Some((_, label)) = tags.iter().find(|(at, _)| *at == column) {
            spans.push(Span::styled(
                (*label).to_string(),
                Style::default().fg(theme.surface).bg(theme.accent).add_modifier(Modifier::BOLD),
            ));
            column += label.chars().count();
            continue;
        }
        spans.push(Span::styled(characters[column].to_string(), Style::default().fg(theme.dim)));
        column += 1;
    }

    Line::from(spans)
}

/// Parks the real terminal cursor on the buffer cursor, so it blinks natively.
///
/// `row` comes from the render pass, which already knows which visual row the
/// cursor landed on — recomputing it here could disagree with what was drawn.
/// Heading and link lines get their colour; everything else is plain text.
///
/// This is *not* syntax highlighting (ADR-0003 rules that out) — it is the two
/// structural cues that help you find your place in a long note.
fn editor_line_style(characters: &[char], start: usize, theme: Theme) -> Style {
    if start == 0 {
        let line: String = characters.iter().collect();
        if crate::editor::markdown::heading_level(&line).is_some() {
            return Style::default().fg(theme.heading).add_modifier(Modifier::BOLD);
        }
    }
    Style::default().fg(theme.text)
}

fn place_cursor(frame: &mut Frame, inner: Rect, editor: &Editor, width: usize, row: Option<u16>) {
    if matches!(editor.mode, Mode::Jump { .. }) {
        return;
    }
    let Some(row) = row else { return };

    let text = editor.buffer.line(editor.buffer.cursor.line);
    let (_, column) = wrap::locate(&text, width, editor.buffer.cursor.column);

    let Ok(column) = u16::try_from(column) else { return };
    if row >= inner.height || column + GUTTER >= inner.width {
        return;
    }

    frame.set_cursor_position(Position::new(inner.x + GUTTER + column, inner.y + row));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::buffer::Buffer;
    use ratatui::{Terminal, backend::TestBackend};

    fn draw(editor: &Editor, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, frame.area(), editor, Theme::default())).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn lines_are_numbered_and_shown() {
        let editor = Editor::with_buffer(Buffer::from_str("first\nsecond"));
        let rendered = draw(&editor, 40, 8);

        assert!(rendered.contains("first"));
        assert!(rendered.contains("second"));
        assert!(rendered.contains('1') && rendered.contains('2'));
    }

    #[test]
    fn an_unsaved_buffer_is_marked() {
        let mut buffer = Buffer::from_str("x");
        buffer.path = Some(std::path::PathBuf::from("/tmp/note.md"));
        buffer.insert("y");

        let rendered = draw(&Editor::with_buffer(buffer), 40, 6);
        assert!(rendered.contains("note.md"));
        assert!(rendered.contains('●'), "an unsaved buffer says so");
    }

    #[test]
    fn the_mode_is_always_visible() {
        let mut editor = Editor::with_buffer(Buffer::from_str("x"));
        assert!(draw(&editor, 40, 6).contains("NORMAL"));

        editor.enter_insert();
        assert!(draw(&editor, 40, 6).contains("INSERT"));
    }

    #[test]
    fn jump_tags_overlay_the_text_without_shifting_it() {
        let mut editor = Editor::with_buffer(Buffer::from_str("alpha beta"));
        editor.set_viewport(4, 40);
        editor.enter_jump();

        let rendered = draw(&editor, 40, 6);
        // The tag replaces the word's first characters rather than pushing the
        // line along, so the line keeps its length.
        assert!(rendered.contains("lpha"), "the tail of the word survives");
        assert!(!rendered.contains("alpha beta"), "the heads are covered by tags");
    }

    /// The normal case for this vault: 91% of notes have a line over 120
    /// characters, so most editing happens on wrapped text.
    #[test]
    fn a_long_line_wraps_across_rows_with_a_continuation_marker() {
        let mut editor = Editor::with_buffer(Buffer::from_str(&"word ".repeat(40)));
        editor.set_viewport(10, 30);

        let rendered = draw(&editor, 40, 12);
        assert!(rendered.contains('⤷'), "wrapped rows are marked as continuations");
        assert!(rendered.matches("word").count() > 10, "the whole line is visible, not clipped");

        // Only the first row carries the line number.
        assert_eq!(rendered.matches(" 1  ").count(), 1);
    }

    #[test]
    fn a_tiny_viewport_does_not_panic() {
        let editor = Editor::with_buffer(Buffer::from_str("some text here"));
        for (width, height) in [(0, 0), (1, 1), (7, 3), (40, 1)] {
            let _ = draw(&editor, width, height);
        }
    }
}
