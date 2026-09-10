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

    // Scrollback lines have *negative* line numbers: `display_iter` starts at
    // `Line(-display_offset - 1)` and counts up through the live screen. So a
    // grid line maps to a viewport row by adding the offset back on.
    //
    // Getting this wrong is silent and confusing: `u16::try_from` on a negative
    // line simply fails, every history row is skipped, and the live rows draw
    // at the top — which looks like the bottom of the screen emptying out
    // rather than like scrolling.
    let offset = i32::try_from(content.display_offset).unwrap_or(0);
    let selection = content.selection;
    let buffer = frame.buffer_mut();

    for indexed in content.display_iter {
        let cell = indexed.cell;

        // The trailing half of a double-width character. The wide char itself
        // already occupies this column visually, so emitting anything here
        // would overwrite it.
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }

        let Some(row) = viewport_row(indexed.point.line.0, offset) else { continue };
        let Ok(column) = u16::try_from(indexed.point.column.0) else { continue };
        if row >= area.height || column >= area.width {
            continue;
        }

        let Some(target) = buffer.cell_mut(Position::new(area.x + column, area.y + row)) else {
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

        // Selected cells take the theme's own highlight, the same fill a
        // selected row gets everywhere else — selection is a filled shape, not
        // a colour, and copy mode is no exception.
        if selection.is_some_and(|range| range.contains(indexed.point)) {
            background = theme.highlight;
            foreground = theme.text;
        }

        // An untouched cell holds '\0', which would render as a hole.
        target.set_char(if cell.c == '\0' { ' ' } else { cell.c });
        target.set_style(
            Style::default().fg(foreground).bg(background).add_modifier(modifiers(cell.flags)),
        );
    }

    // The cursor lives in live-screen coordinates, so it moves down the
    // viewport as you scroll back, and off it entirely once you are far enough.
    if focused {
        let cursor = content.cursor.point;
        if let Some(row) = viewport_row(cursor.line.0, offset)
            && let Ok(column) = u16::try_from(cursor.column.0)
            && row < area.height
            && column < area.width
        {
            frame.set_cursor_position(Position::new(area.x + column, area.y + row));
        }
    }
}

/// Maps a grid line onto a viewport row.
///
/// `line` is negative for scrollback history and zero-or-positive for the live
/// screen; `offset` is how far back the view is scrolled. `None` means the line
/// sits above the viewport and should not be drawn.
const fn viewport_row(line: i32, offset: i32) -> Option<u16> {
    let row = line + offset;
    if row < 0 {
        return None;
    }
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "guarded by the negative check, and callers bound it by the area height"
    )]
    Some(row as u16)
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
            NamedColor::Background => theme.terminal_background,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this exists to prevent: scrollback lines are *negative*, and
    /// `u16::try_from` on a negative silently drops them. That looked like the
    /// bottom of the screen emptying out rather than like scrolling back.
    #[test]
    fn history_lines_map_onto_the_viewport_when_scrolled_back() {
        // Scrolled back 30 lines: grid line -30 is the top row of the view.
        assert_eq!(viewport_row(-30, 30), Some(0));
        assert_eq!(viewport_row(-29, 30), Some(1));
        assert_eq!(viewport_row(0, 30), Some(30), "the live screen sits below it");
    }

    #[test]
    fn nothing_is_dropped_when_the_view_is_live() {
        assert_eq!(viewport_row(0, 0), Some(0));
        assert_eq!(viewport_row(23, 0), Some(23));
    }

    #[test]
    fn lines_above_the_viewport_are_skipped_rather_than_wrapping() {
        // Further back than we are scrolled: genuinely off-screen.
        assert_eq!(viewport_row(-31, 30), None);
        assert_eq!(viewport_row(-1, 0), None);
    }

    #[test]
    fn every_row_of_a_scrolled_screen_is_accounted_for() {
        // Twenty-four rows scrolled back thirty: each grid line must land on
        // exactly one distinct viewport row, none dropped.
        let offset = 30;
        let rows: Vec<u16> =
            (-offset..(24 - offset)).filter_map(|line| viewport_row(line, offset)).collect();

        assert_eq!(rows.len(), 24, "a whole screen must render, not a partial one");
        assert_eq!(rows.first(), Some(&0));
        assert_eq!(rows.last(), Some(&23));
    }
}
