//! Git worktrees, so several agents can work on one repository at once
//! without treading on each other.
//!
//! Houston keeps every worktree it creates under `~/.houston/worktrees/`,
//! named after the session that asked for it. That means a worktree is never
//! left inside the user's project directory, and cleaning up is a matter of
//! looking in one place.
//!
//! Shelling out to `git` rather than linking a library: worktrees are a
//! porcelain feature, the commands are stable, and `git` is already required
//! for any of this to make sense.

use crate::paths;
use anyhow::{Context, Result, bail};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Where Houston keeps the worktrees it creates.
pub fn root() -> Result<PathBuf> {
    let directory = crate::hooks::state_dir()?.join("worktrees");
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    Ok(directory)
}

/// A worktree Houston created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    /// Directory name under `root()`, which is also how sessions refer to it.
    pub name: String,
    pub path: PathBuf,
    /// The repository it belongs to.
    pub repository: PathBuf,
    pub branch: Option<String>,
    /// Uncommitted changes present.
    pub dirty: bool,
    /// Commits this branch has that its upstream does not, and vice versa.
    ///
    /// Both zero when there is no upstream, which is the normal state of a
    /// worktree nobody has pushed yet. "Nothing to compare against" and "in
    /// step with the remote" look the same here; the difference has not been
    /// worth a third state to render.
    pub ahead: usize,
    pub behind: usize,
    /// What has changed in it. `None` if it could not be read.
    pub changes: Option<crate::diff::Changes>,
}

/// What a worktree is *for*, which is the question the manager answers.
///
/// Ordered by how much it wants your attention. "Orphaned" is the one worth
/// naming out loud: a worktree Houston made, whose repository has since been
/// moved or deleted, cannot be landed or removed by git and will sit on disk
/// until somebody notices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Its repository is gone. Git cannot manage it any more; only `rm` can.
    Orphaned,
    /// A live session is working in it.
    InUse,
    /// No session is using it, and there is work in it that is not committed.
    Abandoned,
    /// No session is using it, and nothing would be lost by removing it.
    Idle,
}

impl Status {
    /// What this row says about itself.
    ///
    /// **Each label names the reason, not the mood.** "idle" was accurate and
    /// useless: a worktree nobody is using and a worktree whose repository has
    /// been deleted are both "not doing anything", and they want completely
    /// different things from you. One is safe to remove, one is holding work
    /// you have not committed, and one cannot be removed by git at all.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Orphaned => "repository gone",
            Self::InUse => "session running",
            Self::Abandoned => "no session, uncommitted",
            Self::Idle => "no session, clean",
        }
    }

    /// What removing it would cost you, in a few words.
    ///
    /// The label says what the state is; this says what to do about it. Shown
    /// beside the selected row rather than on every one, because four copies
    /// of advice is not advice.
    #[must_use]
    pub const fn advice(self) -> &'static str {
        match self {
            Self::Orphaned => "git cannot touch it; removing deletes the directory",
            Self::InUse => "close its session before removing it",
            Self::Abandoned => "removing it loses the uncommitted work",
            Self::Idle => "safe to remove",
        }
    }

    #[must_use]
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Orphaned => "⚠",
            Self::InUse => "●",
            Self::Abandoned => "◆",
            Self::Idle => "·",
        }
    }
}

impl Worktree {
    /// Classifies this worktree, given the sessions currently running.
    #[must_use]
    pub fn status(&self, in_use: bool) -> Status {
        if self.repository.as_os_str().is_empty() || !self.repository.exists() {
            return Status::Orphaned;
        }
        if in_use {
            return Status::InUse;
        }
        if self.dirty { Status::Abandoned } else { Status::Idle }
    }
}

/// The branch checked out in a directory, by reading `.git/HEAD`.
///
/// Deliberately not `git rev-parse`: this is shown for every session in the
/// sidebar and refreshed while you watch, and spawning a process per session
/// per refresh to read one short file would be absurd.
///
/// Handles the worktree case, where `.git` is a *file* pointing at the real
/// git directory rather than being one.
#[must_use]
pub fn branch_of(directory: &Path) -> Option<String> {
    let git = find_git(directory)?;

    let head = std::fs::read_to_string(git.join("HEAD")).ok()?;
    let head = head.trim();

    // `ref: refs/heads/main` when on a branch; a bare hash when detached.
    head.strip_prefix("ref: refs/heads/").map_or_else(
        || head.get(..8).map(|short| format!("detached {short}")),
        |branch| Some(branch.to_string()),
    )
}

