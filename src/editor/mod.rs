//! The markdown editor.
//!
//! ADR-0003 — modal, with amp's interaction model as the reference. amp is
//! Josh's daily `$EDITOR`, so "amp-style" is a testable target rather than an
//! aesthetic one: does this feel like the editor already in use.
//!
//! Deliberately **not** in scope: LSP, multi-cursor, macros, plugins, editor
//! split panes, or any language beyond markdown.

pub mod buffer;
pub mod jump;
pub mod markdown;
pub mod wrap;

use anyhow::Result;
use buffer::{Buffer, Cursor};
use jump::Tag;
use std::path::Path;

/// The top of the viewport, as a visual position.
///
/// A plain line number is not enough once lines wrap: one logical line in this
/// vault can be 50 visual rows tall, so scrolling has to be able to stop part
/// way down one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Anchor {
    pub line: usize,
    /// Visual row within that line.
    pub row: usize,
}

/// What the keyboard means right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Keys are commands. Where you spend most of your time.
    Normal,
    /// Keys are text.
    Insert,
    /// Two-character tags are showing; keys narrow them down.
    Jump { typed: String },
    /// Typing an incremental search.
    Search { query: String },
}

impl Mode {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Jump { .. } => "JUMP",
            Self::Search { .. } => "SEARCH",
        }
    }
}

pub struct Editor {
    pub buffer: Buffer,
    pub mode: Mode,
    /// Top of the viewport, kept in step with the cursor.
    pub anchor: Anchor,
    /// Live jump targets, rebuilt each time jump mode is entered.
    tags: Vec<Tag>,
    /// The last search, so `n` can repeat it.
    last_search: String,
    /// Rows the viewport can show, set from the render loop each frame.
    viewport: usize,
    /// Characters that fit across, which decides where lines wrap.
    width: usize,
    /// A close has been attempted with unsaved changes and is awaiting
    /// confirmation. Never faked by clearing `modified` — the unsaved marker
    /// must keep telling the truth while this is pending.
    pub close_armed: bool,
}

