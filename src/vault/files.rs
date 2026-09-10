//! Creating, renaming and removing things in the vault.
//!
//! The vault was read-only, which is most of why it stayed a place you read
//! from. If making a note means alt-tabbing to Obsidian, you will keep the
//! habit of writing notes in Obsidian.
//!
//! Every path here is *relative to the vault root* and typed by a person, so
//! every one of them goes through [`resolve`] first. A name is not a path you
//! can trust.

use anyhow::{Context, Result, bail};
use std::path::{Component, Path, PathBuf};

/// Turns a typed name into an absolute path inside the vault.
///
/// **The whole security surface of this module.** `..` in a typed name would
/// otherwise write outside the vault, and an absolute name would ignore the
/// vault entirely — "create a note called `/etc/hosts`" should be refused, not
/// attempted. Rejecting the components is done before touching the disk, so
/// there is no window where a bad path exists.
pub fn resolve(root: &Path, relative: &str) -> Result<PathBuf> {
    let trimmed = relative.trim();

    // Checked *before* the slashes are trimmed. Trimming first turns
    // `/etc/hosts` into `etc/hosts`, which is safe — it stays inside the vault
    // — but silently makes something nobody asked for. Somebody typing an
    // absolute path has misunderstood, and should be told.
    if trimmed.starts_with('/') || Path::new(trimmed).is_absolute() {
        bail!("a name is relative to the vault, so it cannot start at the root");
    }

    let relative = trimmed.trim_matches('/');
    if relative.is_empty() {
        bail!("a name is needed");
    }

    let candidate = Path::new(relative);
    for component in candidate.components() {
        match component {
            Component::Normal(part) => {
                if part.is_empty() {
                    bail!("that name has an empty folder in it");
                }
            }
            Component::CurDir => {}
            Component::ParentDir => bail!("a name cannot climb out of the vault with .."),
            Component::RootDir | Component::Prefix(_) => {
                bail!("a name is relative to the vault, so it cannot start at the root");
            }
        }
    }

    Ok(root.join(candidate))
}

/// Creates an empty note and returns its path.
///
/// Adds `.md` when it is missing, because "a note called meeting-notes" is
/// what somebody means, and creates any folders on the way.
pub fn create_note(root: &Path, relative: &str) -> Result<PathBuf> {
    let with_extension = if Path::new(relative.trim())
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
    {
        relative.trim().to_string()
    } else {
        format!("{}.md", relative.trim().trim_matches('/'))
    };

    let path = resolve(root, &with_extension)?;
    if path.exists() {
        bail!("{} already exists", display_in(root, &path));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not make {}", display_in(root, parent)))?;
    }

    // `create_new` rather than `write`: two people — or an agent and you —
    // making the same note at once should collide, not silently truncate.
    std::fs::File::create_new(&path)
        .with_context(|| format!("could not create {}", display_in(root, &path)))?;
    Ok(path)
}

/// Creates a folder, and any folders above it.
pub fn create_folder(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = resolve(root, relative)?;
    if path.exists() {
        bail!("{} already exists", display_in(root, &path));
    }

    std::fs::create_dir_all(&path)
        .with_context(|| format!("could not make {}", display_in(root, &path)))?;
    Ok(path)
}

/// Renames a note or folder, keeping it inside the vault.
///
/// The new name is interpreted relative to the vault root, so it can move
/// something as well as rename it — typing `archive/old-note` puts it there.
pub fn rename(root: &Path, from: &Path, to: &str) -> Result<PathBuf> {
    let keep_extension = from.is_file()
        && from.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        && !Path::new(to.trim()).extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));

    let to = if keep_extension { format!("{}.md", to.trim().trim_matches('/')) } else { to.into() };

    let destination = resolve(root, &to)?;
    if destination == from {
        return Ok(destination);
    }
    if destination.exists() {
        bail!("{} already exists", display_in(root, &destination));
    }

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not make {}", display_in(root, parent)))?;
    }

    std::fs::rename(from, &destination)
        .with_context(|| format!("could not rename to {}", display_in(root, &destination)))?;
    Ok(destination)
}

/// Removes a note, or a folder and everything under it.
///
/// Callers ask first — see `app::Pending`. This does the deed and nothing else,
/// because a function that sometimes asks and sometimes does not is a function
/// nobody can reason about at the call site.
pub fn remove(path: &Path) -> Result<()> {
    if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }
        .with_context(|| format!("could not remove {}", path.display()))
}

/// How many notes are inside a folder, for a confirmation that says what is
/// at stake rather than asking you to guess.
#[must_use]
pub fn count_within(path: &Path) -> usize {
    walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .count()
}