/// Walks up looking for `.git`, resolving the worktree indirection.
fn find_git(directory: &Path) -> Option<PathBuf> {
    for ancestor in directory.ancestors() {
        let candidate = ancestor.join(".git");

        if candidate.is_dir() {
            return Some(candidate);
        }
        if candidate.is_file() {
            // A worktree's `.git` is `gitdir: /path/to/repo/.git/worktrees/name`.
            let contents = std::fs::read_to_string(&candidate).ok()?;
            let path = contents.trim().strip_prefix("gitdir:")?.trim();
            return Some(PathBuf::from(path));
        }
    }
    None
}

/// Whether a directory is inside a git repository.
#[must_use]
pub fn is_repository(directory: &Path) -> bool {
    git(directory, &["rev-parse", "--git-dir"]).is_ok()
}

/// The repository root containing `directory`.
pub fn repository_root(directory: &Path) -> Result<PathBuf> {
    let output = git(directory, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(output.trim()))
}

/// The *main* repository a directory belongs to.
///
/// Inside a worktree, `--show-toplevel` reports the worktree itself, so using
/// it would make a worktree think it was its own repository — and `git
/// worktree remove` would then be asked to clean up from the wrong place.
/// `--git-common-dir` points at the main checkout's `.git` in both cases.
pub fn main_repository(directory: &Path) -> Result<PathBuf> {
    let output = git(directory, &["rev-parse", "--git-common-dir"])?;
    let common = PathBuf::from(output.trim());

    // Relative when git is run from inside the repository it names.
    let common = if common.is_absolute() { common } else { directory.join(common) };

    let root = common.parent().unwrap_or(&common).to_path_buf();
    root.canonicalize().or(Ok(root))
}

/// Creates a worktree for `repository` on a new branch.
pub fn create(repository: &Path, requested_name: &str) -> Result<Worktree> {
    create_in(&root()?, repository, requested_name)
}

/// Creates a worktree under an explicit root.
///
/// The root is a parameter rather than resolved inside, so tests never share
/// the real `~/.houston/worktrees` — see the note in `CLAUDE.md` about
/// functions that resolve `$HOME` paths internally being untestable safely.
///
/// The name is slugified, and a collision gets a numeric suffix rather than an
/// error — being told "that name is taken" while trying to start work is a
/// worse experience than getting `auth-refactor-2`.
pub fn create_in(root: &Path, repository: &Path, requested_name: &str) -> Result<Worktree> {
    if !is_repository(repository) {
        bail!("{} is not a git repository", repository.display());
    }
    let repository = repository_root(repository)?;

    let base = paths::slug(requested_name);
    let (name, path) = available_name(root, &base)?;

    // `-b` creates the branch; without it a second worktree on the same branch
    // is refused by git.
    let branch = name.clone();
    git(&repository, &["worktree", "add", "-b", &branch, &path.to_string_lossy(), "HEAD"])
        .with_context(|| format!("could not create a worktree at {}", path.display()))?;

    Ok(Worktree {
        name,
        path,
        repository,
        branch: Some(branch),
        dirty: false,
        ahead: 0,
        behind: 0,
        changes: None,
    })
}

/// A free directory name under `root`, suffixing on collision.
fn available_name(root: &Path, base: &str) -> Result<(String, PathBuf)> {
    std::fs::create_dir_all(root)?;

    for suffix in 1..1000 {
        let name = if suffix == 1 { base.to_string() } else { format!("{base}-{suffix}") };
        let path = root.join(&name);
        if !path.exists() {
            return Ok((name, path));
        }
    }
    bail!("could not find a free name for {base}")
}

