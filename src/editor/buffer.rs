//! The text buffer: a rope, a cursor, and an undo history.
//!
//! ADR-0003. A `String` will not do — the largest note in the real vault is
//! 179 KB and every insert would copy it.

use anyhow::{Context, Result};
use ropey::Rope;
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

/// How many undo steps are kept.
///
/// Every keystroke in insert mode is a step. Rope snapshots share structure so
/// each one is cheap, but an unbounded history still grows for as long as the
/// editor is open — and this is a long-lived workspace, not a process you quit
/// after ten minutes.
pub const UNDO_LIMIT: usize = 1000;

/// A position in the buffer, in grapheme-agnostic character terms.
///
/// `column` is a character offset into the line, not a byte offset and not a
/// terminal column. Rendering converts it; editing does not need to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Cursor {
    pub line: usize,
    pub column: usize,
}

/// One reversible step.
///
/// Snapshots the whole rope rather than storing a diff. `Rope` clones are
/// cheap — the structure is shared, so a snapshot costs a handful of pointers,
/// not 179 KB.
#[derive(Debug, Clone)]
struct Snapshot {
    text: Rope,
    cursor: Cursor,
}

pub struct Buffer {
    text: Rope,
    pub cursor: Cursor,
    pub path: Option<PathBuf>,
    /// Whether there are changes not yet written to disk.
    pub modified: bool,
    /// The file's mtime when we last read or wrote it, so an edit made
    /// elsewhere — Obsidian has the same vault open — can be detected before
    /// we overwrite it.
    disk_mtime: Option<SystemTime>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Column the cursor tries to return to when moving vertically, so passing
    /// through a short line does not permanently lose your place.
    goal_column: Option<usize>,
}

impl Buffer {
    #[must_use]
    pub fn from_str(text: &str) -> Self {
        Self {
            text: Rope::from_str(text),
            cursor: Cursor::default(),
            path: None,
            modified: false,
            disk_mtime: None,
            undo: Vec::new(),
            redo: Vec::new(),
            goal_column: None,
        }
    }

