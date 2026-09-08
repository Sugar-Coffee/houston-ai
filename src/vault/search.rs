//! Fuzzy matching over note paths.
//!
//! ADR-0004's exit criterion is finding any note in under three seconds. With
//! 1,065 notes that is a matching problem, not a rendering one.

use super::{NoteId, Vault};
use nucleo_matcher::{
    Config, Matcher as Nucleo, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};

/// Scores notes against a query.
///
/// Wraps `nucleo`, which is the matcher behind Helix and Telescope, so ranking
/// behaves the way a fuzzy finder is expected to.
pub struct Matcher {
    inner: Nucleo,
    buffer: Vec<char>,
}

impl Matcher {
    #[must_use]
    pub fn new() -> Self {
        // `match_paths` biases toward the final path segment, which is what a
        // person means when they type a note name.
        Self { inner: Nucleo::new(Config::DEFAULT.match_paths()), buffer: Vec::new() }
    }

    /// Notes matching `query`, best first.
    ///
    /// An empty query returns everything in index order, so the picker opens
    /// showing the vault rather than showing nothing.
    pub fn search(&mut self, vault: &Vault, query: &str, limit: usize) -> Vec<NoteId> {
        if query.trim().is_empty() {
            return (0..vault.len().min(limit)).map(NoteId).collect();
        }

        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

        let mut scored: Vec<(u32, NoteId)> = vault
            .notes()
            .iter()
            .enumerate()
            .filter_map(|(index, note)| {
                self.buffer.clear();
                let haystack = Utf32Str::new(&note.relative, &mut self.buffer);
                pattern.score(haystack, &mut self.inner).map(|score| (score, NoteId(index)))
            })
            .collect();

        // Highest score first; ties broken by index so results never jitter
        // between identical queries.
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().take(limit).map(|(_, id)| id).collect()
    }
}

impl Default for Matcher {
    fn default() -> Self {
        Self::new()
    }
}

/// A line of a note that contains `needle`, for full-text search.
#[derive(Debug, Clone)]
pub struct Hit {
    pub note: NoteId,
    /// 1-based, to match how editors number lines.
    pub line: usize,
    pub text: String,
}

/// Plain case-insensitive substring search across the vault.
///
/// Deliberately not fuzzy: when searching *content* rather than names, people
/// mean the literal words. Reads files on demand rather than holding the whole
/// vault in memory.
#[must_use]
pub fn grep(vault: &Vault, needle: &str, limit: usize) -> Vec<Hit> {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    for (index, note) in vault.notes().iter().enumerate() {
        let Ok(body) = std::fs::read_to_string(&note.path) else { continue };

        for (number, line) in body.lines().enumerate() {
            if line.to_lowercase().contains(&needle) {
                hits.push(Hit {
                    note: NoteId(index),
                    line: number + 1,
                    text: line.trim().chars().take(160).collect(),
                });
                if hits.len() >= limit {
                    return hits;
                }
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;
    use std::fs;

    fn scratch_vault(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("houston-search-{name}"));
        let _ = fs::remove_dir_all(&root);
        for (path, body) in files {
            let full = root.join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, body).unwrap();
        }
        root
    }

    #[test]
    fn an_empty_query_returns_the_whole_vault() {
        let root = scratch_vault("empty", &[("a.md", ""), ("b.md", "")]);
        let vault = Vault::open(&root).unwrap();
        let mut matcher = Matcher::new();

        assert_eq!(matcher.search(&vault, "", 10).len(), 2);
        assert_eq!(matcher.search(&vault, "   ", 10).len(), 2, "whitespace is still empty");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fuzzy_matching_finds_a_note_from_an_abbreviation() {
        let root = scratch_vault(
            "fuzzy",
            &[("Projects/payments-rebuild.md", ""), ("Daily/2026-09-08.md", "")],
        );
        let vault = Vault::open(&root).unwrap();
        let mut matcher = Matcher::new();

        let results = matcher.search(&vault, "payreb", 10);
        assert!(!results.is_empty(), "an abbreviation should still match");
        assert_eq!(vault.get(results[0]).unwrap().stem, "payments-rebuild");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn results_are_capped_by_the_limit() {
        let files: Vec<(String, String)> =
            (0..50).map(|i| (format!("note-{i}.md"), String::new())).collect();
        let borrowed: Vec<(&str, &str)> =
            files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let root = scratch_vault("limit", &borrowed);
        let vault = Vault::open(&root).unwrap();
        let mut matcher = Matcher::new();

        assert_eq!(matcher.search(&vault, "note", 5).len(), 5);
        assert_eq!(matcher.search(&vault, "", 5).len(), 5, "the cap applies to empty queries too");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn grep_finds_content_and_reports_one_based_lines() {
        let root = scratch_vault("grep", &[("a.md", "first\nsecond MATCH here\nthird")]);
        let vault = Vault::open(&root).unwrap();

        let hits = grep(&vault, "match", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 2, "line numbers are 1-based");
        assert_eq!(hits[0].text, "second MATCH here");

        assert!(grep(&vault, "", 10).is_empty(), "an empty needle matches nothing");

        fs::remove_dir_all(&root).ok();
    }
}
