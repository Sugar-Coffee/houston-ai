//! What an agent actually changed.
//!
//! Houston can start an agent and tell you when it wants you. Until this
//! module it could not tell you what came out the other end, which left the
//! most-asked question of a multi-agent session — "what did *this* one do?" —
//! answerable only in another window. See ADR-0009.
//!
//! Read-only by construction. Nothing here writes to a repository, and the
//! index is never touched: a summary that quietly staged files would be a
//! trap, not a feature.

use anyhow::{Context, Result};
use std::{path::Path, process::Command};

/// A one-line summary of the work in a directory.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Changes {
    pub files: usize,
    pub insertions: usize,
    pub deletions: usize,
}

impl Changes {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.files == 0
    }

    /// The long form for a header.
    #[must_use]
    pub fn describe(&self) -> String {
        let files = if self.files == 1 { "1 file" } else { &format!("{} files", self.files) };
        format!("{files}, +{} −{}", self.insertions, self.deletions)
    }
}

/// What has changed in `directory` since the last commit.
///
/// `None` when the directory is not a repository, or git cannot be run. That
/// is not an error worth showing: most shell sessions are not in a repository
/// and a card that said "not a git repo" would be noise on every one of them.
#[must_use]
pub fn changes_in(directory: &Path) -> Option<Changes> {
    if !crate::worktree::is_repository(directory) {
        return None;
    }

    // `HEAD` rather than a bare `git diff`, so work the agent has already
    // staged still counts. An agent that ran `git add` has not undone its
    // own work, and a summary that dropped to zero when it did would be
    // actively misleading.
    let tracked =
        parse_numstat(&run(directory, &["diff", "--numstat", "HEAD"]).unwrap_or_default());
    let untracked = untracked_changes(directory);

    Some(Changes {
        files: tracked.files + untracked.files,
        insertions: tracked.insertions + untracked.insertions,
        deletions: tracked.deletions + untracked.deletions,
    })
}

/// Sums `git diff --numstat` output.
///
/// Numstat rather than `--shortstat`: three tab-separated columns parse the
/// same way forever, where the prose of "3 files changed, 1 insertion(+)"
/// carries pluralisation and omits clauses that happen to be zero.
#[must_use]
pub fn parse_numstat(output: &str) -> Changes {
    let mut changes = Changes::default();

    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let mut columns = line.split('\t');
        let (Some(added), Some(removed)) = (columns.next(), columns.next()) else { continue };

        changes.files += 1;
        // Binary files report `-`, which is not zero but is not countable.
        changes.insertions += added.parse::<usize>().unwrap_or(0);
        changes.deletions += removed.parse::<usize>().unwrap_or(0);
    }
    changes
}

/// How many untracked entries are worth opening.
///
/// The number on a card is a rough size. Past a few dozen files it has
/// stopped changing the picture and is only costing time, and the time is
/// paid every three seconds for as long as that session is selected.
const UNTRACKED_LIMIT: usize = 64;

/// And how large one of them is worth reading to count its lines.
const COUNTED_BYTES: u64 = 1 << 20;

/// Files the agent created, which no `git diff` will ever mention.
///
/// Worth the extra command. Writing new files is most of what a coding agent
/// does, and a card reading `+0 −0` while it had just written six of them
/// would teach you to distrust the number.
///
/// **Bounded, because this is on a timer.** See [`untracked_entries`] for how
/// the list is kept short; this caps what is done with it. A directory is
/// counted as the one entry git reported and never opened, a file over a
/// megabyte is counted but not read, and past [`UNTRACKED_LIMIT`] entries
/// nothing more is opened at all.
fn untracked_changes(directory: &Path) -> Changes {
    let mut changes = Changes::default();

    for name in untracked_entries(directory) {
        changes.files += 1;
        if changes.files > UNTRACKED_LIMIT || name.ends_with('/') {
            continue;
        }

        let path = directory.join(&name);
        if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > COUNTED_BYTES) {
            continue;
        }
        // Binary files still count as a file; their line count does not.
        if let Ok(contents) = std::fs::read_to_string(&path) {
            changes.insertions += contents.lines().count();
        }
    }
    changes
}