/// Every worktree Houston has created, with its current state.
///
/// Reads the directory rather than asking git, because a worktree whose
/// repository has been deleted still occupies disk and still needs removing.
pub fn list() -> Result<Vec<Worktree>> {
    list_in(&root()?)
}

/// Every worktree under an explicit root. See `create_in` for why this is a
/// parameter.
pub fn list_in(root: &Path) -> Result<Vec<Worktree>> {
    let mut worktrees = Vec::new();

    for entry in std::fs::read_dir(root)?.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        let repository = main_repository(&path).unwrap_or_default();
        let branch = git(&path, &["rev-parse", "--abbrev-ref", "HEAD"])
            .ok()
            .map(|output| output.trim().to_string());

        let dirty =
            git(&path, &["status", "--porcelain"]).is_ok_and(|output| !output.trim().is_empty());
        // One command for both numbers. Asking twice would double the process
        // count for a row that already costs three.
        let (ahead, behind) =
            git(&path, &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
                .map_or((0, 0), |output| parse_ahead_behind(&output));

        // Two more subprocesses per worktree. Affordable because this list is
        // loaded when you open the view or press `r`, never per frame.
        let changes = crate::diff::changes_in(&path);

        worktrees.push(Worktree { name, path, repository, branch, dirty, ahead, behind, changes });
    }

    worktrees.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(worktrees)
}

/// Reads `git rev-list --left-right --count`, which is two tab-separated
/// numbers: what the left side has that the right does not, then the reverse.
///
/// `(0, 0)` for anything unparseable. A worktree with no upstream makes git
/// exit non-zero here, and that is the common case, not an error worth
/// surfacing.
#[must_use]
pub fn parse_ahead_behind(output: &str) -> (usize, usize) {
    let mut columns = output.split_whitespace();
    let ahead = columns.next().and_then(|value| value.parse().ok()).unwrap_or(0);
    let behind = columns.next().and_then(|value| value.parse().ok()).unwrap_or(0);
    (ahead, behind)
}

/// Removes a worktree.
///
/// Refuses while it has uncommitted changes unless `force`. Deleting work
/// someone has not committed is the one unrecoverable thing this module can
/// do, so it takes a deliberate second act.
pub fn remove(worktree: &Worktree, force: bool) -> Result<()> {
    if worktree.dirty && !force {
        bail!("{} has uncommitted changes — press D to remove it anyway", worktree.name);
    }

    // Ask git first so its bookkeeping is updated. If the repository is gone,
    // fall through to removing the directory ourselves.
    let removed = if worktree.repository.as_os_str().is_empty() {
        false
    } else {
        let mut arguments = vec!["worktree", "remove"];
        if force {
            arguments.push("--force");
        }
        let path = worktree.path.to_string_lossy().into_owned();
        arguments.push(&path);
        git(&worktree.repository, &arguments).is_ok()
    };

    if !removed && worktree.path.exists() {
        std::fs::remove_dir_all(&worktree.path)
            .with_context(|| format!("could not remove {}", worktree.path.display()))?;
    }
    Ok(())
}

/// What landing a worktree should do, in order.
///
/// Split from doing it so the decisions can be tested without pushing
/// anything to anybody. The same reason `session::restore_spec` exists: the
/// interesting part is what gets chosen, and that part should not require a
/// network, a remote, or somebody's repository to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Commit(String),
    Push,
    PullRequest,
    Remove,
}

/// What the user asked for on the landing form.
#[derive(Debug, Clone, Default)]
pub struct Land {
    pub message: String,
    pub push: bool,
    pub pull_request: bool,
    pub remove: bool,
}

/// Turns a request into an ordered plan, or explains why it cannot.
pub fn plan(worktree: &Worktree, request: &Land) -> Result<Vec<Step>> {
    let mut steps = Vec::new();

    if worktree.dirty {
        let message = request.message.trim();
        if message.is_empty() {
            bail!("a commit needs a message");
        }
        steps.push(Step::Commit(message.to_string()));
    }

    // A pull request needs a branch the remote can see, so asking for one is
    // asking for a push. Silently skipping the push and then failing on the
    // PR would be a worse way to learn that.
    if request.push || request.pull_request {
        steps.push(Step::Push);
    }
    if request.pull_request {
        steps.push(Step::PullRequest);
    }
    if request.remove {
        steps.push(Step::Remove);
    }

    if steps.is_empty() {
        bail!("nothing to do — {} is already clean", worktree.name);
    }
    Ok(steps)
}