impl Editor {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self::with_buffer(Buffer::open(path)?))
    }

    #[must_use]
    pub const fn with_buffer(buffer: Buffer) -> Self {
        Self {
            buffer,
            mode: Mode::Normal,
            anchor: Anchor { line: 0, row: 0 },
            tags: Vec::new(),
            last_search: String::new(),
            viewport: 24,
            width: 80,
            close_armed: false,
        }
    }

    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    /// Tells the editor the shape of its viewport. Called from the render
    /// loop, because only it knows.
    pub const fn set_viewport(&mut self, rows: usize, width: usize) {
        self.viewport = if rows == 0 { 1 } else { rows };
        self.width = if width == 0 { 1 } else { width };
    }

    /// Visual rows a logical line occupies.
    fn line_height(&self, line: usize) -> usize {
        wrap::height(&self.buffer.line(line), self.width)
    }

    /// The cursor's position in visual terms: its line, and the row within it.
    fn cursor_visual_row(&self) -> usize {
        let line = self.buffer.line(self.buffer.cursor.line);
        wrap::locate(&line, self.width, self.buffer.cursor.column).0
    }

    /// The visual rows from the anchor down to the cursor.
    ///
    /// `None` when the cursor is above the anchor or further than a screenful
    /// below it — in both cases the answer is "re-anchor", and counting the
    /// exact distance across a 1,000-line file would be pointless work.
    fn rows_from_anchor(&self) -> Option<usize> {
        let cursor = self.buffer.cursor.line;
        if cursor < self.anchor.line {
            return None;
        }
        // Every line is at least one row, so this bounds the loop below.
        if cursor - self.anchor.line > self.viewport {
            return None;
        }

        let mut rows = 0;
        for line in self.anchor.line..cursor {
            rows += self.line_height(line);
            if rows > self.viewport {
                return None;
            }
        }
        rows += self.cursor_visual_row();
        rows.checked_sub(self.anchor.row)
    }

    /// Whether a close is waiting on confirmation.
    ///
    /// Arms on the first attempt with unsaved changes; any other key disarms,
    /// so closing by accident cannot lose work but closing on purpose still
    /// takes two keys and no dialog.
    pub const fn arm_close(&mut self) -> bool {
        if self.close_armed {
            return true;
        }
        self.close_armed = true;
        false
    }

    pub const fn disarm_close(&mut self) {
        self.close_armed = false;
    }

    /// Keeps the cursor on screen after any movement.
    pub fn follow_cursor(&mut self) {
        match self.rows_from_anchor() {
            // Already visible.
            Some(rows) if rows < self.viewport => {}
            // Below the fold: anchor so the cursor sits on the last row.
            Some(_) => self.anchor_above_cursor(self.viewport.saturating_sub(1)),
            // Above, or far away: put the cursor a little in from the top so
            // there is context above it rather than it hugging the edge.
            None => self.anchor_above_cursor(self.viewport / 4),
        }
    }

    /// Anchors so the cursor sits `margin` visual rows below the top.
    fn anchor_above_cursor(&mut self, margin: usize) {
        let mut line = self.buffer.cursor.line;
        let mut row = self.cursor_visual_row();
        let mut remaining = margin;

        while remaining > 0 {
            if row > 0 {
                let step = remaining.min(row);
                row -= step;
                remaining -= step;
            } else if line > 0 {
                line -= 1;
                row = self.line_height(line).saturating_sub(1);
                remaining -= 1;
            } else {
                break;
            }
        }

        self.anchor = Anchor { line, row };
    }

    /// Scrolls by whole visual rows, dragging the cursor to stay on screen.
    ///
    /// Paging by *logical* line would be useless here: the vault has a
    /// 153-line note averaging 840 characters a line, where one page-down
    /// could otherwise skip past a screenful of text.
    pub fn scroll_rows(&mut self, delta: isize) {
        if delta >= 0 {
            self.advance_anchor(delta.unsigned_abs());
        } else {
            self.retreat_anchor(delta.unsigned_abs());
        }

        // Drag the cursor along, or scrolled-then-typed text lands somewhere
        // surprising.
        if self.rows_from_anchor().is_none_or(|rows| rows >= self.viewport) {
            let target = self.anchor;
            let column = wrap::offset(&self.buffer.line(target.line), self.width, target.row, 0);
            self.buffer.move_to(Cursor { line: target.line, column });
        }
    }

    fn advance_anchor(&mut self, mut rows: usize) {
        let last = self.buffer.line_count().saturating_sub(1);

        while rows > 0 {
            let height = self.line_height(self.anchor.line);
            let room = height.saturating_sub(self.anchor.row + 1);

            if rows <= room {
                self.anchor.row += rows;
                return;
            }
            if self.anchor.line >= last {
                self.anchor.row = height.saturating_sub(1);
                return;
            }
            rows -= room + 1;
            self.anchor.line += 1;
            self.anchor.row = 0;
        }
    }

    fn retreat_anchor(&mut self, mut rows: usize) {
        while rows > 0 {
            if self.anchor.row > 0 {
                let step = rows.min(self.anchor.row);
                self.anchor.row -= step;
                rows -= step;
            } else if self.anchor.line > 0 {
                self.anchor.line -= 1;
                self.anchor.row = self.line_height(self.anchor.line).saturating_sub(1);
                rows -= 1;
            } else {
                return;
            }
        }
    }

    // --- markdown editing ---

    /// Return in insert mode, with list awareness.
    ///
    /// Continues the list you are in, and ends it when you press Return on an
    /// empty item. 308 of the vault's 328 notes contain a bullet list, so this
    /// is the single most-used editing convenience there is.
    pub fn insert_newline(&mut self) {
        let line = self.buffer.line(self.buffer.cursor.line);

        // Return on an empty item removes the marker instead of making another.
        if markdown::is_empty_item(&line) && self.buffer.cursor.column >= line.chars().count() {
            self.buffer.replace_line("");
            self.buffer.insert("\n");
            return;
        }

        let continuation = markdown::marker(&line)
            .filter(|marker| self.buffer.cursor.column >= marker.content_at)
            .map(|marker| marker.continuation());

        self.buffer.insert("\n");
        if let Some(prefix) = continuation {
            self.buffer.insert(&prefix);
        }
    }

    /// Ticks or unticks the task on the cursor's line.
    pub fn toggle_task(&mut self) -> bool {
        let line = self.buffer.line(self.buffer.cursor.line);
        let Some(updated) = markdown::toggle_task(&line) else { return false };
        self.buffer.replace_line(&updated);
        true
    }

    /// Moves to the next or previous heading.
    ///
    /// 325 of 328 notes have headings, which makes this the natural way to move
    /// through a long note — far more useful than paging through a build log.
    pub fn jump_heading(&mut self, forward: bool) -> bool {
        let count = self.buffer.line_count();
        let start = self.buffer.cursor.line;

        for offset in 1..=count {
            let line = if forward {
                (start + offset) % count
            } else {
                (start + count - offset % count) % count
            };
            if markdown::heading_level(&self.buffer.line(line)).is_some() {
                self.buffer.move_to(Cursor { line, column: 0 });
                self.follow_cursor();
                return true;
            }
        }
        false
    }

    /// The `[[wikilink]]` under the cursor, if there is one.
    #[must_use]
    pub fn link_under_cursor(&self) -> Option<String> {
        let line = self.buffer.line(self.buffer.cursor.line);
        markdown::link_at(&line, self.buffer.cursor.column)
    }

    /// Scrolls a screenful, keeping one row of overlap so you do not lose your
    /// place across the jump.
    pub fn page(&mut self, down: bool) {
        let step = isize::try_from(self.viewport.saturating_sub(1).max(1)).unwrap_or(1);
        self.scroll_rows(if down { step } else { -step });
    }

    /// Moves the cursor one visual row, which is not the same as one logical
    /// line once text wraps.
    pub fn move_visual(&mut self, down: bool) {
        let line = self.buffer.line(self.buffer.cursor.line);
        let (row, column) = wrap::locate(&line, self.width, self.buffer.cursor.column);
        let height = wrap::height(&line, self.width);

        if down && row + 1 < height {
            let offset = wrap::offset(&line, self.width, row + 1, column);
            self.buffer.move_to(Cursor { line: self.buffer.cursor.line, column: offset });
            return;
        }
        if !down && row > 0 {
            let offset = wrap::offset(&line, self.width, row - 1, column);
            self.buffer.move_to(Cursor { line: self.buffer.cursor.line, column: offset });
            return;
        }

        // Off the end of this line, so step to the neighbouring one and land
        // on its nearest row.
        let target_line = if down {
            let next = self.buffer.cursor.line + 1;
            if next >= self.buffer.line_count() {
                return;
            }
            next
        } else {
            if self.buffer.cursor.line == 0 {
                return;
            }
            self.buffer.cursor.line - 1
        };

        let target = self.buffer.line(target_line);
        let target_row = if down { 0 } else { wrap::height(&target, self.width) - 1 };
        let offset = wrap::offset(&target, self.width, target_row, column);
        self.buffer.move_to(Cursor { line: target_line, column: offset });
    }

    // --- modes ---

    pub fn enter_insert(&mut self) {
        self.mode = Mode::Insert;
    }

    pub fn enter_normal(&mut self) {
        self.mode = Mode::Normal;
        self.tags.clear();
    }

    /// Enters jump mode, tagging every word start on screen.
    ///
    /// Only what is actually visible gets tagged. With wrapping, that is far
    /// fewer logical lines than the viewport is tall.
    pub fn enter_jump(&mut self) {
        let mut lines = Vec::new();
        let mut rows = 0;
        let mut line = self.anchor.line;

        while rows < self.viewport && line < self.buffer.line_count() {
            let height = self.line_height(line);
            let skipped = if line == self.anchor.line { self.anchor.row } else { 0 };
            lines.push(self.buffer.line(line));
            rows += height.saturating_sub(skipped);
            line += 1;
        }

        self.tags = jump::tags(&lines, self.anchor.line);
        self.mode = Mode::Jump { typed: String::new() };
    }

    /// Feeds a character to jump mode. `true` once a jump has happened.
    pub fn jump_input(&mut self, character: char) -> bool {
        let Mode::Jump { typed } = &mut self.mode else { return false };
        typed.push(character);
        let typed = typed.clone();

        let (exact, remaining) = jump::resolve(&self.tags, &typed);

        if let Some(tag) = exact {
            let cursor = tag.cursor;
            self.buffer.move_to(cursor);
            self.enter_normal();
            return true;
        }

        // A key that matches nothing is a typo, not a command. Drop back to
        // normal rather than leaving a dead overlay on screen.
        if remaining.is_empty() {
            self.enter_normal();
        }
        false
    }

    pub fn enter_search(&mut self) {
        self.mode = Mode::Search { query: String::new() };
    }

    pub fn search_input(&mut self, character: char) {
        if let Mode::Search { query } = &mut self.mode {
            query.push(character);
        }
    }

    pub fn search_backspace(&mut self) {
        if let Mode::Search { query } = &mut self.mode {
            query.pop();
        }
    }

    /// Accepts the search and jumps to the first hit after the cursor.
    pub fn search_accept(&mut self) -> bool {
        let Mode::Search { query } = &self.mode else { return false };
        let query = query.clone();
        self.enter_normal();

        if query.is_empty() {
            return false;
        }
        self.last_search = query;
        self.search_next()
    }

    /// Jumps to the next hit, wrapping. `false` if there is nothing to find.
    pub fn search_next(&mut self) -> bool {
        if self.last_search.is_empty() {
            return false;
        }
        let needle = self.last_search.to_lowercase();
        let count = self.buffer.line_count();
        let start = self.buffer.cursor.line;

        // Start on the line *after* the cursor so repeating the search
        // advances instead of finding the same hit forever.
        for offset in 1..=count {
            let line = (start + offset) % count;
            if let Some(column) = self.buffer.line(line).to_lowercase().find(&needle) {
                let column = self.buffer.line(line)[..column].chars().count();
                self.buffer.move_to(Cursor { line, column });
                self.follow_cursor();
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(text: &str) -> Editor {
        let mut editor = Editor::with_buffer(Buffer::from_str(text));
        editor.set_viewport(10, 80);
        editor
    }

    /// Many short lines, so visual rows and logical lines line up.
    fn many_lines(count: usize) -> Editor {
        let text: String =
            (0..count).map(|index| format!("line{index}\n")).collect::<Vec<_>>().concat();
        editor(&text)
    }

    /// Performance guard against the real vault's largest note.
    ///
    /// `log.md` is 179 KB over 749 lines with a longest line of 1,597
    /// characters. The bounds are generous — this exists to catch a
    /// pathological regression (an accidental O(n) per keystroke), not to
    /// measure precisely, so it must not be flaky on a loaded machine.
    #[test]
    fn a_179_kilobyte_note_stays_responsive() {
        use std::time::Instant;

        // Same shape as the real file: long prose lines, headings throughout.
        let text: String = (0..750)
            .map(|index| {
                if index % 12 == 0 {
                    format!("## Section {index}\n")
                } else {
                    format!("{} entry {index}\n", "some prose text ".repeat(15))
                }
            })
            .collect::<Vec<_>>()
            .concat();
        assert!(text.len() > 150_000, "the fixture should match the real file's scale");

        let mut editor = Editor::with_buffer(Buffer::from_str(&text));
        editor.set_viewport(40, 110);

        let start = Instant::now();
        for _ in 0..200 {
            editor.move_visual(true);
        }
        let movement = start.elapsed();

        let start = Instant::now();
        for _ in 0..100 {
            editor.page(true);
        }
        for _ in 0..100 {
            editor.page(false);
        }
        let paging = start.elapsed();

        let start = Instant::now();
        editor.buffer.move_buffer_end();
        editor.follow_cursor();
        for _ in 0..50 {
            editor.jump_heading(true);
        }
        let headings = start.elapsed();

        let start = Instant::now();
        editor.enter_jump();
        let tagging = start.elapsed();

        // Each of these is hundreds of operations; a keystroke must be far
        // under a frame, so hundreds must be well under a second.
        assert!(movement.as_millis() < 500, "200 cursor moves took {movement:?}");
        assert!(paging.as_millis() < 500, "200 pages took {paging:?}");
        assert!(headings.as_millis() < 500, "50 heading jumps took {headings:?}");
        assert!(tagging.as_millis() < 100, "tagging one screen took {tagging:?}");
    }

    #[test]
    fn undo_history_is_bounded_so_a_long_session_cannot_grow_without_limit() {
        let mut editor = editor("");
        for index in 0..(buffer::UNDO_LIMIT + 200) {
            editor.buffer.insert(&format!("{index} "));
        }
        assert!(
            editor.buffer.undo_depth() <= buffer::UNDO_LIMIT,
            "history grew to {}",
            editor.buffer.undo_depth()
        );

        // And the recent history still works.
        assert!(editor.buffer.undo());
    }

    #[test]
    fn entering_and_leaving_insert_mode_round_trips() {
        let mut editor = editor("x");
        assert_eq!(editor.mode, Mode::Normal);

        editor.enter_insert();
        assert_eq!(editor.mode, Mode::Insert);

        editor.enter_normal();
        assert_eq!(editor.mode, Mode::Normal);
    }

    #[test]
    fn closing_with_unsaved_changes_needs_confirming_without_lying() {
        let mut editor = editor("original");
        editor.buffer.insert("x");
        assert!(editor.buffer.modified);

        assert!(!editor.arm_close(), "the first attempt only arms");
        assert!(editor.buffer.modified, "the unsaved marker must stay honest");

        assert!(editor.arm_close(), "the second confirms");

        editor.disarm_close();
        assert!(!editor.arm_close(), "disarming resets the confirmation");
    }

    #[test]
    fn jump_mode_moves_the_cursor_to_a_tag() {
        let mut editor = editor("alpha beta\ngamma delta");
        editor.enter_jump();

        let target = editor.tags()[3].clone();
        assert_eq!(target.cursor, Cursor { line: 1, column: 6 });

        for character in target.label.chars() {
            editor.jump_input(character);
        }
        assert_eq!(editor.buffer.cursor, target.cursor);
        assert_eq!(editor.mode, Mode::Normal, "a jump ends jump mode");
    }

    #[test]
    fn an_unmatched_jump_key_returns_to_normal_rather_than_hanging() {
        let mut editor = editor("one two");
        editor.enter_jump();

        editor.jump_input('1');
        assert_eq!(editor.mode, Mode::Normal);
        assert!(editor.tags().is_empty(), "the overlay is cleared");
    }

    #[test]
    fn jump_only_tags_what_is_on_screen() {
        let mut editor = many_lines(100);
        editor.anchor = Anchor { line: 40, row: 0 };
        editor.enter_jump();

        assert_eq!(editor.tags().len(), 10, "one per visible line");
        assert_eq!(editor.tags()[0].cursor.line, 40, "tags are absolute, not viewport-relative");
    }

    #[test]
    fn the_viewport_follows_the_cursor_in_both_directions() {
        let mut editor = many_lines(100);

        editor.buffer.move_to(Cursor { line: 50, column: 0 });
        editor.follow_cursor();
        assert!(editor.anchor.line <= 50 && 50 < editor.anchor.line + 10);

        editor.buffer.move_to(Cursor { line: 2, column: 0 });
        editor.follow_cursor();
        assert!(editor.anchor.line <= 2, "scrolling back up works too");
    }

    #[test]
    fn paging_drags_the_cursor_with_it() {
        let mut editor = many_lines(100);

        editor.page(true);
        assert!(
            editor.buffer.cursor.line >= editor.anchor.line,
            "the cursor must not be left off screen"
        );
        assert!(editor.anchor.line > 0, "the view actually moved");
    }

    #[test]
    fn paging_back_from_the_top_stays_at_the_top() {
        let mut editor = many_lines(100);
        editor.page(false);
        assert_eq!(editor.anchor, Anchor { line: 0, row: 0 });
    }

    // --- wrapping ---

    /// 91% of the real vault's notes have a line over 120 characters, so this
    /// is the normal case, not an edge case.
    #[test]
    fn a_long_line_occupies_several_visual_rows() {
        let mut editor = Editor::with_buffer(Buffer::from_str(&"word ".repeat(60)));
        editor.set_viewport(10, 40);

        assert!(editor.line_height(0) > 5, "a 300-character line must wrap");
    }

    #[test]
    fn vertical_movement_walks_visual_rows_not_logical_lines() {
        let mut editor = Editor::with_buffer(Buffer::from_str(&"word ".repeat(40)));
        editor.set_viewport(10, 40);

        // One logical line, several visual rows: moving down must stay on it.
        editor.move_visual(true);
        assert_eq!(editor.buffer.cursor.line, 0, "still the same logical line");
        assert!(editor.buffer.cursor.column > 0, "but further along it");

        editor.move_visual(false);
        assert_eq!(editor.buffer.cursor.column, 0, "and back again");
    }

    #[test]
    fn vertical_movement_crosses_into_the_next_logical_line_at_the_end() {
        let mut editor = editor("short\nalso short");

        editor.move_visual(true);
        assert_eq!(editor.buffer.cursor.line, 1);

        editor.move_visual(false);
        assert_eq!(editor.buffer.cursor.line, 0);
    }

    #[test]
    fn paging_through_one_enormous_line_makes_progress() {
        // The real vault has a 5,139-character line. Paging by logical line
        // would move nowhere at all here.
        let mut editor = Editor::with_buffer(Buffer::from_str(&"word ".repeat(1200)));
        editor.set_viewport(10, 80);

        editor.page(true);
        assert_eq!(editor.anchor.line, 0, "still inside the same line");
        assert!(editor.anchor.row > 0, "but scrolled down within it");

        let after_one = editor.anchor.row;
        editor.page(true);
        assert!(editor.anchor.row > after_one, "paging keeps making progress");
    }

    #[test]
    fn scrolling_never_runs_past_the_end() {
        let mut editor = many_lines(20);
        for _ in 0..50 {
            editor.page(true);
        }
        assert!(editor.anchor.line < editor.buffer.line_count());
    }

    #[test]
    fn a_narrow_viewport_does_not_hang_or_panic() {
        let mut editor = Editor::with_buffer(Buffer::from_str("some reasonably long text here"));
        for width in [0, 1, 2, 5] {
            editor.set_viewport(3, width);
            editor.follow_cursor();
            editor.page(true);
            editor.move_visual(true);
            editor.enter_jump();
            editor.enter_normal();
        }
    }

    // --- search ---

    #[test]
    fn search_finds_the_next_hit_and_wraps() {
        let mut editor = editor("alpha\nbeta\ngamma\nbeta again");

        editor.enter_search();
        for character in "beta".chars() {
            editor.search_input(character);
        }
        assert!(editor.search_accept());
        assert_eq!(editor.buffer.cursor.line, 1);

        assert!(editor.search_next());
        assert_eq!(editor.buffer.cursor.line, 3, "repeating advances");

        assert!(editor.search_next());
        assert_eq!(editor.buffer.cursor.line, 1, "and wraps around");
    }

    #[test]
    fn search_is_case_insensitive_and_reports_misses() {
        let mut editor = editor("Alpha\nBeta");

        editor.enter_search();
        for character in "alpha".chars() {
            editor.search_input(character);
        }
        assert!(editor.search_accept());
        assert_eq!(editor.buffer.cursor.line, 0);

        editor.enter_search();
        for character in "nothing".chars() {
            editor.search_input(character);
        }
        assert!(!editor.search_accept(), "a miss is reported, not silently ignored");
    }

    #[test]
    fn an_empty_search_does_not_clear_the_previous_one() {
        let mut editor = editor("alpha\nbeta");

        editor.enter_search();
        for character in "beta".chars() {
            editor.search_input(character);
        }
        editor.search_accept();

        editor.enter_search();
        assert!(!editor.search_accept(), "an empty query does nothing");
        assert!(editor.search_next(), "the previous search still repeats");
    }

    #[test]
    fn backspace_edits_the_search_query() {
        let mut editor = editor("alpha\nbeta");
        editor.enter_search();

        for character in "betax".chars() {
            editor.search_input(character);
        }
        editor.search_backspace();

        let Mode::Search { query } = &editor.mode else { panic!("still searching") };
        assert_eq!(query, "beta");
    }
}
