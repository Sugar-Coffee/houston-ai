//! Path handling: home expansion and directory completion.
//!
//! Typing a full path into a text field is miserable, so anywhere Houston asks
//! for a directory it offers shell-style completion — `Tab` fills in as far as
//! the candidates agree, and the candidates are listed underneath.

use std::path::{Path, PathBuf};

/// Expands a leading `~`, which people type and `PathBuf` does not understand.
#[must_use]
pub fn expand_home(path: &Path) -> PathBuf {
    let Ok(rest) = path.strip_prefix("~") else { return path.to_path_buf() };
    std::env::var_os("HOME")
        .map_or_else(|| path.to_path_buf(), |home| PathBuf::from(home).join(rest))
}

/// Replaces the home directory with `~`, for display.
#[must_use]
pub fn contract_home(path: &Path) -> String {
    let Some(home) = std::env::var_os("HOME") else { return path.display().to_string() };
    path.strip_prefix(PathBuf::from(home))
        .map_or_else(|_| path.display().to_string(), |rest| format!("~/{}", rest.display()))
}

/// What completing a partial path produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Completion {
    /// The input extended as far as every candidate agrees. This is what `Tab`
    /// commits, so it is only ever an extension — never a replacement.
    pub extended: String,
    /// Candidate directory names, for showing under the field.
    pub matches: Vec<String>,
}

/// Completes a partial directory path.
///
/// Directories only: every caller is asking "where should this run", and
/// offering files would be noise. A trailing slash means "list what is in
/// here"; anything else filters the last segment.
#[must_use]
pub fn complete_directory(input: &str) -> Completion {
    let (parent, prefix) = split_input(input);
    let expanded = expand_home(Path::new(&parent));

    // A path that does not exist yet is a normal thing to be typing. Return
    // the input unchanged rather than a default `Completion`, whose empty
    // `extended` would wipe what the user had written.
    let Ok(entries) = std::fs::read_dir(&expanded) else {
        return Completion { extended: input.to_string(), matches: Vec::new() };
    };

    let mut matches: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            // Hidden directories only appear once you type the dot, the same
            // as a shell.
            if name.starts_with('.') && !prefix.starts_with('.') {
                return None;
            }
            name.starts_with(&prefix).then_some(name)
        })
        .collect();

    matches.sort_unstable();

    let extended = common_prefix(&matches).map_or_else(
        || input.to_string(),
        |shared| {
            let mut path = format!("{parent}{shared}");
            // One unambiguous match gets a trailing slash, so the next Tab
            // descends into it rather than re-completing the same name.
            if matches.len() == 1 {
                path.push('/');
            }
            path
        },
    );

    Completion { extended, matches }
}

/// Splits an input into the directory to search and the fragment to match.
fn split_input(input: &str) -> (String, String) {
    match input.rsplit_once('/') {
        Some((parent, prefix)) => (format!("{parent}/"), prefix.to_string()),
        // No slash at all: complete against the current directory.
        None => ("./".to_string(), input.to_string()),
    }
}

/// The longest prefix every candidate shares.
fn common_prefix(candidates: &[String]) -> Option<String> {
    let first = candidates.first()?;
    let shared = first
        .char_indices()
        .take_while(|(index, character)| {
            candidates.iter().all(|candidate| {
                candidate.chars().nth(*index).is_some_and(|other| other == *character)
            })
        })
        .count();
    Some(first.chars().take(shared).collect())
}

/// A filesystem-safe name derived from a title.
///
/// Used to name a worktree directory after its session, so the folder on disk
/// is recognisable rather than a hash.
#[must_use]
pub fn slug(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut last_was_dash = true;

    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            slug.extend(character.to_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    let trimmed = slug.trim_end_matches('-').to_string();
    if trimmed.is_empty() { "session".to_string() } else { trimmed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_expands_and_contracts_symmetrically() {
        let home = PathBuf::from(std::env::var("HOME").unwrap());

        assert_eq!(expand_home(Path::new("~/notes")), home.join("notes"));
        assert_eq!(contract_home(&home.join("notes")), "~/notes");

        // Paths that are not under home are left alone in both directions.
        assert_eq!(expand_home(Path::new("/etc")), PathBuf::from("/etc"));
        assert_eq!(contract_home(Path::new("/etc")), "/etc");

        // Only a leading `~` component counts, not a name that starts with it.
        assert_eq!(expand_home(Path::new("~notes")), PathBuf::from("~notes"));
    }

    #[test]
    fn completing_an_unambiguous_directory_appends_a_slash() {
        // `/usr/lo` can only be `/usr/local` on any sane unix.
        let completion = complete_directory("/usr/lo");
        assert_eq!(completion.extended, "/usr/local/");
        assert_eq!(completion.matches, vec!["local"]);
    }

    #[test]
    fn completing_extends_only_as_far_as_candidates_agree() {
        let root = std::env::temp_dir().join("houston-complete-test");
        let _ = std::fs::remove_dir_all(&root);
        for name in ["alpha-one", "alpha-two", "beta"] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }

        let base = format!("{}/", root.display());
        let completion = complete_directory(&format!("{base}al"));

        assert_eq!(completion.extended, format!("{base}alpha-"), "stops where they diverge");
        assert_eq!(completion.matches, vec!["alpha-one", "alpha-two"]);
        assert!(!completion.extended.ends_with('/'), "ambiguous matches get no slash");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_trailing_slash_lists_the_directorys_contents() {
        let root = std::env::temp_dir().join("houston-complete-list");
        let _ = std::fs::remove_dir_all(&root);
        for name in ["one", "two"] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
        std::fs::write(root.join("a-file.txt"), "x").unwrap();

        let completion = complete_directory(&format!("{}/", root.display()));
        assert_eq!(completion.matches, vec!["one", "two"], "files are not offered");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn hidden_directories_appear_only_once_you_type_the_dot() {
        let root = std::env::temp_dir().join("houston-complete-hidden");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::create_dir_all(root.join("visible")).unwrap();

        let listed = complete_directory(&format!("{}/", root.display()));
        assert_eq!(listed.matches, vec!["visible"]);

        let asked = complete_directory(&format!("{}/.", root.display()));
        assert_eq!(asked.matches, vec![".hidden"]);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn completion_expands_a_tilde_before_searching() {
        // Whatever is in the home directory, asking for `~/` must find some.
        let completion = complete_directory("~/");
        assert!(!completion.matches.is_empty(), "home should contain directories");
    }

    #[test]
    fn nothing_matching_leaves_the_input_untouched() {
        let completion = complete_directory("/definitely/not/here/at/all");
        assert_eq!(completion.extended, "/definitely/not/here/at/all");
        assert!(completion.matches.is_empty());
    }

    #[test]
    fn slugs_are_filesystem_safe_and_readable() {
        assert_eq!(slug("Auth Refactor"), "auth-refactor");
        assert_eq!(slug("COG-341: fix  the /thing/"), "cog-341-fix-the-thing");
        assert_eq!(slug("  leading and trailing  "), "leading-and-trailing");
        assert_eq!(slug("already-fine"), "already-fine");
    }

    #[test]
    fn a_slug_is_never_empty() {
        // An unnamed session still needs a directory name.
        assert_eq!(slug(""), "session");
        assert_eq!(slug("!!!"), "session");
        assert_eq!(slug("   "), "session");
    }
}
