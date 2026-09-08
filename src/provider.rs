//! Which coding agents are available to launch.
//!
//! ADR-0006: a provider is just a named `LaunchSpec` template, which is why
//! "run a plain shell" needs no special case — it is a provider whose command
//! is your login shell.

use crate::pty::LaunchSpec;
use std::path::{Path, PathBuf};

/// An agent CLI Houston knows how to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider {
    /// Shown in the UI.
    pub label: &'static str,
    /// The executable to look for on `PATH`.
    pub command: &'static str,
}

/// Ordered by preference: the first one found on `PATH` becomes the default.
pub const KNOWN: [Provider; 4] = [
    Provider { label: "Claude Code", command: "claude" },
    Provider { label: "Codex", command: "codex" },
    Provider { label: "Gemini", command: "gemini" },
    Provider { label: "opencode", command: "opencode" },
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

/// The user's login shell, falling back to `/bin/sh`.
#[must_use]
pub fn login_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
