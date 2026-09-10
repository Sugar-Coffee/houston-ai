//! The vault as a folder tree.
//!
//! The flat list this replaces was the right shape for finding a note by name
//! and the wrong shape for everything else. You cannot see how the vault is
//! organised, you cannot tell an empty folder exists, and there is nowhere
//! obvious to put a new note — which is most of why the vault stayed a place
//! you read from rather than a place you work.
//!
//! Fuzzy find and full-text search still show a flat list, because a tree is a
//! worse answer to "where is the note called X". This is the browsing view
//! only.

use crate::vault::{NoteId, Vault};
use std::collections::BTreeSet;

/// One line of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Path relative to the vault root. The identity of the row.
    pub relative: String,
    /// What to draw: the last component.
    pub name: String,
    /// How far in, in folders.
    pub depth: usize,
    /// `None` for a folder.
    pub note: Option<NoteId>,
    /// Folders only: whether this one is open.
    pub expanded: bool,
}

impl Row {
    /// Test-only: the browser converts rows into `Entry`, which carries the
    /// distinction in its own shape.
    #[cfg(test)]
    #[must_use]
    pub const fn is_folder(&self) -> bool {
        self.note.is_none()
    }
}

/// Which folders are open.
///
/// Only the open set is stored. A vault with a thousand notes has a few dozen
/// folders and you will have opened three of them, so remembering the
/// exceptions is smaller and — more usefully — means a folder created while
/// Houston is running starts closed like every other one.
#[derive(Debug, Default, Clone)]
pub struct Tree {
    expanded: BTreeSet<String>,
}

impl Tree {
    /// Opens or closes a folder.
    pub fn toggle(&mut self, relative: &str) {
        if !self.expanded.remove(relative) {
            self.expanded.insert(relative.to_string());
        }
    }

    /// Opens a folder and every folder above it, so a path can be revealed.
    pub fn reveal(&mut self, relative: &str) {
        let mut prefix = String::new();
        for part in relative.split('/').filter(|part| !part.is_empty()) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            self.expanded.insert(prefix.clone());
        }
    }

    pub fn collapse(&mut self, relative: &str) {
        self.expanded.remove(relative);
    }

    #[must_use]
    pub fn is_expanded(&self, relative: &str) -> bool {
        self.expanded.contains(relative)
    }

    /// The rows to draw, in order.
    ///
    /// Folders before files at every level, each alphabetically — the ordering
    /// every file browser uses, and the one that makes a folder you are
    /// looking for findable by eye.
    #[must_use]
    pub fn rows(&self, vault: &Vault) -> Vec<Row> {
        // Every folder that exists, including ones implied by a note's path
        // but not walked (which cannot happen today, and would show up as a
        // missing branch if it ever did).
        let mut folders: BTreeSet<&str> = vault.folders().iter().map(String::as_str).collect();
        for note in vault.notes() {
            let mut path = note.folder();
            while !path.is_empty() {
                folders.insert(path);
                path = path.rsplit_once('/').map_or("", |(parent, _)| parent);
            }
        }

        let mut rows = Vec::new();
        self.walk("", &folders, vault, 0, &mut rows);
        rows
    }

    /// Emits one folder's contents, recursing into the open ones.
    fn walk(
        &self,
        parent: &str,
        folders: &BTreeSet<&str>,
        vault: &Vault,
        depth: usize,
        rows: &mut Vec<Row>,
    ) {
        let children: Vec<&str> =
            folders.iter().copied().filter(|folder| parent_of(folder) == parent).collect();

        for folder in children {
            let expanded = self.is_expanded(folder);
            rows.push(Row {
                relative: folder.to_string(),
                name: last_part(folder).to_string(),
                depth,
                note: None,
                expanded,
            });

            if expanded {
                self.walk(folder, folders, vault, depth + 1, rows);
            }
        }

        for (index, note) in vault.notes().iter().enumerate() {
            if note.folder() == parent {
                rows.push(Row {
                    relative: note.relative.clone(),
                    name: note.stem.clone(),
                    depth,
                    note: Some(NoteId(index)),
                    expanded: false,
                });
            }
        }
    }
}

