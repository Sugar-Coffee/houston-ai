//! Jump mode: amp's signature move.
//!
//! Press the jump key and every word start on screen gets a two-character tag.
//! Type the tag and the cursor teleports there. It replaces a dozen motions
//! with one, and it is the single idea most worth taking from amp (ADR-0003).

use super::buffer::Cursor;

/// Letters used to build tags.
///
/// Home-row-first so the common targets — the ones nearest the top of the
/// screen — are the easiest to type. `q` is excluded because it is the key
/// people press when they want out.
const ALPHABET: [char; 24] = [
    'a', 's', 'd', 'f', 'j', 'k', 'l', 'g', 'h', 'w', 'e', 'r', 'u', 'i', 'o', 'p', 'z', 'x', 'c',
    'v', 'b', 'n', 'm', 't',
];

/// A jump target: a tag and where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub label: String,
    pub cursor: Cursor,
}

/// Builds tags for every word start in the given lines.
///
/// `first_line` is the buffer line the viewport starts at, so tags carry
/// absolute positions and the caller does not have to translate them back.
#[must_use]
pub fn tags(lines: &[String], first_line: usize) -> Vec<Tag> {
    let positions: Vec<Cursor> = lines
        .iter()
        .enumerate()
        .flat_map(|(offset, line)| {
            word_starts(line)
                .into_iter()
                .map(move |column| Cursor { line: first_line + offset, column })
        })
        .collect();

    let width = label_width(positions.len());
    positions
        .into_iter()
        .enumerate()
        .map(|(index, cursor)| Tag { label: label_for(index, width), cursor })
        .collect()
}

/// Character offsets where a word begins.
fn word_starts(line: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut previous_was_separator = true;

    for (column, character) in line.chars().enumerate() {
        let separator = !character.is_alphanumeric() && character != '_';
        if !separator && previous_was_separator {
            starts.push(column);
        }
        previous_was_separator = separator;
    }
    starts
}

/// How many characters each label needs to stay unique.
///
/// One character while they fit, because a single keystroke is the whole point.
const fn label_width(count: usize) -> usize {
    if count <= ALPHABET.len() { 1 } else { 2 }
}

fn label_for(index: usize, width: usize) -> String {
    if width == 1 {
        return ALPHABET[index % ALPHABET.len()].to_string();
    }
    let first = ALPHABET[(index / ALPHABET.len()) % ALPHABET.len()];
    let second = ALPHABET[index % ALPHABET.len()];
    format!("{first}{second}")
}

/// Narrows a tag set by what has been typed so far.
///
/// Returns the exact match if the input identifies one, alongside the tags
/// still in play so the overlay can show what is left.
#[must_use]
pub fn resolve<'a>(tags: &'a [Tag], typed: &str) -> (Option<&'a Tag>, Vec<&'a Tag>) {
    let matching: Vec<&Tag> = tags.iter().filter(|tag| tag.label.starts_with(typed)).collect();

    let exact = matching.iter().copied().find(|tag| tag.label == typed);
    (exact, matching)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_starts_ignore_punctuation_and_runs_of_space() {
        assert_eq!(word_starts("hello world"), vec![0, 6]);
        assert_eq!(word_starts("  spaced   out  "), vec![2, 11]);
        assert_eq!(word_starts("- [ ] a task"), vec![6, 8]);
        assert!(word_starts("   ").is_empty());
        assert!(word_starts("").is_empty());
    }

    #[test]
    fn snake_case_is_one_word() {
        assert_eq!(word_starts("some_long_name here"), vec![0, 15]);
    }

    #[test]
    fn a_short_screen_gets_single_character_tags() {
        let lines = vec!["one two three".to_string()];
        let tags = tags(&lines, 0);

        assert_eq!(tags.len(), 3);
        assert!(tags.iter().all(|tag| tag.label.chars().count() == 1), "one keystroke each");
    }

    #[test]
    fn tags_are_unique_across_a_full_screen() {
        // More targets than the alphabet, so labels must grow to two.
        let lines: Vec<String> = (0..40).map(|index| format!("word{index} another")).collect();
        let tags = tags(&lines, 0);

        assert_eq!(tags.len(), 80);
        assert!(tags.iter().all(|tag| tag.label.chars().count() == 2));

        let mut labels: Vec<&str> = tags.iter().map(|tag| tag.label.as_str()).collect();
        labels.sort_unstable();
        let count = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), count, "every label must be unique");
    }

    #[test]
    fn tags_carry_absolute_buffer_positions() {
        let lines = vec!["alpha".to_string(), "beta".to_string()];
        let tags = tags(&lines, 100);

        assert_eq!(tags[0].cursor, Cursor { line: 100, column: 0 });
        assert_eq!(tags[1].cursor, Cursor { line: 101, column: 0 });
    }

    #[test]
    fn resolving_narrows_then_matches() {
        let lines: Vec<String> = (0..40).map(|index| format!("word{index} another")).collect();
        let tags = tags(&lines, 0);
        let target = tags[30].clone();

        let prefix = &target.label[..1];
        let (exact, remaining) = resolve(&tags, prefix);
        assert!(exact.is_none(), "a prefix alone should not jump");
        assert!(remaining.len() > 1);

        let (exact, remaining) = resolve(&tags, &target.label);
        assert_eq!(exact, Some(&target));
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn nonsense_input_matches_nothing() {
        let lines = vec!["one two".to_string()];
        let tags = tags(&lines, 0);

        let (exact, remaining) = resolve(&tags, "zzz");
        assert!(exact.is_none());
        assert!(remaining.is_empty(), "the overlay should clear rather than mislead");
    }

    #[test]
    fn an_empty_screen_produces_no_tags() {
        assert!(tags(&[], 0).is_empty());
        assert!(tags(&[String::new(), "   ".to_string()], 0).is_empty());
    }
}