/// A path as the user thinks of it: relative to the vault.
fn display_in(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("houston-files-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// The one that matters. These names are typed by a person, and a name is
    /// not a path you can trust.
    #[test]
    fn a_name_cannot_climb_out_of_the_vault() {
        let root = scratch("escape");

        for attempt in ["../outside", "../../etc/hosts", "notes/../../escaped", ".."] {
            let refused = resolve(&root, attempt).unwrap_err().to_string();
            assert!(refused.contains(".."), "{attempt:?} should be refused: {refused}");
        }

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_absolute_name_is_refused_rather_than_followed() {
        let root = scratch("absolute");

        let refused = resolve(&root, "/etc/hosts").unwrap_err().to_string();
        assert!(refused.contains("relative"), "got: {refused}");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_empty_name_says_so() {
        let root = scratch("empty");
        assert!(resolve(&root, "   ").is_err());
        assert!(resolve(&root, "/").is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_note_gets_a_markdown_extension_it_did_not_ask_for() {
        let root = scratch("extension");

        let made = create_note(&root, "meeting-notes").unwrap();
        assert_eq!(made.file_name().unwrap(), "meeting-notes.md");

        let kept = create_note(&root, "already.md").unwrap();
        assert_eq!(kept.file_name().unwrap(), "already.md", "and is not doubled up");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_note_in_a_folder_that_does_not_exist_yet_makes_the_folder() {
        let root = scratch("parents");

        let made = create_note(&root, "projects/2026/kickoff").unwrap();

        assert!(made.is_file());
        assert!(root.join("projects/2026").is_dir(), "the folders on the way are made too");

        std::fs::remove_dir_all(&root).ok();
    }

    /// Silently truncating somebody's note would be the worst possible
    /// outcome of a typo.
    #[test]
    fn creating_over_something_that_exists_is_refused() {
        let root = scratch("collide");
        std::fs::write(root.join("taken.md"), "precious content").unwrap();

        let refused = create_note(&root, "taken").unwrap_err().to_string();
        assert!(refused.contains("already exists"), "got: {refused}");
        assert_eq!(
            std::fs::read_to_string(root.join("taken.md")).unwrap(),
            "precious content",
            "and the file is untouched"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn renaming_keeps_the_extension_unless_you_give_one() {
        let root = scratch("rename");
        std::fs::write(root.join("before.md"), "x").unwrap();

        let moved = rename(&root, &root.join("before.md"), "after").unwrap();

        assert_eq!(moved.file_name().unwrap(), "after.md");
        assert!(!root.join("before.md").exists(), "the old name is gone");

        std::fs::remove_dir_all(&root).ok();
    }

    /// Renaming into a folder is how something gets moved, which is worth
    /// having without a separate command for it.
    #[test]
    fn renaming_into_a_folder_moves_it() {
        let root = scratch("move");
        std::fs::write(root.join("stray.md"), "x").unwrap();

        let moved = rename(&root, &root.join("stray.md"), "archive/stray").unwrap();

        assert!(moved.starts_with(root.join("archive")));
        assert!(moved.is_file());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn renaming_onto_something_that_exists_is_refused() {
        let root = scratch("rename-collide");
        std::fs::write(root.join("one.md"), "one").unwrap();
        std::fs::write(root.join("two.md"), "two").unwrap();

        assert!(rename(&root, &root.join("one.md"), "two").is_err());
        assert_eq!(std::fs::read_to_string(root.join("two.md")).unwrap(), "two");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_folder_is_created_empty_and_refuses_to_replace_one() {
        let root = scratch("folder");

        let made = create_folder(&root, "reference/papers").unwrap();
        assert!(made.is_dir());

        assert!(create_folder(&root, "reference/papers").is_err(), "it is already there");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removing_takes_a_note_or_a_whole_folder() {
        let root = scratch("remove");
        std::fs::create_dir_all(root.join("gone")).unwrap();
        std::fs::write(root.join("gone/inside.md"), "x").unwrap();
        std::fs::write(root.join("solo.md"), "x").unwrap();

        remove(&root.join("solo.md")).unwrap();
        assert!(!root.join("solo.md").exists());

        remove(&root.join("gone")).unwrap();
        assert!(!root.join("gone").exists(), "a folder goes with its contents");

        std::fs::remove_dir_all(&root).ok();
    }

    /// So the confirmation can say what is at stake rather than asking you to
    /// remember what is in there.
    #[test]
    fn a_folder_can_say_how_many_notes_it_holds() {
        let root = scratch("count");
        std::fs::create_dir_all(root.join("many/deeper")).unwrap();
        std::fs::write(root.join("many/one.md"), "x").unwrap();
        std::fs::write(root.join("many/deeper/two.md"), "x").unwrap();

        assert_eq!(count_within(&root.join("many")), 2, "counted through subfolders");

        std::fs::remove_dir_all(&root).ok();
    }
}