/// Untracked, non-ignored entries, named relative to `directory`.
///
/// **Git's default listing, not `--untracked-files=all`.** The difference is
/// whether an untracked *directory* comes back as one entry or as every file
/// inside it, and it is the difference between a summary and a catastrophe: a
/// repository here with an unignored `.pnpm-store/` reported 6 entries by
/// default and 72,794 with `-uall`. Houston then opened all 72,794 — a
/// gigabyte of reads — every three seconds, on the thread that handles
/// keystrokes. The app was not slow, it was gone.
///
/// A collapsed directory is one entry with no line count, which is the honest
/// answer for a card that has room for a number and not for a file tree.
///
/// Relative on purpose. Git echoes whatever path it is given straight into
/// the `+++ b/…` header, and an absolute one under a temp directory is wider
/// than the pane — the header for a new file then clips to nothing useful
/// while every tracked file next to it reads fine.
fn untracked_entries(directory: &Path) -> Vec<String> {
    let output = run(directory, &["status", "--porcelain"]);

    output
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.strip_prefix("?? "))
        .map(|name| unquote(name).to_string())
        .collect()
}

/// Undoes the quoting git applies to paths with unusual characters.
///
/// Only the outer quotes are removed. A path containing an escaped byte is
/// left as git wrote it rather than half-decoded into something that does not
/// exist on disk.
fn unquote(name: &str) -> &str {
    name.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).unwrap_or(name)
}

/// The full diff for a directory, as unified-diff text.
///
/// Untracked files are appended as their own diffs, generated by git rather
/// than assembled here, so a new file reads exactly like every other hunk.
pub fn full(directory: &Path) -> Result<String> {
    if !crate::worktree::is_repository(directory) {
        anyhow::bail!("{} is not a git repository", directory.display());
    }

    let mut diff = run(directory, &["diff", "HEAD"]).context("could not read the diff")?;

    // One subprocess per untracked file, so the same cap applies here and for
    // a sharper reason: unbounded, pressing `v` on a repository with a large
    // unignored directory in it would spawn seventy thousand copies of git.
    // A directory is skipped rather than expanded — `--no-index` on one
    // recurses, which is the explosion again by another route.
    let entries = untracked_entries(directory);
    let shown = entries.iter().filter(|name| !name.ends_with('/')).take(UNTRACKED_LIMIT);

    for name in shown {
        // `--no-index` against /dev/null is how git itself renders a new file.
        // It exits non-zero because the two sides differ, which is the point.
        if let Some(added) = run(directory, &["diff", "--no-index", "--", "/dev/null", name]) {
            diff.push_str(&added);
        }
    }

    // Said rather than silently dropped: a review that quietly stops short is
    // worse than one that admits where it stopped.
    let left_out = entries.iter().filter(|name| name.ends_with('/')).count()
        + entries
            .iter()
            .filter(|name| !name.ends_with('/'))
            .count()
            .saturating_sub(UNTRACKED_LIMIT);
    if left_out > 0 {
        diff.push_str(&format!("\n--- {left_out} more untracked entries not shown ---\n"));
    }

    Ok(diff)
}

/// An open diff and how far down it you have scrolled.
///
/// The text is captured once when the view opens rather than re-read per
/// frame. An agent writing files while you read its diff would otherwise
/// shift the lines under you, which is the same class of bug jump mode was
/// designed around in the editor.
#[derive(Debug, Clone)]
pub struct View {
    /// The session this belongs to, for the title bar.
    pub title: String,
    pub lines: Vec<String>,
    /// First visible line.
    pub scroll: usize,
    /// A summary of what is below, so the header can say it in words.
    pub changes: Changes,
}

impl View {
    /// Captures the diff for a directory.
    pub fn open(title: String, directory: &Path) -> Result<Self> {
        let text = full(directory)?;
        let changes = changes_in(directory).unwrap_or_default();

        if text.trim().is_empty() {
            anyhow::bail!("{title} has no uncommitted changes");
        }

        Ok(Self { title, lines: text.lines().map(str::to_string).collect(), scroll: 0, changes })
    }

