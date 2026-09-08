//! Renders an `alacritty_terminal` grid into a ratatui area.
//!
//! This is the bridge between a child process's idea of a screen and ours. It
//! is the part of the app most worth getting exactly right — everything the
//! user sees inside a session goes through here.

use crate::{palette, pty::Notifier, ui::Theme};
use alacritty_terminal::{
    term::{Term, cell::Flags},
    vte::ansi::{Color as TermColor, NamedColor},
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
};

/// Draws the terminal's visible screen into `area`.
///
/// When `focused`, the real cursor is parked over the child's cursor so it
/// blinks natively rather than being faked with an inverted cell.
pub fn render(frame: &mut Frame, area: Rect, term: &Term<Notifier>, theme: Theme, focused: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let content = term.renderable_content();
    let buffer = frame.buffer_mut();

    for indexed in content.display_iter {
        let cell = indexed.cell;

        // The trailing half of a double-width character. The wide char itself
        // already occupies this column visually, so emitting anything here
        // would overwrite it.
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }

        let Ok(line) = u16::try_from(indexed.point.line.0) else { continue };
        let Ok(column) = u16::try_from(indexed.point.column.0) else { continue };
        if line >= area.height || column >= area.width {
            continue;
        }

        let Some(target) = buffer.cell_mut(Position::new(area.x + column, area.y + line)) else {
            continue;
        };

        let mut foreground = convert(cell.fg, theme);
        let mut background = convert(cell.bg, theme);

        // INVERSE and HIDDEN are resolved here rather than passed through as
        // modifiers: ratatui's REVERSED is applied by the backend, which would
        // double-apply against a terminal that already inverts.
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut foreground, &mut background);
        }
        if cell.flags.contains(Flags::HIDDEN) {
            foreground = background;
        }

        // An untouched cell holds '\0', which would render as a hole.
        target.set_char(if cell.c == '\0' { ' ' } else { cell.c });
        target.set_style(
            Style::default().fg(foreground).bg(background).add_modifier(modifiers(cell.flags)),
        );
    }

    if focused {
        let cursor = content.cursor.point;
        if let (Ok(line), Ok(column)) =
            (u16::try_from(cursor.line.0), u16::try_from(cursor.column.0))
            && line < area.height
            && column < area.width
        {
            frame.set_cursor_position(Position::new(area.x + column, area.y + line));
        }
    }
}

fn modifiers(flags: Flags) -> Modifier {
    let mut modifier = Modifier::empty();
    if flags.contains(Flags::BOLD) {
        modifier |= Modifier::BOLD;
    }
    if flags.contains(Flags::ITALIC) {
        modifier |= Modifier::ITALIC;
    }
    if flags.intersects(Flags::ALL_UNDERLINES) {
        modifier |= Modifier::UNDERLINED;
    }
    if flags.contains(Flags::DIM) {
        modifier |= Modifier::DIM;
    }
    if flags.contains(Flags::STRIKEOUT) {
        modifier |= Modifier::CROSSED_OUT;
    }
    modifier
}

/// Maps a terminal colour onto a ratatui one.
///
/// Indexed and named palette colours are passed through as `Color::Indexed` so
/// the user's own terminal theme decides how they look — the same reason `ls`
/// output matches your colour scheme in any terminal. Only the special
/// foreground/background/cursor slots resolve to Houston's theme.
fn convert(color: TermColor, theme: Theme) -> Color {
    match color {
        TermColor::Spec(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
        TermColor::Indexed(index) => Color::Indexed(index),
        TermColor::Named(named) => match named {
            NamedColor::Foreground | NamedColor::BrightForeground => theme.text,
            NamedColor::Background => theme.surface,
            NamedColor::Cursor => theme.accent,
            NamedColor::DimForeground => theme.dim,
            other => {
                let index = other as usize;
                if index < 16 {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "guarded by the < 16 check on the line above"
                    )]
                    Color::Indexed(index as u8)
                } else {
                    // Dim variants live at 259+ and have no ratatui equivalent;
                    // resolve them through the palette instead of guessing.
                    let rgb = palette::xterm_256(index);
                    Color::Rgb(rgb.r, rgb.g, rgb.b)
                }
            }
        },
    }
}
