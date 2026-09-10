//! Where a child process actually is, as opposed to where it was started.
//!
//! A shell session is launched in some directory and then you `cd` out of it,
//! which is the entire point of a shell. Houston remembered the launch
//! directory, so quitting and reopening put you back at the start of the walk
//! rather than where you had got to.
//!
//! There is no portable way to ask. Linux publishes it as a symlink under
//! `/proc`, which is free to read. macOS has no such interface short of
//! `libproc`, which means FFI, which `unsafe_code = "forbid"` rules out — so
//! `lsof` answers it instead, at the cost of a subprocess.
//!
//! That cost is why this is called on a timer and never per frame.

use std::path::{Path, PathBuf};

/// The working directory of a running process.
///
/// `None` when it cannot be determined — the process has exited, the platform
/// has no way to ask, or the answer is not a directory any more. Callers keep
/// whatever they had rather than treating "do not know" as "moved".
#[must_use]
pub fn of(pid: u32) -> Option<PathBuf> {
    let found = if cfg!(target_os = "linux") { from_proc(pid) } else { from_lsof(pid) };

    // A directory that has been deleted underneath the shell is still its cwd
    // as far as the kernel is concerned, and restoring into it would fail.
    found.filter(|path| path.is_dir())
}

/// Linux: a symlink, so this is a readlink and nothing else.
fn from_proc(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(Path::new("/proc").join(pid.to_string()).join("cwd")).ok()
}

/// macOS: `lsof`, in its machine-readable mode.
///
/// `-Fn` prints one field per line, each tagged with a leading character, so
/// the answer is the line starting with `n` — no column counting, and no
/// trouble with paths containing spaces.
fn from_lsof(pid: u32) -> Option<PathBuf> {
    let output = std::process::Command::new("lsof")
        .args(["-a", "-d", "cwd", "-Fn", "-p"])
        .arg(pid.to_string())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix('n'))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one case worth asserting on a machine we do not control: our own
    /// process, whose working directory the test already knows.
    #[test]
    fn a_running_process_reports_the_directory_it_is_in() {
        let mine = std::process::id();
        let expected = std::env::current_dir().expect("the test has a working directory");

        let found = of(mine).expect("we can read our own working directory");

        // Both sides canonicalised: macOS hands back `/private/var/...` where
        // `current_dir` says `/var/...`, and they are the same directory.
        assert_eq!(
            found.canonicalize().ok(),
            expected.canonicalize().ok(),
            "reading our own cwd should agree with the standard library"
        );
    }

    #[test]
    fn a_pid_that_is_not_running_reports_nothing_rather_than_guessing() {
        // Deliberately absurd: pids are bounded well below this on every
        // platform Houston targets.
        assert!(of(u32::MAX - 1).is_none(), "no process, no answer");
    }
}