    /// Moves the viewport, stopping at both ends.
    ///
    /// Clamped so the last line can reach the top but no further: scrolling
    /// into blank space below a diff tells you nothing and loses your place.
    pub fn scroll_by(&mut self, delta: isize) {
        let last = self.lines.len().saturating_sub(1);
        self.scroll = self.scroll.saturating_add_signed(delta).min(last);
    }

    pub const fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_to_bottom(&mut self, height: usize) {
        self.scroll = self.lines.len().saturating_sub(height.max(1));
    }

    /// The lines to draw, given how tall the pane is.
    #[must_use]
    pub fn visible(&self, height: usize) -> &[String] {
        let start = self.scroll.min(self.lines.len());
        let end = start.saturating_add(height).min(self.lines.len());
        &self.lines[start..end]
    }
}

/// What a line of unified diff means, which is all the renderer needs to know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// `diff --git`, `index`, `+++`, `---`: which file this is.
    File,
    /// An `@@` hunk header.
    Hunk,
    Added,
    Removed,
    Context,
}

/// Classifies one line of unified diff.
///
/// Order matters here. `+++ b/file` starts with `+` and is a header, not an
/// added line, so the file markers have to be tested before the signs.
#[must_use]
pub fn classify(line: &str) -> Row {
    if line.starts_with("+++") || line.starts_with("---") {
        return Row::File;
    }
    if line.starts_with("diff --git")
        || line.starts_with("index ")
        || line.starts_with("new file")
        || line.starts_with("deleted file")
        || line.starts_with("similarity index")
        || line.starts_with("rename ")
    {
        return Row::File;
    }
    if line.starts_with("@@") {
        return Row::Hunk;
    }
    match line.as_bytes().first() {
        Some(b'+') => Row::Added,
        Some(b'-') => Row::Removed,
        _ => Row::Context,
    }
}

