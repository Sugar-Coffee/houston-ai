//! Rendering the editor.

use crate::{
    editor::{Editor, Mode},
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

/// The rows available for text, so the editor can size its viewport.
#[must_use]
pub fn text_height(area: Rect) -> usize {
    Block::default().borders(Borders::ALL).inner(area).height as usize
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

    // The border colour is the mode indicator you see without looking down.
    let accent = match editor.mode {
        Mode::Insert => theme.accent,
        _ => theme.dim,
    };

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

    if inner.width <= GUTTER || inner.height == 0 {
        return;
    }

    let last = (editor.scroll + inner.height as usize).min(editor.buffer.line_count());
    let lines: Vec<Line> =
        (editor.scroll..last).map(|index| render_line(editor, index, theme)).collect();

    frame.render_widget(Paragraph::new(lines), inner);

    place_cursor(frame, inner, editor);
}

fn mode_label(editor: &Editor) -> String {
    match &editor.mode {
        Mode::Search { query } => format!("SEARCH {query}▏"),
        Mode::Jump { typed } if !typed.is_empty() => format!("JUMP {typed}"),
        other => other.label().to_string(),
    }
}

/// One buffer line, with its number and any jump tags sitting on top of it.
fn render_line<'a>(editor: &Editor, index: usize, theme: Theme) -> Line<'a> {
    let on_cursor_line = index == editor.buffer.cursor.line;

    let number_style = if on_cursor_line {
        Style::default().fg(theme.accent)
    } else {
        Style::default().fg(theme.dim)
    };
    let mut spans = vec![Span::styled(format!("{:>4}  ", index + 1), number_style)];

    let text = editor.buffer.line(index);

    // Jump tags replace the first characters of the words they mark, which is
    // what makes them readable — an inserted tag would shift the whole line and
    // make you re-find the target you were aiming at.
    let tags: Vec<(usize, &str)> = editor
        .tags()
        .iter()
        .filter(|tag| tag.cursor.line == index)
        .map(|tag| (tag.cursor.column, tag.label.as_str()))
        .collect();

    if tags.is_empty() {
        spans.push(Span::styled(text, Style::default().fg(theme.text)));
        return Line::from(spans);
    }

    let characters: Vec<char> = text.chars().collect();
    let mut column = 0;
    while column < characters.len() {
        if let Some((_, label)) = tags.iter().find(|(start, _)| *start == column) {
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
fn place_cursor(frame: &mut Frame, inner: Rect, editor: &Editor) {
    if matches!(editor.mode, Mode::Jump { .. }) {
        return;
    }

    let cursor = editor.buffer.cursor;
    if cursor.line < editor.scroll {
        return;
    }

    let Ok(row) = u16::try_from(cursor.line - editor.scroll) else { return };
    let Ok(column) = u16::try_from(cursor.column) else { return };
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
        editor.set_viewport(4);
        editor.enter_jump();

        let rendered = draw(&editor, 40, 6);
        // The tag replaces the word's first characters rather than pushing the
        // line along, so the line keeps its length.
        assert!(rendered.contains("lpha"), "the tail of the word survives");
        assert!(!rendered.contains("alpha beta"), "the heads are covered by tags");
    }

    #[test]
    fn a_tiny_viewport_does_not_panic() {
        let editor = Editor::with_buffer(Buffer::from_str("some text here"));
        for (width, height) in [(0, 0), (1, 1), (7, 3), (40, 1)] {
            let _ = draw(&editor, width, height);
        }
    }
}
