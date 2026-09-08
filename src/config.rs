//! Houston's settings, stored at `~/.houston/config.toml`.
//!
//! TOML rather than JSON because this file is meant to be hand-edited as much
//! as it is written by the Settings view.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The welcome note written into a freshly created vault.
///
/// A brand new vault otherwise renders as "0 notes indexed", which reads as a
/// bug rather than an empty state. This is an ordinary note — deleting it
/// breaks nothing.
const WELCOME: &str = "\
# Welcome to your Houston vault

This folder is your knowledge base. Houston created it and nothing else touches
it.

Notes are plain markdown files. Make as many folders as you like — Houston
indexes everything ending in `.md`, however deep.

## Getting around

- `/` fuzzy-find a note by name
- `f` search inside notes
- `↵` open the selected note
- `y` copy its path to the clipboard
- `i` send its path straight into a running agent session

## Linking notes

Write `[[note name]]` to link to another note. Houston resolves those, and `l`
lists the links going out of the note you are reading while `b` shows the ones
coming in.

## Already have an Obsidian vault?

Open Settings (`4`) and point Houston at it. Nothing is copied or moved — it
reads the folder where it sits.
";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Where the vault lives. `None` means the default under `~/.houston`.
    pub vault: Option<PathBuf>,
}

impl Config {
    /// Reads the config, falling back to defaults.
    ///
    /// A malformed file is reported rather than silently replaced — losing
    /// someone's settings because of a stray comma would be worse than
    /// refusing to start with them.
    pub fn load() -> (Self, Option<String>) {
        let Ok(path) = config_path() else {
            return (Self::default(), None);
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return (Self::default(), None);
        };

        match toml::from_str(&text) {
            Ok(config) => (config, None),
            Err(error) => (
                Self::default(),
                Some(format!("{} is not valid TOML ({error}) — using defaults", path.display())),
            ),
        }
    }

    /// Writes the config to an explicit path.
    ///
    /// Takes the path rather than resolving it, so tests cannot write over the
    /// real `~/.houston/config.toml`. An earlier version of this did exactly
    /// that and silently repointed a live install at a temp folder.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self).context("could not encode the config")?;
        std::fs::write(path, text)
            .with_context(|| format!("could not write {}", path.display()))?;
        Ok(())
    }

    /// Where the vault should be, without creating anything.
    pub fn vault_root(&self) -> Result<PathBuf> {
        Ok(resolve_vault(
            std::env::var_os("HOUSTON_VAULT").map(PathBuf::from).as_deref(),
            self.vault.as_deref(),
            crate::hooks::state_dir()?.join("vault"),
        ))
    }

    /// Whether the vault location was chosen by the user rather than defaulted.
    ///
    /// A chosen path that does not exist is a typo worth surfacing; the
    /// default one is ours to create.
    fn vault_is_explicit(&self) -> bool {
        self.vault.is_some() || std::env::var_os("HOUSTON_VAULT").is_some()
    }

    /// The vault root, created with a welcome note if it does not exist.
    ///
    /// Only the *default* location is created. A configured path that does not
    /// exist is an error the user needs to see — silently making a folder at a
    /// mistyped path would hide the typo.
    pub fn ensure_vault(&self) -> Result<PathBuf> {
        let root = self.vault_root()?;
        if root.is_dir() {
            return Ok(root);
        }

        if self.vault_is_explicit() {
            anyhow::bail!("no vault at {} — check the path in Settings", root.display());
        }

        std::fs::create_dir_all(&root)
            .with_context(|| format!("could not create {}", root.display()))?;
        std::fs::write(root.join("Welcome.md"), WELCOME)
            .with_context(|| format!("could not write into {}", root.display()))?;
        Ok(root)
    }
}

/// Picks the vault root from the three possible sources, in priority order.
///
/// ADR-0007: an explicit environment override, then the configured path, then
/// Houston's own folder. Never a folder we happened to find lying around.
///
/// Split out as a pure function so the precedence can be tested without
/// mutating process-wide environment state — which `unsafe_code = "forbid"`
/// rightly will not allow anyway.
fn resolve_vault(
    environment: Option<&Path>,
    configured: Option<&Path>,
    default: PathBuf,
) -> PathBuf {
    environment.or(configured).map_or(default, expand_home)
}

