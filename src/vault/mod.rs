//! The vault: a folder of markdown notes, indexed for fast navigation.
//!
//! ADR-0004 — this exists to feed agent sessions, not to be a document reader.
//! The index is therefore built around *finding* a note (fuzzy open, wikilink
//! resolution, backlinks) rather than around rendering one.

pub mod browser;
pub mod markdown;
pub mod search;

pub use browser::Browser;
pub use search::Matcher;

use anyhow::{Context, Result};
use std::{collections::HashMap, path::PathBuf};
use walkdir::WalkDir;

/// Directories that are never notes, however deep they are nested.
const SKIP_DIRECTORIES: [&str; 6] =
    [".git", ".obsidian", ".trash", "node_modules", ".playwright-mcp", ".claude"];

/// Index position of a note. Stable for the lifetime of one index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoteId(pub usize);

#[derive(Debug, Clone)]
pub struct Note {
    /// Absolute path on disk.
    pub path: PathBuf,
    /// Path relative to the vault root, used for display and matching.
    pub relative: String,
    /// File stem — what a `[[wikilink]]` refers to.
    pub stem: String,
    #[expect(dead_code, reason = "Phase 5 uses this for 'touched since the session started'")]
    pub modified: Option<std::time::SystemTime>,
}

impl Note {
    /// The directory part of the relative path, for grouping in the UI.
    pub fn folder(&self) -> &str {
        self.relative.rsplit_once('/').map_or("", |(folder, _)| folder)
    }
}

#[derive(Debug, Default)]
pub struct Vault {
    /// Kept so Phase 8's settings view can show which vault is loaded.
    #[expect(dead_code, reason = "Phase 8's settings view displays this")]
    pub root: PathBuf,
    notes: Vec<Note>,
    /// Lowercased stem -> notes with that stem. Wikilinks resolve through this.
    by_stem: HashMap<String, Vec<NoteId>>,
    /// Note -> the notes it links to. Built lazily as documents are opened,
    /// because parsing 1,000 files up front would make startup crawl.
    links: HashMap<NoteId, Vec<NoteId>>,
}

impl Vault {
    /// Walks `root` and indexes every markdown file under it.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let root =
            root.canonicalize().with_context(|| format!("no vault at {}", root.display()))?;

        let mut notes = Vec::new();

        for entry in WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| !is_skipped(entry.file_name().to_string_lossy().as_ref()))
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "md") {
                continue;
            }

            let Ok(relative) = path.strip_prefix(&root) else { continue };
            notes.push(Note {
                relative: relative.to_string_lossy().into_owned(),
                stem: path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                modified: entry.metadata().ok().and_then(|meta| meta.modified().ok()),
                path: path.to_path_buf(),
            });
        }

        notes.sort_by(|a, b| a.relative.cmp(&b.relative));

        let mut by_stem: HashMap<String, Vec<NoteId>> = HashMap::new();
        for (index, note) in notes.iter().enumerate() {
            by_stem.entry(note.stem.to_lowercase()).or_default().push(NoteId(index));
        }

        Ok(Self { root, notes, by_stem, links: HashMap::new() })
    }

    pub const fn len(&self) -> usize {
        self.notes.len()
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    pub fn get(&self, id: NoteId) -> Option<&Note> {
        self.notes.get(id.0)
    }

    /// Resolves a `[[wikilink]]` target to a note.
    ///
    /// Obsidian matches on the file stem, case-insensitively, and tolerates a
    /// `#heading` or `|alias` suffix. It also allows a partial path
    /// (`Projects/foo`) to disambiguate when two notes share a stem.
    pub fn resolve_link(&self, target: &str) -> Option<NoteId> {
        let target = target.split(['#', '|']).next().unwrap_or(target).trim();
        if target.is_empty() {
            return None;
        }

        let candidates = self.by_stem.get(&stem_of(target).to_lowercase())?;

        // A bare stem with one match is the common case.
        if candidates.len() == 1 || !target.contains('/') {
            return candidates.first().copied();
        }

        // Ambiguous: prefer the note whose relative path ends with the target.
        let needle = target.to_lowercase();
        candidates
            .iter()
            .find(|id| {
                self.get(**id).is_some_and(|note| {
                    let relative = note.relative.to_lowercase();
                    relative == format!("{needle}.md")
                        || relative.ends_with(&format!("/{needle}.md"))
                })
            })
            .or_else(|| candidates.first())
            .copied()
    }

    /// Records which notes a document links to. Called when a note is parsed.
    pub fn record_links(&mut self, from: NoteId, targets: &[String]) {
        let resolved: Vec<NoteId> =
            targets.iter().filter_map(|target| self.resolve_link(target)).collect();
        self.links.insert(from, resolved);
    }

    pub fn outgoing(&self, from: NoteId) -> &[NoteId] {
        self.links.get(&from).map_or(&[], Vec::as_slice)
    }

    /// Notes that link *to* this one, among those parsed so far.
    ///
    /// Only covers notes already opened this session. A full backlink graph
    /// means parsing all 1,000 notes, which belongs behind a background index
    /// rather than in the startup path.
    pub fn backlinks(&self, to: NoteId) -> Vec<NoteId> {
        let mut found: Vec<NoteId> = self
            .links
            .iter()
            .filter(|(_, targets)| targets.contains(&to))
            .map(|(from, _)| *from)
            .collect();
        found.sort_unstable();
        found
    }

    /// The notes changed since a point in time, newest first.
    ///
    /// Phase 5 uses this for "what has the agent touched since it started".
    #[expect(dead_code, reason = "Phase 5 surfaces this per session")]
    pub fn modified_since(&self, since: std::time::SystemTime) -> Vec<NoteId> {
        let mut recent: Vec<(NoteId, std::time::SystemTime)> = self
            .notes
            .iter()
            .enumerate()
            .filter_map(|(index, note)| {
                note.modified.filter(|time| *time > since).map(|time| (NoteId(index), time))
            })
            .collect();
        recent.sort_by_key(|(_, time)| std::cmp::Reverse(*time));
        recent.into_iter().map(|(id, _)| id).collect()
    }
}