    pub fn open(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("could not read {}", path.display()))?;
        let mut buffer = Self::from_str(&text);
        buffer.path = Some(path.to_path_buf());
        buffer.disk_mtime = mtime(path);
        Ok(buffer)
    }

    /// The whole buffer as a string. Used for saving and by tests.
    #[must_use]
    pub fn text(&self) -> String {
        self.text.to_string()
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        // Rope counts a trailing newline as starting an extra empty line,
        // which is true but makes the buffer look one line longer than it
        // reads.
        self.text.len_lines().max(1)
    }

    /// One line without its terminator.
    #[must_use]
    pub fn line(&self, index: usize) -> String {
        if index >= self.text.len_lines() {
            return String::new();
        }
        let line = self.text.line(index);
        line.to_string().trim_end_matches(['\n', '\r']).to_string()
    }

    #[must_use]
    pub fn line_length(&self, index: usize) -> usize {
        self.line(index).chars().count()
    }

    /// Character offset of the cursor from the start of the buffer.
    fn offset_of(&self, cursor: Cursor) -> usize {
        let line = cursor.line.min(self.text.len_lines().saturating_sub(1));
        let start = self.text.line_to_char(line);
        start + cursor.column.min(self.line_length(line))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { text: self.text.clone(), cursor: self.cursor }
    }

    /// Records the current state so the next change can be undone.
    fn checkpoint(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > UNDO_LIMIT {
            // Drop the oldest step. `remove(0)` is O(n) on a Vec, but n is
            // capped and this happens once per keystroke past the limit.
            self.undo.remove(0);
        }
        // Any new edit invalidates the redo branch, the same as every other
        // editor with a linear redo.
        self.redo.clear();
    }

    /// How many undo steps are held. Used by the history-bound test.
    #[cfg(test)]
    #[must_use]
    pub const fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.checkpoint();

        let offset = self.offset_of(self.cursor);
        self.text.insert(offset, text);
        self.modified = true;
        self.goal_column = None;

        // Move the cursor to the end of what was inserted.
        let mut line = self.cursor.line;
        let mut column = self.cursor.column;
        for character in text.chars() {
            if character == '\n' {
                line += 1;
                column = 0;
            } else {
                column += 1;
            }
        }
        self.cursor = Cursor { line, column };
    }

    /// Deletes the character before the cursor. `true` if anything went.
    pub fn delete_backwards(&mut self) -> bool {
        let offset = self.offset_of(self.cursor);
        if offset == 0 {
            return false;
        }
        self.checkpoint();

        // When deleting a line break, the cursor belongs at the *join point* —
        // the previous line's old end. Measuring after the removal would give
        // the length of the merged line and drop the cursor at its far end.
        let join_column = (self.cursor.column == 0 && self.cursor.line > 0)
            .then(|| self.line_length(self.cursor.line - 1));

        self.text.remove(offset - 1..offset);
        self.modified = true;
        self.goal_column = None;

        if let Some(column) = join_column {
            self.cursor = Cursor { line: self.cursor.line - 1, column };
        } else {
            self.cursor.column -= 1;
        }
        true
    }

    /// Deletes the character under the cursor.
    pub fn delete_forwards(&mut self) -> bool {
        let offset = self.offset_of(self.cursor);
        if offset >= self.text.len_chars() {
            return false;
        }
        self.checkpoint();
        self.text.remove(offset..=offset);
        self.modified = true;
        true
    }

    /// Deletes the cursor's line.
    pub fn delete_line(&mut self) {
        if self.text.len_chars() == 0 {
            return;
        }
        self.checkpoint();

        let line = self.cursor.line.min(self.text.len_lines().saturating_sub(1));
        let start = self.text.line_to_char(line);
        let end = if line + 1 < self.text.len_lines() {
            self.text.line_to_char(line + 1)
        } else {
            self.text.len_chars()
        };

        self.text.remove(start..end);
        self.modified = true;
        self.cursor.line = self.cursor.line.min(self.line_count().saturating_sub(1));
        self.clamp_column();
    }

    /// Replaces the cursor's line, keeping the cursor within it.
    pub fn replace_line(&mut self, replacement: &str) {
        self.checkpoint();

        let line = self.cursor.line.min(self.text.len_lines().saturating_sub(1));
        let start = self.text.line_to_char(line);
        let end = start + self.line(line).chars().count();

        self.text.remove(start..end);
        self.text.insert(start, replacement);
        self.modified = true;
        self.clamp_column();
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else { return false };
        self.redo.push(self.snapshot());
        self.text = previous.text;
        self.cursor = previous.cursor;
        self.modified = true;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(self.snapshot());
        self.text = next.text;
        self.cursor = next.cursor;
        self.modified = true;
        true
    }

    // --- movement ---

    pub fn move_to(&mut self, cursor: Cursor) {
        self.cursor = Cursor {
            line: cursor.line.min(self.line_count().saturating_sub(1)),
            column: cursor.column,
        };
        self.clamp_column();
        self.goal_column = None;
    }

    pub fn move_left(&mut self) {
        if self.cursor.column > 0 {
            self.cursor.column -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.column = self.line_length(self.cursor.line);
        }
        self.goal_column = None;
    }

    pub fn move_right(&mut self) {
        if self.cursor.column < self.line_length(self.cursor.line) {
            self.cursor.column += 1;
        } else if self.cursor.line + 1 < self.line_count() {
            self.cursor.line += 1;
            self.cursor.column = 0;
        }
        self.goal_column = None;
    }

    pub const fn move_line_start(&mut self) {
        self.cursor.column = 0;
        self.goal_column = None;
    }

    pub fn move_line_end(&mut self) {
        self.cursor.column = self.line_length(self.cursor.line);
        self.goal_column = None;
    }

    pub fn move_buffer_start(&mut self) {
        self.cursor = Cursor::default();
        self.goal_column = None;
    }

    pub fn move_buffer_end(&mut self) {
        let line = self.line_count().saturating_sub(1);
        self.cursor = Cursor { line, column: self.line_length(line) };
        self.goal_column = None;
    }

    fn clamp_column(&mut self) {
        self.cursor.column = self.cursor.column.min(self.line_length(self.cursor.line));
    }

    // --- saving ---

    /// Whether the file changed on disk since we last read or wrote it.
    #[must_use]
    pub fn changed_on_disk(&self) -> bool {
        let Some(path) = &self.path else { return false };
        match (mtime(path), self.disk_mtime) {
            (Some(current), Some(known)) => current != known,
            // The file was deleted, or we never knew its time. Either way the
            // safe answer is "something changed".
            (None, Some(_)) => true,
            _ => false,
        }
    }

    /// Writes the buffer.
    ///
    /// Refuses if the file changed underneath us unless `force`. The vault is
    /// open in Obsidian at the same time — see `docs/research/vault-profile.md`
    /// — so this is a real hazard, not a theoretical one.
    pub fn save(&mut self, force: bool) -> Result<PathBuf> {
        let path = self.path.clone().context("this buffer has no file to save to")?;

        if !force && self.changed_on_disk() {
            anyhow::bail!(
                "{} changed on disk since you opened it — press S to overwrite",
                path.display()
            );
        }

        write_atomically(&path, &self.text())?;
        self.disk_mtime = mtime(&path);
        self.modified = false;
        Ok(path)
    }
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|meta| meta.modified().ok())
}