/// Carries out a plan, saying what happened.
///
/// Stops at the first failure and reports how far it got. A half-landed
/// worktree is a normal outcome — a push can be rejected, `gh` may not be
/// installed — and the recovery is always to look at what did happen and
/// press the key again.
pub fn land(worktree: &Worktree, steps: &[Step]) -> Result<String> {
    let branch = worktree.branch.clone().unwrap_or_else(|| "HEAD".to_string());
    let mut done: Vec<String> = Vec::new();

    for step in steps {
        let outcome = match step {
            Step::Commit(message) => {
                git(&worktree.path, &["add", "-A"])?;
                git(&worktree.path, &["commit", "-m", message])?;
                "committed".to_string()
            }
            Step::Push => {
                git(&worktree.path, &["push", "-u", "origin", &branch]).with_context(|| {
                    format!("could not push {branch} to origin (landed: {})", summarise(&done))
                })?;
                format!("pushed to origin/{branch}")
            }
            Step::PullRequest => open_pull_request(&worktree.path, &done)?,
            Step::Remove => {
                // Re-read, because the commit above has just made it clean and
                // the copy we were handed still says otherwise.
                let mut fresh = worktree.clone();
                fresh.dirty = false;
                remove(&fresh, false)?;
                "removed the worktree".to_string()
            }
        };
        done.push(outcome);
    }
    Ok(summarise(&done))
}

/// Opens a PR with `gh`, which is the only tool that can.
fn open_pull_request(path: &Path, done: &[String]) -> Result<String> {
    let output = Command::new("gh")
        .args(["pr", "create", "--fill"])
        .current_dir(path)
        .output()
        .context("could not run gh — install the GitHub CLI, or untick the pull request")?;

    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr);
        bail!("gh pr create: {} (landed: {})", reason.trim(), summarise(done));
    }

    // `gh` prints the URL it made, which is the one thing worth repeating.
    let url = String::from_utf8_lossy(&output.stdout);
    let url = url.lines().rev().find(|line| line.starts_with("http")).unwrap_or("").trim();

    Ok(if url.is_empty() { "opened a pull request".to_string() } else { format!("PR {url}") })
}

fn summarise(done: &[String]) -> String {
    if done.is_empty() { "nothing".to_string() } else { done.join(", ") }
}

