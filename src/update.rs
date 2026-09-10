//! Noticing that a newer Houston exists, and fetching it when asked.
//!
//! **Nothing here downloads anything on its own.** The check asks GitHub which
//! tag is latest, at most once a day, on a thread that nobody waits for. What
//! it does with the answer is put a word in the tab strip. Replacing the
//! binary happens only when you run `houston update`.
//!
//! That split is deliberate. A workspace holding half a dozen live agent
//! sessions is the wrong place for a surprise, and an app that rewrites its
//! own executable while you are working is a surprise however well it goes.

use anyhow::{Context, Result, bail};
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const REPO: &str = "Sugar-Coffee/houston-ai";

/// How long an answer stays good for.
///
/// A day. Releases do not appear hourly, and a check on every launch would
/// mean a network round trip in the way of a workspace that is otherwise
/// entirely local.
const CACHE_FOR: Duration = Duration::from_hours(24);

/// The version this binary was built as.
#[must_use]
pub const fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Splits a version into numbers, ignoring anything after them.
///
/// `v0.2.0`, `0.2.0` and `0.2.0-rc1` all read as `(0, 2, 0)`. A pre-release
/// suffix is dropped rather than ordered: getting that right needs the whole
/// semver precedence table, and the only question here is "is there something
/// newer", which the numbers answer.
#[must_use]
pub fn parse(version: &str) -> Option<(u32, u32, u32)> {
    let version = version.trim().trim_start_matches('v');
    let core = version.split(['-', '+']).next()?;

    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// Whether `latest` is worth telling somebody about.
///
/// Unparseable input is never newer. A tag nobody can read is not a reason to
/// nag, and treating it as one would turn a typo in a release name into a
/// permanent banner.
#[must_use]
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse(current), parse(latest)) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    }
}

/// Where the last answer is kept.
fn cache_path() -> Result<PathBuf> {
    Ok(crate::hooks::state_dir()?.join("update.json"))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// Reads the cached answer, if it is still fresh.
fn cached() -> Option<String> {
    let text = std::fs::read_to_string(cache_path().ok()?).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;

    let checked = value.get("checked_at")?.as_u64()?;
    if now().saturating_sub(checked) > CACHE_FOR.as_secs() {
        return None;
    }
    Some(value.get("latest")?.as_str()?.to_string())
}

fn remember(latest: &str) {
    let Ok(path) = cache_path() else { return };
    let body = serde_json::json!({ "checked_at": now(), "latest": latest });
    let _ = std::fs::write(path, body.to_string());
}

/// Asks GitHub for the latest tag.
///
/// Reads the redirect on `/releases/latest` rather than the API. It needs no
/// token, is not rate-limited the way the API is, and returns a tag name
/// without a JSON parse — the same trick `install.sh` uses, for the same
/// reasons.
fn ask_github() -> Result<Option<String>> {
    // Deliberately no `-f`. With it, a repository that has no releases 404s
    // and curl exits non-zero — which is indistinguishable from the network
    // being down, and those two want completely different advice.
    let output = std::process::Command::new("curl")
        .args(["-sSLI", "-o", "/dev/null", "-w", "%{url_effective}", "--max-time", "10"])
        .arg(format!("https://github.com/{REPO}/releases/latest"))
        .output()
        .context("could not run curl — it is what this uses to reach GitHub")?;

    if !output.status.success() {
        bail!("could not reach GitHub — check your connection");
    }

    // A repository with no releases does not redirect at all, so the tag has
    // to be matched rather than trimmed off the end — and the two outcomes are
    // worth telling apart, because one is your network and the other is not.
    let url = String::from_utf8_lossy(&output.stdout);
    Ok(url
        .trim()
        .rsplit_once("/releases/tag/")
        .map(|(_, tag)| tag.to_string())
        .filter(|tag| !tag.is_empty()))
}

/// The latest version, from the cache or from the network.
///
/// `None` means "no answer", never "you are up to date" — a failed lookup and
/// a confirmed match are different things, and only one of them is worth
/// remembering.
#[must_use]
pub fn latest() -> Option<String> {
    if let Some(cached) = cached() {
        return Some(cached);
    }
    let latest = ask_github().ok().flatten()?;
    remember(&latest);
    Some(latest)
}

/// The check, as the app runs it: returns a version only if it is newer.
#[must_use]
pub fn available() -> Option<String> {
    let latest = latest()?;
    is_newer(current(), &latest).then_some(latest)
}

/// Installs the latest release over this binary.
///
/// Runs the same `install.sh` anybody else would, pointed at the directory
/// this executable is actually in — so an install that went to `~/.cargo/bin`
/// is updated there rather than being shadowed by a second copy somewhere
/// else on the `PATH`.
pub fn run() -> Result<()> {
    let exe = std::env::current_exe().context("could not work out where this binary lives")?;
    let dir = exe.parent().context("this binary has no directory")?;

    let latest = if let Some(cached) = cached() {
        cached
    } else if let Some(tag) = ask_github()? {
        remember(&tag);
        tag
    } else {
        // Reached GitHub and it said there are none. Not a failure, and not
        // something a retry will fix.
        println!("  No releases published yet.\n");
        return Ok(());
    };

    println!("  houston {} → {latest}", current());
    if !is_newer(current(), &latest) {
        println!("  Already up to date.\n");
        return Ok(());
    }

    let script = format!("https://raw.githubusercontent.com/{REPO}/main/install.sh");
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("curl -fsSL {script} | HOUSTON_INSTALL_DIR={} sh", dir.display()))
        .status()
        .context("could not run the installer — is curl available?")?;

    if !status.success() {
        bail!("the installer did not finish; nothing was replaced");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_with_or_without_the_leading_v() {
        assert_eq!(parse("0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse("v0.2.0"), Some((0, 2, 0)), "release tags carry a v");
        assert_eq!(parse("  v1.10.3  "), Some((1, 10, 3)));
    }

    #[test]
    fn a_short_version_fills_in_the_zeroes() {
        assert_eq!(parse("v1"), Some((1, 0, 0)));
        assert_eq!(parse("v1.2"), Some((1, 2, 0)));
    }

    #[test]
    fn a_pre_release_suffix_is_dropped_rather_than_ordered() {
        assert_eq!(parse("v0.2.0-rc1"), Some((0, 2, 0)));
        assert_eq!(parse("v0.2.0+build7"), Some((0, 2, 0)));
    }

    #[test]
    fn numbers_are_compared_as_numbers_not_as_text() {
        assert!(is_newer("0.9.0", "0.10.0"), "0.10 sorts before 0.9 as a string and must not here");
        assert!(is_newer("1.0.0", "1.0.1"));
        assert!(!is_newer("1.0.1", "1.0.0"), "older is not newer");
        assert!(!is_newer("1.0.0", "1.0.0"), "the same version is not an update");
    }

    /// A tag nobody can read is not a reason to nag forever.
    #[test]
    fn an_unreadable_version_is_never_newer() {
        assert!(!is_newer("0.1.0", "nightly"));
        assert!(!is_newer("0.1.0", ""));
        assert!(!is_newer("whatever", "0.9.0"), "not knowing what we are is not an update either");
        assert_eq!(parse("nightly"), None);
    }

    #[test]
    fn the_version_reported_is_the_one_this_was_built_as() {
        assert_eq!(current(), env!("CARGO_PKG_VERSION"));
        assert!(parse(current()).is_some(), "our own version has to be readable");
    }
}