/// Writes via a temporary file and a rename.
///
/// A partial write would corrupt a note. `rename` within the same directory is
/// atomic, so a reader either sees the old file or the new one.
fn write_atomically(path: &Path, contents: &str) -> Result<()> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let temporary = directory
        .join(format!(".{}.houston-tmp", path.file_name().unwrap_or_default().to_string_lossy()));

    std::fs::write(&temporary, contents)
        .with_context(|| format!("could not write {}", temporary.display()))?;

    std::fs::rename(&temporary, path)
        .inspect_err(|_| {
            // Do not leave litter next to someone's notes.
            let _ = std::fs::remove_file(&temporary);
        })
        .with_context(|| format!("could not replace {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> Buffer {
        Buffer::from_str(text)
    }

    #[test]
    fn inserting_moves_the_cursor_past_what_was_typed() {
        let mut buffer = buffer("");
        buffer.insert("hello");

        assert_eq!(buffer.text(), "hello");
        assert_eq!(buffer.cursor, Cursor { line: 0, column: 5 });
        assert!(buffer.modified);
    }

    #[test]
    fn inserting_a_newline_starts_a_line() {
        let mut buffer = buffer("ab");
        buffer.move_line_end();
        buffer.insert("\ncd");

        assert_eq!(buffer.text(), "ab\ncd");
        assert_eq!(buffer.cursor, Cursor { line: 1, column: 2 });
    }

    #[test]
    fn backspace_joins_lines_at_a_line_start() {
        let mut buffer = buffer("ab\ncd");
        buffer.move_to(Cursor { line: 1, column: 0 });

        assert!(buffer.delete_backwards());
        assert_eq!(buffer.text(), "abcd");
        assert_eq!(buffer.cursor, Cursor { line: 0, column: 2 });
    }

    #[test]
    fn backspace_at_the_very_start_does_nothing() {
        let mut buffer = buffer("abc");
        assert!(!buffer.delete_backwards());
        assert_eq!(buffer.text(), "abc");
        assert!(!buffer.modified, "a no-op must not mark the buffer dirty");
    }

    #[test]
    fn replacing_a_line_leaves_its_neighbours_alone() {
        let mut buffer = buffer("one\ntwo\nthree");
        buffer.move_to(Cursor { line: 1, column: 3 });

        buffer.replace_line("TWO CHANGED");
        assert_eq!(buffer.text(), "one\nTWO CHANGED\nthree");
        assert_eq!(buffer.cursor.line, 1);
    }

    #[test]
    fn replacing_a_line_with_a_shorter_one_clamps_the_cursor() {
        let mut buffer = buffer("a long line here");
        buffer.move_line_end();

        buffer.replace_line("short");
        assert!(buffer.cursor.column <= 5, "the cursor cannot sit past the end");
    }

    #[test]
    fn undo_and_redo_walk_the_history() {
        let mut buffer = buffer("");
        buffer.insert("one");
        buffer.insert(" two");
        assert_eq!(buffer.text(), "one two");

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "one");
        assert!(buffer.undo());
        assert_eq!(buffer.text(), "");
        assert!(!buffer.undo(), "the history is exhausted");

        assert!(buffer.redo());
        assert_eq!(buffer.text(), "one");
        assert!(buffer.redo());
        assert_eq!(buffer.text(), "one two");
    }

    #[test]
    fn a_new_edit_discards_the_redo_branch() {
        let mut buffer = buffer("");
        buffer.insert("a");
        buffer.undo();
        buffer.insert("b");

        assert!(!buffer.redo(), "redo must not resurrect an abandoned branch");
        assert_eq!(buffer.text(), "b");
    }

    #[test]
    fn horizontal_movement_wraps_between_lines() {
        let mut buffer = buffer("ab\ncd");

        buffer.move_to(Cursor { line: 0, column: 2 });
        buffer.move_right();
        assert_eq!(buffer.cursor, Cursor { line: 1, column: 0 });

        buffer.move_left();
        assert_eq!(buffer.cursor, Cursor { line: 0, column: 2 });
    }

    #[test]
    fn deleting_a_line_keeps_the_cursor_in_the_buffer() {
        let mut buffer = buffer("one\ntwo\nthree");
        buffer.move_to(Cursor { line: 2, column: 4 });

        buffer.delete_line();
        assert_eq!(buffer.text(), "one\ntwo\n");
        assert!(buffer.cursor.line < buffer.line_count());
    }

    #[test]
    fn unicode_is_counted_in_characters_not_bytes() {
        let mut buffer = buffer("héllo wörld");
        buffer.move_line_end();
        assert_eq!(buffer.cursor.column, 11);

        buffer.insert("!");
        assert_eq!(buffer.text(), "héllo wörld!");
    }

    #[test]
    fn saving_writes_atomically_and_clears_the_modified_flag() {
        let path = std::env::temp_dir().join("houston-buffer-save.md");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, "original").unwrap();

        let mut buffer = Buffer::open(&path).unwrap();
        buffer.move_buffer_end();
        buffer.insert(" edited");
        assert!(buffer.modified);

        buffer.save(false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original edited");
        assert!(!buffer.modified);

        // No temporary file left behind next to someone's notes.
        //
        // Checked for *this* file only. An earlier version scanned the whole
        // temp directory, which made the test flaky: tests run concurrently, so
        // another buffer's in-flight temp file would fail this assertion at
        // random.
        let litter = path.with_file_name(format!(
            ".{}.houston-tmp",
            path.file_name().unwrap().to_string_lossy()
        ));
        assert!(!litter.exists(), "left {} behind", litter.display());

        std::fs::remove_file(&path).ok();
    }

    /// The vault is open in Obsidian at the same time. Overwriting someone's
    /// external edit without asking would lose work.
    #[test]
    fn saving_refuses_when_the_file_changed_underneath() {
        let path = std::env::temp_dir().join("houston-buffer-conflict.md");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, "original").unwrap();

        let mut buffer = Buffer::open(&path).unwrap();
        buffer.insert("mine ");

        // Something else writes the file. Sleep-free: set the mtime forward.
        std::fs::write(&path, "theirs").unwrap();
        let future = SystemTime::now() + std::time::Duration::from_secs(120);
        filetime_set(&path, future);

        assert!(buffer.changed_on_disk());
        assert!(buffer.save(false).is_err(), "an external change must block the write");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "theirs", "their work survives");

        buffer.save(true).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("mine"));

        std::fs::remove_file(&path).ok();
    }

    /// Nudges a file's mtime without sleeping.
    fn filetime_set(path: &Path, time: SystemTime) {
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(time).unwrap();
    }
}
