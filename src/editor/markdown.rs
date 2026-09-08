//! Markdown-aware editing helpers.
//!
//! Houston edits prose, not code, so the useful conveniences are different from
//! a code editor's. Chosen by counting the real vault
//! (`docs/research/vault-profile.md`), not by guessing:
//!
//! | construct | notes containing it |
//! |---|---:|
//! | bullet lists | 308 of 328 |
//! | headings | 325 |
//! | ordered lists | 158 |
//! | wikilinks | 157 |
//! | task items | 24 |
//!
//! Syntax highlighting is deliberately absent. It is a code-editor feature; in
//! prose it decorates without helping you write.

/// The list marker a line begins with, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    /// Leading whitespace, so continuation keeps the nesting level.
    pub indent: String,
    /// The bullet or number, without its trailing space.
    pub bullet: Bullet,
    /// A task checkbox, if the item has one.
    pub task: bool,
    /// Character offset where the item's content begins.
    pub content_at: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bullet {
    /// `-`, `*` or `+`.
    Unordered(char),
    /// `1.` and friends. Carries the number so the next item can increment.
    Ordered(u64),
}

impl Marker {
    /// The prefix a continuation line should start with.
    ///
    /// Ordered lists increment; task items continue as unticked, because a new
    /// item is by definition not done yet.
    #[must_use]
    pub fn continuation(&self) -> String {
        let bullet = match &self.bullet {
            Bullet::Unordered(character) => character.to_string(),
            Bullet::Ordered(number) => format!("{}.", number + 1),
        };
        let task = if self.task { " [ ]" } else { "" };
        format!("{}{bullet}{task} ", self.indent)
    }
}

/// Reads the list marker at the start of a line.
#[must_use]
pub fn marker(line: &str) -> Option<Marker> {
    let characters: Vec<char> = line.chars().collect();
    let indent_len = characters.iter().take_while(|c| **c == ' ' || **c == '\t').count();
    let indent: String = characters[..indent_len].iter().collect();
    let rest: String = characters[indent_len..].iter().collect();

    let (bullet, after_bullet) = if let Some(first) = rest.chars().next()
        && matches!(first, '-' | '*' | '+')
    {
        (Bullet::Unordered(first), 1)
    } else {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 || !rest[digits..].starts_with('.') {
            return None;
        }
        let number = rest[..digits].parse::<u64>().ok()?;
        (Bullet::Ordered(number), digits + 1)
    };

    // A marker must be followed by a space, or `-not-a-list` would count.
    let tail: String = rest.chars().skip(after_bullet).collect();
    if !tail.starts_with(' ') {
        return None;
    }

    let trimmed = tail.trim_start();
    let spaces = tail.chars().count() - trimmed.chars().count();
    let task =
        trimmed.starts_with("[ ]") || trimmed.starts_with("[x]") || trimmed.starts_with("[X]");
    let checkbox = usize::from(task) * 3;

    // Skip the space that follows a checkbox too.
    let after_checkbox: String = trimmed.chars().skip(checkbox).collect();
    let extra = usize::from(task && after_checkbox.starts_with(' '));

    Some(Marker {
        indent,
        bullet,
        task,
        content_at: indent_len + after_bullet + spaces + checkbox + extra,
    })
}

/// Whether a list item has nothing after its marker.
///
/// Pressing Return on one of these ends the list rather than making another
/// empty item, which is what every markdown editor does and what fingers
/// expect.
#[must_use]
pub fn is_empty_item(line: &str) -> bool {
    marker(line).is_some_and(|marker| line.chars().skip(marker.content_at).all(char::is_whitespace))
}

/// The heading level of a line, if it is one.
#[must_use]
pub fn heading_level(line: &str) -> Option<usize> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    // `#tag` is not a heading; `# Heading` is.
    line.chars().nth(hashes).is_some_and(char::is_whitespace).then_some(hashes)
}

/// Toggles a task checkbox on a line, returning the new line.
#[must_use]
pub fn toggle_task(line: &str) -> Option<String> {
    let marker = marker(line)?;
    if !marker.task {
        return None;
    }

    let characters: Vec<char> = line.chars().collect();
    let box_start = characters.iter().enumerate().position(|(_, c)| *c == '[')?;
    let state = *characters.get(box_start + 1)?;

    let replacement = if state == ' ' { 'x' } else { ' ' };
    let mut updated = characters;
    updated[box_start + 1] = replacement;
    Some(updated.into_iter().collect())
}

/// The `[[wikilink]]` target under a character offset, if any.
///
/// Links are the vault's navigation model — 157 of 328 notes use them — so
/// following one from the editor should not mean closing it first.
#[must_use]
pub fn link_at(line: &str, column: usize) -> Option<String> {
    let characters: Vec<char> = line.chars().collect();
    let mut search = 0;

    while let Some(open) = find_from(&characters, search, ['[', '[']) {
        let Some(close) = find_from(&characters, open + 2, [']', ']']) else { break };

        // Inclusive of the brackets, so the cursor sitting on `[[` counts.
        if column >= open && column <= close + 1 {
            let target: String = characters[open + 2..close].iter().collect();
            // Obsidian's `[[target|alias]]` and `[[target#heading]]`.
            let target = target.split(['|', '#']).next().unwrap_or(&target).trim();
            return (!target.is_empty()).then(|| target.to_string());
        }
        search = close + 2;
    }
    None
}

