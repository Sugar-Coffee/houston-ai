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

use anyhow::Result;
use buffer::{Buffer, Cursor};
use jump::Tag;
use std::path::Path;

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
    /// First buffer line shown, kept in step with the cursor.
    pub scroll: usize,
    /// Live jump targets, rebuilt each time jump mode is entered.
    tags: Vec<Tag>,
    /// The last search, so `n` can repeat it.
    last_search: String,
    /// Rows the viewport can show, set from the render loop each frame.
    viewport: usize,
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
            scroll: 0,
            tags: Vec::new(),
            last_search: String::new(),
            viewport: 24,
            close_armed: false,
        }
    }

    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    /// Tells the editor how tall its viewport is. Called from the renderer,
    /// because only it knows.
    pub const fn set_viewport(&mut self, rows: usize) {
        self.viewport = if rows == 0 { 1 } else { rows };
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
    pub const fn follow_cursor(&mut self) {
        let line = self.buffer.cursor.line;

        if line < self.scroll {
            self.scroll = line;
        } else if line >= self.scroll + self.viewport {
            self.scroll = line + 1 - self.viewport;
        }
    }

    pub fn scroll_by(&mut self, delta: isize) {
        let target = self.scroll.saturating_add_signed(delta);
        self.scroll = target.min(self.buffer.line_count().saturating_sub(1));

        // Drag the cursor along rather than leaving it off screen, which is
        // what makes scrolled-then-typed text land somewhere surprising.
        let cursor = self.buffer.cursor.line;
        if cursor < self.scroll {
            self.buffer.move_to(Cursor { line: self.scroll, column: 0 });
        } else if cursor >= self.scroll + self.viewport {
            self.buffer.move_to(Cursor { line: self.scroll + self.viewport - 1, column: 0 });
        }
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
    pub fn enter_jump(&mut self) {
        let last = (self.scroll + self.viewport).min(self.buffer.line_count());
        let lines: Vec<String> = (self.scroll..last).map(|index| self.buffer.line(index)).collect();

        self.tags = jump::tags(&lines, self.scroll);
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
        editor.set_viewport(10);
        editor
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
    fn entering_and_leaving_insert_mode_round_trips() {
        let mut editor = editor("x");
        assert_eq!(editor.mode, Mode::Normal);

        editor.enter_insert();
        assert_eq!(editor.mode, Mode::Insert);

        editor.enter_normal();
        assert_eq!(editor.mode, Mode::Normal);
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

        // '1' cannot start any label.
        editor.jump_input('1');
        assert_eq!(editor.mode, Mode::Normal);
        assert!(editor.tags().is_empty(), "the overlay is cleared");
    }

    #[test]
    fn jump_only_tags_what_is_on_screen() {
        let text: String =
            (0..100).map(|index| format!("line{index}\n")).collect::<Vec<_>>().concat();
        let mut editor = editor(&text);
        editor.scroll = 40;
        editor.enter_jump();

        assert_eq!(editor.tags().len(), 10, "one per visible line");
        assert_eq!(editor.tags()[0].cursor.line, 40, "tags are absolute, not viewport-relative");
    }

    #[test]
    fn the_viewport_follows_the_cursor_in_both_directions() {
        let text: String =
            (0..100).map(|index| format!("line{index}\n")).collect::<Vec<_>>().concat();
        let mut editor = editor(&text);

        editor.buffer.move_to(Cursor { line: 50, column: 0 });
        editor.follow_cursor();
        assert!(editor.scroll <= 50 && 50 < editor.scroll + 10, "cursor is on screen");

        editor.buffer.move_to(Cursor { line: 2, column: 0 });
        editor.follow_cursor();
        assert!(editor.scroll <= 2, "scrolling back up works too");
    }

    #[test]
    fn scrolling_drags_the_cursor_with_it() {
        let text: String =
            (0..100).map(|index| format!("line{index}\n")).collect::<Vec<_>>().concat();
        let mut editor = editor(&text);

        editor.scroll_by(50);
        assert!(
            editor.buffer.cursor.line >= editor.scroll,
            "the cursor must not be left off screen"
        );
    }

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