/// Runs git and returns stdout, ignoring the exit status.
///
/// Deliberately not `worktree::git`, which treats a non-zero exit as an error.
/// Diff commands exit 1 to mean "there were differences" — the single most
/// expected outcome here — so a runner that failed on it would report success
/// only when there was nothing to show.
fn run(directory: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).current_dir(directory).output().ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A throwaway repository with one commit.
    ///
    /// Under the temp directory rather than anywhere near `$HOME` — see the
    /// note in `CLAUDE.md` about the test that repointed a live install.
    fn scratch_repository(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("houston-diff-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();

        for arguments in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.email", "test@example.com"],
            &["config", "user.name", "Test"],
        ] {
            run(&path, arguments).unwrap();
        }
        std::fs::write(path.join("README.md"), "one\ntwo\nthree\n").unwrap();
        run(&path, &["add", "."]).unwrap();
        run(&path, &["commit", "-q", "-m", "first"]).unwrap();
        path
    }

    #[test]
    fn numstat_columns_are_summed_across_files() {
        let changes = parse_numstat("10\t2\tsrc/a.rs\n5\t0\tsrc/b.rs\n");

        assert_eq!(changes.files, 2, "one row is one file");
        assert_eq!(changes.insertions, 15);
        assert_eq!(changes.deletions, 2);
    }

    #[test]
    fn a_binary_file_counts_as_changed_without_inventing_a_line_count() {
        let changes = parse_numstat("-\t-\tlogo.png\n3\t1\tsrc/a.rs\n");

        assert_eq!(changes.files, 2, "the binary file still changed");
        assert_eq!(changes.insertions, 3, "its unknowable lines count as none, not as an error");
        assert_eq!(changes.deletions, 1);
    }

    #[test]
    fn nothing_changed_is_nothing_rather_than_a_failure() {
        assert_eq!(parse_numstat(""), Changes::default());
        assert!(parse_numstat("").is_empty(), "an empty summary knows it is empty");
    }

    #[test]
    fn a_file_the_agent_created_is_counted_even_though_no_diff_mentions_it() {
        let repository = scratch_repository("untracked");
        std::fs::write(repository.join("new.rs"), "a\nb\nc\nd\n").unwrap();

        let changes = changes_in(&repository).expect("a repository reports changes");
        assert_eq!(changes.files, 1, "a new file is a changed file");
        assert_eq!(
            changes.insertions, 4,
            "writing new files is most of what an agent does, so +0 would be a lie"
        );

        std::fs::remove_dir_all(&repository).ok();
    }

    /// **The freeze.** A repository with an unignored `.pnpm-store/` in it
    /// reported 6 untracked entries by default and 72,794 with `-uall`.
    /// Houston opened every one of them — a gigabyte — every three seconds,
    /// on the thread that reads the keyboard.
    #[test]
    fn an_untracked_directory_is_one_entry_rather_than_everything_inside_it() {
        let repository = scratch_repository("untracked-dir");
        let store = repository.join("store");
        std::fs::create_dir_all(&store).unwrap();
        for index in 0..500 {
            std::fs::write(store.join(format!("{index}.txt")), "a\nb\nc\n").unwrap();
        }

        let changes = changes_in(&repository).expect("a repository reports changes");

        assert_eq!(changes.files, 1, "the directory is one entry, not five hundred");
        assert_eq!(changes.insertions, 0, "and nothing inside it was opened to be counted");

        std::fs::remove_dir_all(&repository).ok();
    }

    /// The count is a rough size. Reading a gigabyte to refine it is not a
    /// trade anybody would make knowingly.
    #[test]
    fn a_large_untracked_file_is_counted_but_not_read() {
        let repository = scratch_repository("untracked-big");
        let line = "x".repeat(1024) + "\n";
        std::fs::write(repository.join("huge.log"), line.repeat(2048)).unwrap();
        std::fs::write(repository.join("small.txt"), "one\ntwo\n").unwrap();

        let changes = changes_in(&repository).expect("a repository reports changes");

        assert_eq!(changes.files, 2, "both are changes");
        assert_eq!(changes.insertions, 2, "only the small one contributed lines");

        std::fs::remove_dir_all(&repository).ok();
    }

    /// Past the cap the count stops moving, and the loop stops opening files.
    #[test]
    fn a_great_many_loose_untracked_files_stop_being_opened() {
        let repository = scratch_repository("untracked-many");
        for index in 0..(UNTRACKED_LIMIT + 40) {
            std::fs::write(repository.join(format!("note-{index}.md")), "a\nb\n").unwrap();
        }

        let changes = changes_in(&repository).expect("a repository reports changes");

        assert_eq!(changes.files, UNTRACKED_LIMIT + 40, "every one of them is still counted");
        assert_eq!(
            changes.insertions,
            UNTRACKED_LIMIT * 2,
            "but only the first {UNTRACKED_LIMIT} were opened"
        );

        std::fs::remove_dir_all(&repository).ok();
    }

    /// `git diff --no-index` on a directory recurses, which is the same
    /// explosion by another route — and one subprocess per file besides.
    #[test]
    fn the_review_says_what_it_left_out_rather_than_stopping_quietly() {
        let repository = scratch_repository("review-cap");
        let store = repository.join("store");
        std::fs::create_dir_all(&store).unwrap();
        for index in 0..200 {
            std::fs::write(store.join(format!("{index}.txt")), "a\n").unwrap();
        }

        let diff = full(&repository).expect("a repository has a diff");

        assert!(!diff.contains("store/0.txt"), "the directory was not expanded");
        assert!(diff.contains("1 more untracked entries not shown"), "and it says so");

        std::fs::remove_dir_all(&repository).ok();
    }

    #[test]
    fn work_the_agent_has_already_staged_still_counts() {
        let repository = scratch_repository("staged");
        std::fs::write(repository.join("README.md"), "one\ntwo\nthree\nfour\n").unwrap();
        run(&repository, &["add", "."]).unwrap();

        let changes = changes_in(&repository).expect("a repository reports changes");
        assert_eq!(
            changes.insertions, 1,
            "an agent running `git add` has not undone its work, so the total must not drop"
        );

        std::fs::remove_dir_all(&repository).ok();
    }

    #[test]
    fn a_directory_outside_a_repository_reports_nothing_rather_than_zero() {
        let plain = std::env::temp_dir().join("houston-diff-plain");
        let _ = std::fs::remove_dir_all(&plain);
        std::fs::create_dir_all(&plain).unwrap();

        assert!(
            changes_in(&plain).is_none(),
            "most shell sessions are not in a repository; a zero on every card would be noise"
        );

        std::fs::remove_dir_all(&plain).ok();
    }

    #[test]
    fn a_new_file_is_named_relative_so_its_header_fits_the_pane() {
        let repository = scratch_repository("relative");
        std::fs::write(repository.join("added.rs"), "new\n").unwrap();

        let diff = full(&repository).unwrap();

        assert!(diff.contains("+++ b/added.rs"), "the header names the file, not its full path");
        assert!(
            !diff.contains(&repository.to_string_lossy().into_owned()),
            "an absolute path here is wider than the pane and clips to nothing useful"
        );

        std::fs::remove_dir_all(&repository).ok();
    }

    #[test]
    fn the_full_diff_shows_edits_and_new_files_alike() {
        let repository = scratch_repository("full");
        std::fs::write(repository.join("README.md"), "one\ntwo\nCHANGED\n").unwrap();
        std::fs::write(repository.join("added.rs"), "brand new\n").unwrap();

        let diff = full(&repository).unwrap();

        assert!(diff.contains("+CHANGED"), "an edit to a tracked file appears");
        assert!(diff.contains("+brand new"), "a file the agent created appears too");

        std::fs::remove_dir_all(&repository).ok();
    }

    #[test]
    fn a_summary_reads_the_way_it_would_be_said_aloud() {
        let one = Changes { files: 1, insertions: 3, deletions: 0 };
        assert_eq!(one.describe(), "1 file, +3 −0", "not '1 files'");

        let many = Changes { files: 4, insertions: 12, deletions: 9 };
        assert_eq!(many.describe(), "4 files, +12 −9");
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;

    fn view(lines: usize) -> View {
        View {
            title: "test".to_string(),
            lines: (0..lines).map(|n| format!("line {n}")).collect(),
            scroll: 0,
            changes: Changes::default(),
        }
    }

    #[test]
    fn a_file_header_is_not_an_added_line_despite_starting_with_a_plus() {
        assert_eq!(classify("+++ b/src/main.rs"), Row::File, "'+++' is a header");
        assert_eq!(classify("--- a/src/main.rs"), Row::File, "'---' is a header");
        assert_eq!(classify("+let x = 1;"), Row::Added);
        assert_eq!(classify("-let x = 0;"), Row::Removed);
        assert_eq!(classify(" unchanged"), Row::Context);
        assert_eq!(classify("@@ -1,3 +1,4 @@"), Row::Hunk);
        assert_eq!(classify("diff --git a/x b/x"), Row::File);
    }

    #[test]
    fn scrolling_stops_at_both_ends_rather_than_running_off() {
        let mut view = view(10);

        view.scroll_by(-5);
        assert_eq!(view.scroll, 0, "scrolling up from the top stays at the top");

        view.scroll_by(1000);
        assert_eq!(view.scroll, 9, "the last line can reach the top and no further");
    }

    #[test]
    fn the_visible_window_never_reads_past_the_end() {
        let view = view(3);

        assert_eq!(view.visible(10).len(), 3, "asking for more lines than exist is not a panic");
        assert_eq!(view.visible(2).len(), 2);
    }

    #[test]
    fn the_bottom_shows_the_last_screenful_rather_than_the_last_line() {
        let mut view = view(100);
        view.scroll_to_bottom(20);

        assert_eq!(view.scroll, 80, "G should fill the pane, not leave one line above blank");
    }
}