fn find_from(characters: &[char], start: usize, pattern: [char; 2]) -> Option<usize> {
    (start..characters.len().saturating_sub(1))
        .find(|index| characters[*index] == pattern[0] && characters[index + 1] == pattern[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unordered_markers_are_recognised_with_any_bullet() {
        for bullet in ['-', '*', '+'] {
            let line = format!("{bullet} an item");
            let marker = marker(&line).expect("should parse");
            assert_eq!(marker.bullet, Bullet::Unordered(bullet));
            assert_eq!(marker.continuation(), format!("{bullet} "));
        }
    }

    #[test]
    fn ordered_markers_increment_on_continuation() {
        let marker = marker("3. third thing").unwrap();
        assert_eq!(marker.bullet, Bullet::Ordered(3));
        assert_eq!(marker.continuation(), "4. ");
    }

    #[test]
    fn indentation_is_preserved_so_nesting_survives() {
        let marker = marker("    - nested").unwrap();
        assert_eq!(marker.indent, "    ");
        assert_eq!(marker.continuation(), "    - ");
    }

    #[test]
    fn task_items_continue_unticked() {
        let done = marker("- [x] finished").unwrap();
        assert!(done.task);
        assert_eq!(done.continuation(), "- [ ] ", "a new item is not done yet");
    }

    #[test]
    fn things_that_merely_start_with_a_dash_are_not_lists() {
        assert!(marker("-not-a-list").is_none());
        assert!(marker("plain prose").is_none());
        assert!(marker("").is_none());
        assert!(marker("3.no space").is_none());
    }

    #[test]
    fn an_empty_item_is_detected_so_return_can_end_the_list() {
        assert!(is_empty_item("- "));
        assert!(is_empty_item("  1. "));
        assert!(is_empty_item("- [ ] "));
        assert!(!is_empty_item("- something"));
        assert!(!is_empty_item("not a list at all"));
    }

    #[test]
    fn content_offset_points_past_the_whole_marker() {
        let marker = marker("- [ ] the task").unwrap();
        assert_eq!(&"- [ ] the task"[marker.content_at..], "the task");

        let plain = marker2("  - item");
        assert_eq!(&"  - item"[plain.content_at..], "item");
    }

    fn marker2(line: &str) -> Marker {
        marker(line).unwrap()
    }

    #[test]
    fn headings_need_a_space_to_distinguish_them_from_tags() {
        assert_eq!(heading_level("# Title"), Some(1));
        assert_eq!(heading_level("### Third"), Some(3));
        assert_eq!(heading_level("###### Sixth"), Some(6));

        assert_eq!(heading_level("#tag"), None, "a tag is not a heading");
        assert_eq!(heading_level("####### too many"), None);
        assert_eq!(heading_level("not a heading"), None);
    }

    #[test]
    fn toggling_a_task_flips_it_both_ways() {
        assert_eq!(toggle_task("- [ ] a task").as_deref(), Some("- [x] a task"));
        assert_eq!(toggle_task("- [x] a task").as_deref(), Some("- [ ] a task"));
        assert_eq!(toggle_task("  - [ ] nested").as_deref(), Some("  - [x] nested"));

        assert!(toggle_task("- an ordinary item").is_none());
        assert!(toggle_task("prose").is_none());
    }

    #[test]
    fn a_wikilink_is_found_anywhere_within_its_brackets() {
        let line = "see [[Deep Work]] for more";
        for column in 4..=16 {
            assert_eq!(link_at(line, column).as_deref(), Some("Deep Work"), "at column {column}");
        }
        assert!(link_at(line, 0).is_none(), "before the link");
        assert!(link_at(line, 20).is_none(), "after it");
    }

    #[test]
    fn a_wikilinks_alias_and_heading_are_stripped_from_the_target() {
        assert_eq!(link_at("[[note|shown text]]", 3).as_deref(), Some("note"));
        assert_eq!(link_at("[[note#a heading]]", 3).as_deref(), Some("note"));
    }

    #[test]
    fn the_right_link_is_found_when_a_line_has_several() {
        let line = "[[first]] and [[second]]";
        assert_eq!(link_at(line, 3).as_deref(), Some("first"));
        assert_eq!(link_at(line, 18).as_deref(), Some("second"));
        assert!(link_at(line, 11).is_none(), "the gap between them is not a link");
    }

    #[test]
    fn malformed_links_do_not_panic_or_match() {
        assert!(link_at("[[unclosed", 3).is_none());
        assert!(link_at("[[]]", 2).is_none(), "an empty target is not a link");
        assert!(link_at("", 0).is_none());
    }
}