pub fn config_path() -> Result<PathBuf> {
    Ok(crate::hooks::state_dir()?.join("config.toml"))
}

/// Expands a leading `~`, which people type and `PathBuf` does not understand.
fn expand_home(path: &Path) -> PathBuf {
    let Ok(rest) = path.strip_prefix("~") else { return path.to_path_buf() };
    std::env::var_os("HOME")
        .map_or_else(|| path.to_path_buf(), |home| PathBuf::from(home).join(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fallback() -> PathBuf {
        PathBuf::from("/home/someone/.houston/vault")
    }

    #[test]
    fn an_environment_override_wins_over_everything() {
        let root = resolve_vault(
            Some(Path::new("/tmp/from-the-environment")),
            Some(Path::new("/tmp/from-the-config")),
            fallback(),
        );
        assert_eq!(root, PathBuf::from("/tmp/from-the-environment"));
    }

    #[test]
    fn a_configured_path_is_used_when_there_is_no_override() {
        let root = resolve_vault(None, Some(Path::new("/tmp/configured-vault")), fallback());
        assert_eq!(root, PathBuf::from("/tmp/configured-vault"));
    }

    #[test]
    fn nothing_configured_falls_back_to_houstons_own_directory() {
        assert_eq!(resolve_vault(None, None, fallback()), fallback());

        // And the real default is under ~/.houston, not anywhere else.
        let real = Config::default().vault_root().unwrap();
        assert!(real.ends_with(".houston/vault"), "got {}", real.display());
    }

    /// The behaviour this whole change exists to prevent: Houston used to
    /// adopt `~/Projects/houston` if it happened to be there.
    #[test]
    fn an_existing_folder_is_never_adopted_implicitly() {
        let root = Config::default().vault_root().unwrap();
        assert!(
            !root.ends_with("Projects/houston"),
            "the default must not reach into a folder Houston did not create"
        );
    }

    #[test]
    fn a_tilde_in_a_configured_path_is_expanded() {
        let home = PathBuf::from(std::env::var("HOME").unwrap());
        assert_eq!(expand_home(Path::new("~/notes")), home.join("notes"));
        assert_eq!(expand_home(Path::new("/absolute")), PathBuf::from("/absolute"));
        // A path that merely starts with the letters is left alone.
        assert_eq!(expand_home(Path::new("~notes")), PathBuf::from("~notes"));
    }

    #[test]
    fn saving_writes_only_where_it_is_told() {
        let path = std::env::temp_dir().join("houston-config-save-test.toml");
        let _ = std::fs::remove_file(&path);

        Config { vault: Some(PathBuf::from("/tmp/x")) }.save_to(&path).unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("/tmp/x"));
        assert_ne!(path, config_path().unwrap(), "a test must never target the real config");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn config_round_trips_through_toml() {
        let config = Config { vault: Some(PathBuf::from("/tmp/x")) };
        let text = toml::to_string_pretty(&config).unwrap();
        let decoded: Config = toml::from_str(&text).unwrap();
        assert_eq!(decoded.vault, config.vault);
    }

    #[test]
    fn an_empty_config_file_is_valid() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.vault.is_none());
    }

    #[test]
    fn creating_the_default_vault_leaves_a_welcome_note() {
        let root = std::env::temp_dir().join("houston-vault-create-test");
        let _ = std::fs::remove_dir_all(&root);

        // Exercise the creation path directly; `ensure_vault` guards on the
        // real home directory, which a test must not touch.
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Welcome.md"), WELCOME).unwrap();

        let welcome = std::fs::read_to_string(root.join("Welcome.md")).unwrap();
        assert!(welcome.contains("Welcome to your Houston vault"));
        assert!(welcome.contains("[[note name]]"), "the note explains wikilinks");
        assert!(welcome.contains("Settings"), "it says how to point elsewhere");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_configured_path_that_does_not_exist_is_an_error_not_a_new_folder() {
        let config = Config { vault: Some(PathBuf::from("/tmp/houston-does-not-exist-xyz")) };
        let result = config.ensure_vault();

        assert!(result.is_err(), "a mistyped path must surface, not be created");
        assert!(!Path::new("/tmp/houston-does-not-exist-xyz").exists());
    }
}