fn is_skipped(name: &str) -> bool {
    SKIP_DIRECTORIES.contains(&name)
}

/// The stem of a link target: last path segment, `.md` removed.
fn stem_of(target: &str) -> &str {
    let last = target.rsplit('/').next().unwrap_or(target);
    last.strip_suffix(".md").unwrap_or(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Builds a throwaway vault. Returns the root; the caller removes it.
    fn scratch_vault(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("houston-test-{name}"));
        let _ = fs::remove_dir_all(&root);
        for (path, body) in files {
            let full = root.join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, body).unwrap();
        }
        root
    }

    #[test]
    fn indexes_markdown_and_ignores_everything_else() {
        let root = scratch_vault(
            "index",
            &[
                ("a.md", "# A"),
                ("Projects/b.md", "# B"),
                ("image.png", "not markdown"),
                (".obsidian/workspace.json", "{}"),
                (".git/config", "[core]"),
            ],
        );
        let vault = Vault::open(&root).unwrap();

        assert_eq!(vault.len(), 2, "only the two .md files should be indexed");
        let paths: Vec<&str> = vault.notes().iter().map(|n| n.relative.as_str()).collect();
        assert_eq!(paths, vec!["Projects/b.md", "a.md"]);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resolves_wikilinks_by_stem_case_insensitively() {
        let root = scratch_vault("links", &[("Knowledge/Deep Work.md", "x"), ("other.md", "y")]);
        let vault = Vault::open(&root).unwrap();

        let target = vault.resolve_link("Deep Work").unwrap();
        assert_eq!(vault.get(target).unwrap().stem, "Deep Work");

        assert!(vault.resolve_link("deep work").is_some(), "matching is case-insensitive");
        assert!(vault.resolve_link("Deep Work.md").is_some(), "an .md suffix is tolerated");
        assert!(vault.resolve_link("Deep Work#a-heading").is_some(), "a heading anchor is ignored");
        assert!(vault.resolve_link("Deep Work|shown text").is_some(), "an alias is ignored");
        assert!(vault.resolve_link("nothing here").is_none());
        assert!(vault.resolve_link("").is_none());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_path_qualified_link_picks_the_right_duplicate() {
        let root = scratch_vault("dupes", &[("Projects/notes.md", "a"), ("Archive/notes.md", "b")]);
        let vault = Vault::open(&root).unwrap();

        let archived = vault.resolve_link("Archive/notes").unwrap();
        assert_eq!(vault.get(archived).unwrap().relative, "Archive/notes.md");

        let project = vault.resolve_link("Projects/notes").unwrap();
        assert_eq!(vault.get(project).unwrap().relative, "Projects/notes.md");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn backlinks_come_from_recorded_links() {
        let root = scratch_vault("backlinks", &[("a.md", "x"), ("b.md", "y"), ("c.md", "z")]);
        let mut vault = Vault::open(&root).unwrap();

        let a = vault.resolve_link("a").unwrap();
        let b = vault.resolve_link("b").unwrap();
        let c = vault.resolve_link("c").unwrap();

        vault.record_links(b, &["a".to_string()]);
        vault.record_links(c, &["a".to_string()]);

        assert_eq!(vault.backlinks(a), vec![b, c]);
        assert!(vault.backlinks(b).is_empty());
        assert_eq!(vault.outgoing(b), &[a]);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn opening_a_missing_vault_is_an_error_not_a_panic() {
        assert!(Vault::open("/definitely/not/a/vault").is_err());
    }

    #[test]
    fn folder_is_the_directory_part() {
        let note = Note {
            path: PathBuf::from("/v/Projects/x.md"),
            relative: "Projects/x.md".to_string(),
            stem: "x".to_string(),
            modified: None,
        };
        assert_eq!(note.folder(), "Projects");

        let root_note = Note { relative: "x.md".to_string(), ..note };
        assert_eq!(root_note.folder(), "", "a note at the root has no folder");
    }
}
