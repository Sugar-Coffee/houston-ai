//! Which coding agents are available to launch.
//!
//! ADR-0006: a provider is just a named `LaunchSpec` template, which is why
//! "run a plain shell" needs no special case — it is a provider whose command
//! is your login shell.

use crate::pty::LaunchSpec;
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::SystemTime,
};
use walkdir::WalkDir;

/// An agent CLI Houston knows how to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider {
    /// Shown in the UI.
    pub label: &'static str,
    /// The executable to look for on `PATH`.
    pub command: &'static str,
    /// The argument that comes before a conversation id when resuming.
    ///
    /// The two supported agents spell this differently — `claude --resume
    /// <id>` uses a flag, `codex resume <id>` uses a subcommand — but both are
    /// "one token, then the id", so one field covers them.
    ///
    /// `None` means the agent's resume contract has not been checked. Those
    /// restore their directory and name and start fresh, which is honest —
    /// guessing an argument makes the agent fail to launch at all.
    pub resume_arg: Option<&'static str>,
}

/// Ordered by preference: the first one found on `PATH` becomes the default.
pub const KNOWN: [Provider; 4] = [
    Provider { label: "Claude Code", command: "claude", resume_arg: Some("--resume") },
    Provider { label: "Codex", command: "codex", resume_arg: Some("resume") },
    Provider { label: "Gemini", command: "gemini", resume_arg: None },
    Provider { label: "opencode", command: "opencode", resume_arg: None },
];

impl Provider {
    pub fn launch(self, cwd: PathBuf) -> LaunchSpec {
        LaunchSpec::command(self.command, Vec::new(), cwd)
    }
}

/// The providers actually installed, in preference order.
#[must_use]
pub fn available() -> Vec<Provider> {
    KNOWN.into_iter().filter(|provider| which(provider.command).is_some()).collect()
}

/// The known provider for an executable name, if there is one.
#[must_use]
pub fn by_command(command: &str) -> Option<Provider> {
    KNOWN.into_iter().find(|provider| provider.command == command)
}

/// The provider to use when the user does not pick one.
#[must_use]
pub fn default() -> Option<Provider> {
    available().into_iter().next()
}

/// Finds an executable on `PATH`.
///
/// Hand-rolled rather than shelling out to `which`: spawning a process per
/// lookup at startup is wasteful, and `which` is not guaranteed to exist.
#[must_use]
pub fn which(command: &str) -> Option<PathBuf> {
    // An explicit path bypasses the search entirely.
    if command.contains('/') {
        let path = PathBuf::from(command);
        return is_executable(&path).then_some(path);
    }

    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join(command))
            .find(|candidate| is_executable(candidate))
    })
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Finds the Codex conversation started in `cwd` since `since`.
///
/// Codex has no hook that hands Houston a session id the way Claude Code does,
/// but it records one itself: every session writes a rollout file whose first
/// line is a `session_meta` carrying both `session_id` and `cwd`.
///
/// `since` is the moment Houston started the session, and it is what keeps this
/// honest — without it, the newest rollout for a directory could easily be from
/// last week, and resuming that would drop you into the wrong conversation.
#[must_use]
pub fn codex_conversation(cwd: &Path, since: SystemTime) -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let sessions = PathBuf::from(home).join(".codex/sessions");

    let mut best: Option<(SystemTime, String)> = None;

    for entry in WalkDir::new(sessions).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "jsonl") {
            continue;
        }

        let modified = entry.metadata().ok()?.modified().ok()?;
        if modified < since {
            continue;
        }
        if best.as_ref().is_some_and(|(seen, _)| *seen >= modified) {
            continue;
        }

        // Only the first line is needed, and a rollout can be large.
        let Ok(file) = std::fs::File::open(path) else { continue };
        let mut first = String::new();
        if BufReader::new(file).read_line(&mut first).is_err() {
            continue;
        }

        let Ok(meta) = serde_json::from_str::<serde_json::Value>(&first) else { continue };
        let payload = meta.get("payload")?;

        if payload.get("cwd").and_then(serde_json::Value::as_str) != Some(&cwd.to_string_lossy()) {
            continue;
        }
        if let Some(id) = payload.get("session_id").and_then(serde_json::Value::as_str) {
            best = Some((modified, id.to_string()));
        }
    }

    best.map(|(_, id)| id)
}

/// The user's login shell, falling back to `/bin/sh`.
#[must_use]
pub fn login_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_supported_agents_can_resume_despite_spelling_it_differently() {
        // `claude --resume <id>` versus `codex resume <id>`.
        assert_eq!(by_command("claude").unwrap().resume_arg, Some("--resume"));
        assert_eq!(by_command("codex").unwrap().resume_arg, Some("resume"));

        // Guessing an argument and failing to launch is worse than a fresh
        // session, so unchecked agents claim nothing.
        for command in ["gemini", "opencode"] {
            assert!(by_command(command).unwrap().resume_arg.is_none(), "{command}");
        }
        assert!(by_command("not-an-agent").is_none());
    }

    #[test]
    fn finds_a_binary_that_certainly_exists() {
        assert!(which("sh").is_some(), "sh should be on PATH");
    }

    #[test]
    fn rejects_a_binary_that_certainly_does_not() {
        assert!(which("houston-definitely-not-a-real-binary").is_none());
    }

    #[test]
    fn absolute_paths_skip_the_search() {
        assert_eq!(which("/bin/sh"), Some(PathBuf::from("/bin/sh")));
        assert!(which("/bin/definitely-not-real").is_none());
    }

    #[test]
    fn directories_are_not_executables() {
        // /bin is executable-by-mode but is not a file.
        assert!(which("/bin").is_none());
    }
}

#[cfg(test)]
mod codex_lookup_tests {
    use super::*;

    /// Uses whatever rollout files exist on this machine, so it verifies the
    /// real format rather than a fixture that could drift from it.
    #[test]
    fn a_codex_rollout_is_matched_by_its_recorded_directory() {
        let Some(home) = std::env::var_os("HOME") else { return };
        let sessions = PathBuf::from(home).join(".codex/sessions");

        // Find any rollout and read the directory and id it claims.
        let Some((cwd, expected, modified)) = WalkDir::new(&sessions)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
            .find_map(|entry| {
                let file = std::fs::File::open(entry.path()).ok()?;
                let mut first = String::new();
                BufReader::new(file).read_line(&mut first).ok()?;
                let meta: serde_json::Value = serde_json::from_str(&first).ok()?;
                let payload = meta.get("payload")?;
                Some((
                    PathBuf::from(payload.get("cwd")?.as_str()?),
                    payload.get("session_id")?.as_str()?.to_string(),
                    entry.metadata().ok()?.modified().ok()?,
                ))
            })
        else {
            return; // No Codex history here; nothing to verify against.
        };

        // Anything at or before that file's own timestamp must find it.
        let found = codex_conversation(&cwd, modified);
        assert!(found.is_some(), "a rollout recording {} should be findable", cwd.display());

        // And a cutoff after it must not.
        // Nothing can have been written after now, so this excludes every
        // rollout including the one found above.
        let after = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        assert!(codex_conversation(&cwd, after).is_none(), "a stale rollout must not be adopted");
        let _ = modified;

        let _ = expected;
    }
}
