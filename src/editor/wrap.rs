//! Soft-wrapping: turning long logical lines into visual rows.
//!
//! Not optional for this vault. Measured across the real 328 notes, **91% have
//! at least one line over 120 characters**, the median longest line is 360, and
//! one note has a single 5,139-character line. Without wrapping, editing most
//! notes would mean text running off the right edge where you cannot see it.
//!
//! Everything here works in character offsets, matching `Buffer`'s cursor.

/// Where a visual row sits inside a logical line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// Character offset in the logical line where this row starts.
    pub start: usize,
    /// Character offset just past its last character.
    pub end: usize,
}

impl Row {
    pub const fn len(self) -> usize {
        self.end - self.start
    }
}

/// Splits a line into visual rows no wider than `width`.
///
/// Breaks at spaces where it can, since this is prose. A word longer than the
/// whole width — a URL, usually — is broken mid-word rather than overflowing,
/// because an unbreakable row would push the cursor off screen.
///
/// Always returns at least one row, so an empty line still occupies a row and
/// can hold a cursor.
#[must_use]
pub fn rows(line: &str, width: usize) -> Vec<Row> {
    let characters: Vec<char> = line.chars().collect();
    if width == 0 || characters.len() <= width {
        return vec![Row { start: 0, end: characters.len() }];
    }

    let mut rows = Vec::new();
    let mut start = 0;

    while start < characters.len() {
        let remaining = characters.len() - start;
        if remaining <= width {
            rows.push(Row { start, end: characters.len() });
            break;
        }

        // Look back from the width limit for a space to break on. The limit is
        // inclusive: a space exactly at the boundary is the ideal break.
        let hard_end = start + width;
        let break_at = (start..=hard_end)
            .rev()
            .find(|index| characters.get(*index).is_some_and(|c| *c == ' '))
            .filter(|index| *index > start);

        let end = break_at.unwrap_or(hard_end);
        rows.push(Row { start, end });

        // Swallow the run of spaces the break landed on; leading spaces on the
        // next row would look like accidental indentation.
        start = end;
        while characters.get(start).is_some_and(|c| *c == ' ') {
            start += 1;
        }
    }

    if rows.is_empty() {
        rows.push(Row { start: 0, end: characters.len() });
    }
    rows
}

/// How many visual rows a line occupies.
#[must_use]
pub fn height(line: &str, width: usize) -> usize {
    rows(line, width).len()
}

/// Which visual row a character offset falls on, and its column within it.
#[must_use]
pub fn locate(line: &str, width: usize, column: usize) -> (usize, usize) {
    let rows = rows(line, width);

    for (index, row) in rows.iter().enumerate() {
        // The last row owns the position one past its end, which is where the
        // cursor sits after the final character.
        let last = index + 1 == rows.len();
        if column < row.end || (last && column <= row.end) {
            return (index, column.saturating_sub(row.start));
        }
        // A position inside the whitespace a break swallowed belongs to the
        // end of this row, not the start of the next.
        if let Some(next) = rows.get(index + 1)
            && column < next.start
        {
            return (index, row.len());
        }
    }

    let last = rows.len().saturating_sub(1);
    (last, rows[last].len())
}

/// The character offset for a visual row and column within it.
#[must_use]
pub fn offset(line: &str, width: usize, row: usize, column: usize) -> usize {
    let rows = rows(line, width);
    let Some(target) = rows.get(row.min(rows.len().saturating_sub(1))) else { return 0 };
    (target.start + column).min(target.end)
}

/// The text of one visual row.
///
/// The renderer slices the line itself to avoid an allocation per row, so this
/// exists for tests that need to check what wrapping produced.
#[cfg(test)]
#[must_use]
pub fn row_text(line: &str, row: Row) -> String {
    line.chars().skip(row.start).take(row.len()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(line: &str, width: usize) -> Vec<String> {
        rows(line, width).into_iter().map(|row| row_text(line, row)).collect()
    }

    #[test]
    fn a_short_line_is_one_row() {
        assert_eq!(rows("hello", 20), vec![Row { start: 0, end: 5 }]);
        assert_eq!(height("hello", 20), 1);
    }

    #[test]
    fn an_empty_line_still_occupies_a_row() {
        // Otherwise a blank line could not hold the cursor.
        assert_eq!(rows("", 20).len(), 1);
        assert_eq!(height("", 20), 1);
    }

    #[test]
    fn wrapping_breaks_on_spaces_not_mid_word() {
        assert_eq!(texts("the quick brown fox", 10), vec!["the quick", "brown fox"]);
    }

    #[test]
    fn a_word_longer_than_the_width_is_broken_rather_than_overflowing() {
        // A long URL must not push the cursor off screen.
        let long = "a".repeat(25);
        let wrapped = texts(&long, 10);
        assert_eq!(wrapped.len(), 3);
        assert!(wrapped.iter().all(|row| row.chars().count() <= 10));
    }

    #[test]
    fn no_row_ever_exceeds_the_width() {
        let prose = "The quick brown fox jumps over the lazy dog and keeps on running \
                     for a very long time indeed without stopping at all";
        for width in [8, 12, 20, 37, 80] {
            for row in texts(prose, width) {
                assert!(row.chars().count() <= width, "width {width} produced {row:?}");
            }
        }
    }

    #[test]
    fn wrapping_loses_no_characters_other_than_the_spaces_it_breaks_on() {
        let prose = "alpha beta gamma delta epsilon zeta eta theta";
        let rejoined = texts(prose, 12).join(" ");
        assert_eq!(rejoined, prose, "text must survive a round trip");
    }

    #[test]
    fn a_five_thousand_character_line_wraps_without_trouble() {
        // The real vault has one of these.
        let monster = "word ".repeat(1000);
        let wrapped = rows(&monster, 100);
        assert!(wrapped.len() > 40);
        assert!(wrapped.iter().all(|row| row.len() <= 100));
    }

    #[test]
    fn locate_maps_a_column_onto_its_visual_row() {
        let line = "the quick brown fox";
        // "the quick" / "brown fox"
        assert_eq!(locate(line, 10, 0), (0, 0));
        assert_eq!(locate(line, 10, 4), (0, 4));
        assert_eq!(locate(line, 10, 10), (1, 0), "first character of the second row");
        assert_eq!(locate(line, 10, 19), (1, 9), "the very end of the line");
    }

    #[test]
    fn locate_and_offset_round_trip() {
        let line = "alpha beta gamma delta epsilon";
        for column in 0..=line.chars().count() {
            let (row, within) = locate(line, 12, column);
            let back = offset(line, 12, row, within);
            assert_eq!(back, column, "column {column} did not round trip");
        }
    }

    #[test]
    fn a_zero_width_viewport_does_not_divide_by_zero() {
        assert_eq!(rows("anything", 0).len(), 1);
        assert_eq!(locate("anything", 0, 3), (0, 3));
    }
}