/// Runs git in a directory, returning stdout.
fn git(directory: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .output()
        .context("could not run git — is it installed?")?;

    if !output.status.success() {
        bail!("git {}: {}", arguments.join(" "), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway worktree root, so tests never touch `~/.houston`.
    fn scratch_root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("houston-wt-root-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// A throwaway repository with one commit.
    fn scratch_repository(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("houston-wt-repo-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();

        git(&path, &["init", "-q", "-b", "main"]).unwrap();
        git(&path, &["config", "user.email", "test@example.com"]).unwrap();
        git(&path, &["config", "user.name", "Test"]).unwrap();
        std::fs::write(path.join("README.md"), "# test\n").unwrap();
        git(&path, &["add", "."]).unwrap();
        git(&path, &["commit", "-q", "-m", "first"]).unwrap();
        path
    }

    fn dirty(name: &str) -> Worktree {
        Worktree {
            name: name.to_string(),
            path: PathBuf::from("/tmp/nowhere"),
            repository: PathBuf::from("/tmp/repo"),
            branch: Some("feature".to_string()),
            dirty: true,
            ahead: 1,
            behind: 0,
            changes: None,
        }
    }

    #[test]
    fn tracking_counts_are_read_from_one_command_and_default_to_level() {
        assert_eq!(parse_ahead_behind("3\t1\n"), (3, 1), "left is ahead, right is behind");
        assert_eq!(parse_ahead_behind("0\t0\n"), (0, 0));
        assert_eq!(
            parse_ahead_behind(""),
            (0, 0),
            "no upstream is the normal state of an unpushed worktree, not an error"
        );
        assert_eq!(parse_ahead_behind("nonsense"), (0, 0));
    }

    #[test]
    fn landing_uncommitted_work_without_a_message_is_refused_rather_than_guessed() {
        let request = Land { push: true, ..Land::default() };

        let refusal = plan(&dirty("wt"), &request).unwrap_err().to_string();
        assert!(refusal.contains("message"), "the reason has to say what is missing: {refusal}");
    }

    #[test]
    fn a_clean_worktree_is_not_committed_again() {
        let mut clean = dirty("wt");
        clean.dirty = false;

        let steps = plan(&clean, &Land { push: true, ..Land::default() }).unwrap();
        assert_eq!(steps, vec![Step::Push], "there is nothing to commit, so nothing is");
    }

    #[test]
    fn asking_for_a_pull_request_pushes_first_even_if_you_did_not_tick_push() {
        let request =
            Land { message: "done".to_string(), pull_request: true, push: false, remove: false };

        let steps = plan(&dirty("wt"), &request).unwrap();
        assert_eq!(
            steps,
            vec![Step::Commit("done".to_string()), Step::Push, Step::PullRequest],
            "a PR needs a branch the remote can see; failing at the PR would teach that badly"
        );
    }

    #[test]
    fn removal_comes_last_so_nothing_is_deleted_before_it_is_safe() {
        let request =
            Land { message: "done".to_string(), push: true, pull_request: false, remove: true };

        let steps = plan(&dirty("wt"), &request).unwrap();
        assert_eq!(steps.last(), Some(&Step::Remove), "the tree goes only after the work is out");
    }

    #[test]
    fn a_request_that_would_do_nothing_says_so() {
        let mut clean = dirty("spare");
        clean.dirty = false;

        let refusal = plan(&clean, &Land::default()).unwrap_err().to_string();
        assert!(refusal.contains("clean"), "silence would look like it worked: {refusal}");
    }

    /// The whole plan, carried out against a real repository with a real
    /// remote — but a local one, so nothing leaves the machine.
    #[test]
    fn landing_commits_and_pushes_to_a_real_remote() {
        let remote = std::env::temp_dir().join("houston-wt-remote.git");
        let _ = std::fs::remove_dir_all(&remote);
        std::fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "-q", "--bare", "-b", "main"]).unwrap();

        let repository = scratch_repository("landing");
        git(&repository, &["remote", "add", "origin", remote.to_str().unwrap()]).unwrap();

        let root = scratch_root("landing");
        let mut worktree = create_in(&root, &repository, "ship it").unwrap();
        std::fs::write(worktree.path.join("work.txt"), "agent output\n").unwrap();
        worktree.dirty = true;

        let steps = plan(
            &worktree,
            &Land { message: "the agent's work".to_string(), push: true, ..Land::default() },
        )
        .unwrap();
        let report = land(&worktree, &steps).unwrap();

        assert!(report.contains("committed"), "it says what it did: {report}");
        assert!(report.contains("pushed"), "including the push: {report}");

        let branches = git(&remote, &["branch", "--list"]).unwrap();
        assert!(branches.contains("ship-it"), "the branch really reached the remote: {branches}");

        remove(&worktree, true).ok();
        for path in [&root, &repository, &remote] {
            std::fs::remove_dir_all(path).ok();
        }
    }

    #[test]
    fn a_worktree_reports_the_repository_it_came_from_not_itself() {
        let repository = scratch_repository("mainrepo");
        let root = scratch_root("mainrepo");
        let worktree = create_in(&root, &repository, "from here").unwrap();

        let found = main_repository(&worktree.path).unwrap();
        assert_eq!(found, repository.canonicalize().unwrap());
        assert_ne!(found, worktree.path, "the worktree is not its own repository");

        remove(&worktree, true).ok();
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&repository).ok();
    }

    #[test]
    fn the_branch_is_read_from_the_file_rather_than_by_running_git() {
        let repository = scratch_repository("branch");
        assert_eq!(branch_of(&repository).as_deref(), Some("main"));

        // A subdirectory finds it by walking up.
        let nested = repository.join("deep/inside");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(branch_of(&nested).as_deref(), Some("main"));

        assert!(branch_of(Path::new("/")).is_none(), "not a repository");

        std::fs::remove_dir_all(&repository).ok();
    }

    /// A worktree's `.git` is a file pointing elsewhere, not a directory.
    #[test]
    fn a_worktrees_branch_is_found_through_the_gitdir_indirection() {
        let repository = scratch_repository("branchwt");
        let root = scratch_root("branchwt");
        let worktree = create_in(&root, &repository, "feature work").unwrap();

        assert!(worktree.path.join(".git").is_file(), "the premise of this test");
        assert_eq!(branch_of(&worktree.path).as_deref(), Some("feature-work"));

        remove(&worktree, true).ok();
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&repository).ok();
    }

    #[test]
    fn a_directory_is_recognised_as_a_repository_or_not() {
        let repository = scratch_repository("detect");
        assert!(is_repository(&repository));
        assert!(!is_repository(Path::new("/")), "/ is not a repository");

        std::fs::remove_dir_all(&repository).ok();
    }

    #[test]
    fn creating_a_worktree_puts_it_under_houstons_own_directory() {
        let repository = scratch_repository("create");

        let root = scratch_root("create");
        let worktree = create_in(&root, &repository, "Auth Refactor").unwrap();
        assert_eq!(worktree.name, "auth-refactor", "named from a slug of the session");
        assert!(worktree.path.starts_with(&root), "never inside the user's project");
        assert!(worktree.path.join("README.md").exists(), "the checkout is real");
        assert_eq!(worktree.branch.as_deref(), Some("auth-refactor"));

        remove(&worktree, true).ok();
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&repository).ok();
    }

    #[test]
    fn a_name_collision_gets_a_suffix_rather_than_an_error() {
        let repository = scratch_repository("collide");
        let root = scratch_root("collide");

        let first = create_in(&root, &repository, "same name").unwrap();
        let second = create_in(&root, &repository, "same name").unwrap();

        assert_eq!(first.name, "same-name");
        assert_eq!(second.name, "same-name-2", "being blocked mid-task is worse than a suffix");

        remove(&first, true).ok();
        remove(&second, true).ok();
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&repository).ok();
    }

    #[test]
    fn creating_in_a_plain_directory_is_refused() {
        let plain = std::env::temp_dir().join("houston-wt-not-a-repo");
        std::fs::create_dir_all(&plain).unwrap();
        let root = scratch_root("plain");

        assert!(create_in(&root, &plain, "nope").is_err());
        std::fs::remove_dir_all(&root).ok();

        std::fs::remove_dir_all(&plain).ok();
    }

    #[test]
    fn a_dirty_worktree_is_not_removed_without_being_told_twice() {
        let repository = scratch_repository("dirty");
        let root = scratch_root("dirty");
        let worktree = create_in(&root, &repository, "dirty work").unwrap();

        std::fs::write(worktree.path.join("scratch.txt"), "uncommitted").unwrap();
        let listed = list_in(&root).unwrap().into_iter().find(|w| w.name == worktree.name).unwrap();
        assert!(listed.dirty, "an uncommitted file makes it dirty");

        assert!(remove(&listed, false).is_err(), "unforced removal must refuse");
        assert!(listed.path.exists(), "and must not have deleted anything");

        remove(&listed, true).unwrap();
        assert!(!listed.path.exists());

        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&repository).ok();
    }

    #[test]
    fn listing_reports_a_clean_worktree_as_clean() {
        let repository = scratch_repository("listing");
        let root = scratch_root("listing");
        let worktree = create_in(&root, &repository, "listed one").unwrap();

        let listed = list_in(&root).unwrap().into_iter().find(|w| w.name == worktree.name).unwrap();
        assert!(!listed.dirty);
        assert_eq!(listed.branch.as_deref(), Some(worktree.name.as_str()));
        assert_eq!(
            listed.repository,
            repository_root(&repository).unwrap().canonicalize().unwrap(),
            "a worktree points back at the repository it came from, not at itself"
        );

        remove(&listed, true).ok();
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&repository).ok();
    }
}