/// The folder containing `relative`, or `""` at the top.
fn parent_of(relative: &str) -> &str {
    relative.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// The last component of a path.
fn last_part(relative: &str) -> &str {
    relative.rsplit_once('/').map_or(relative, |(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vault on disk, because `Vault::open` walks a real directory and the
    /// tree's whole job is reflecting what is there.
    fn scratch_vault(name: &str, entries: &[&str]) -> (std::path::PathBuf, Vault) {
        let root = std::env::temp_dir().join(format!("houston-tree-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        for entry in entries {
            let path = root.join(entry);
            if entry.ends_with('/') {
                std::fs::create_dir_all(&path).unwrap();
            } else {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&path, "# note\n").unwrap();
            }
        }

        let vault = Vault::open(&root).unwrap();
        (root, vault)
    }

    fn names(rows: &[Row]) -> Vec<String> {
        rows.iter().map(|row| format!("{}{}", "  ".repeat(row.depth), row.name)).collect()
    }

    #[test]
    fn a_closed_tree_shows_only_the_top_level() {
        let (root, vault) = scratch_vault("closed", &["projects/alpha.md", "readme.md"]);
        let tree = Tree::default();

        assert_eq!(
            names(&tree.rows(&vault)),
            vec!["projects", "readme"],
            "folders before files, and nothing inside a closed folder"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn opening_a_folder_shows_what_is_in_it() {
        let (root, vault) = scratch_vault("open", &["projects/alpha.md", "readme.md"]);
        let mut tree = Tree::default();
        tree.toggle("projects");

        assert_eq!(names(&tree.rows(&vault)), vec!["projects", "  alpha", "readme"]);

        std::fs::remove_dir_all(&root).ok();
    }

    /// The reason folders are indexed separately from notes: a folder you have
    /// just created has nothing in it, and a tree built from note paths alone
    /// would not show it — which reads as "new folder" having failed.
    #[test]
    fn an_empty_folder_still_appears() {
        let (root, vault) = scratch_vault("empty", &["notes/", "readme.md"]);
        let tree = Tree::default();

        assert!(
            names(&tree.rows(&vault)).contains(&"notes".to_string()),
            "an empty folder is still a folder"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn folders_come_before_files_at_every_level() {
        let (root, vault) =
            scratch_vault("order", &["b-folder/inner.md", "a-note.md", "z-note.md"]);
        let mut tree = Tree::default();
        tree.toggle("b-folder");

        assert_eq!(
            names(&tree.rows(&vault)),
            vec!["b-folder", "  inner", "a-note", "z-note"],
            "the folder leads even though its name sorts after a-note"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn nesting_goes_as_deep_as_it_is_opened() {
        let (root, vault) = scratch_vault("deep", &["a/b/c/deep.md"]);
        let mut tree = Tree::default();

        assert_eq!(names(&tree.rows(&vault)), vec!["a"], "closed at the top");

        tree.reveal("a/b/c");
        assert_eq!(
            names(&tree.rows(&vault)),
            vec!["a", "  b", "    c", "      deep"],
            "revealing a path opens every folder above it"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn toggling_opens_then_closes() {
        let mut tree = Tree::default();

        assert!(!tree.is_expanded("notes"));
        tree.toggle("notes");
        assert!(tree.is_expanded("notes"));
        tree.toggle("notes");
        assert!(!tree.is_expanded("notes"), "the same key closes it again");
    }

    #[test]
    fn a_row_knows_whether_it_is_a_folder() {
        let (root, vault) = scratch_vault("kinds", &["projects/alpha.md"]);
        let rows = Tree::default().rows(&vault);

        assert!(rows[0].is_folder(), "projects is a folder");
        assert!(rows[0].note.is_none());

        std::fs::remove_dir_all(&root).ok();
    }
}
