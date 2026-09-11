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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Where the vault lives. `None` means the default under `~/.houston`.
    pub vault: Option<PathBuf>,
    /// Where new agent sessions start. `None` means the home directory.
    ///
    /// Not the directory Houston was launched from: you start Houston once and
    /// leave it running for days, so where it happened to be launched says
    /// nothing about where the next agent should work.
    pub agent_directory: Option<PathBuf>,
    /// Whether Houston captures the mouse. `None` means yes.
    ///
    /// A real trade, which is why it is a setting: capturing gives you a
    /// working scroll wheel inside sessions, and costs your terminal's own
    /// click-drag text selection.
    pub mouse: Option<bool>,
    /// The theme by name. `None` means the default.
    pub theme: Option<String>,
    /// Draw powerline separators. Needs a powerline-patched font.
    ///
    /// Defaults to off, and stays off unless somebody says otherwise. A
    /// terminal without the glyphs draws boxes, and a first run that looks
    /// broken is worse than a first run that looks plain.
    pub powerline: Option<bool>,
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

    /// Where theme files live, created with a template on first use.
    pub fn themes_dir() -> Result<PathBuf> {
        let directory = crate::hooks::state_dir()?.join("themes");
        std::fs::create_dir_all(&directory)
            .with_context(|| format!("could not create {}", directory.display()))?;

        // Written once. Never overwritten — someone may well have edited it.
        let template = directory.join("example.toml");
        if !template.exists() {
            let _ = std::fs::write(&template, crate::ui::theme::TEMPLATE);
        }
        Ok(directory)
    }

    /// The theme in force, and every theme available to choose from.
    #[must_use]
    pub fn themes(&self) -> (crate::ui::Theme, Vec<crate::ui::theme::Named>) {
        let available = Self::themes_dir()
            .map_or_else(|_| crate::ui::theme::built_in(), |dir| crate::ui::theme::available(&dir));

        let chosen = self
            .theme
            .as_deref()
            .and_then(|name| {
                let named = |wanted: &str| available.iter().find(|theme| theme.name == wanted);
                // A theme that has been renamed is still the theme you chose.
                named(name).or_else(|| crate::ui::theme::resolve_alias(name).and_then(named))
            })
            .or_else(|| available.first())
            .map_or_else(crate::ui::Theme::default, |named| named.theme);

        (chosen, available)
    }

    /// Whether to draw powerline separators.
    #[must_use]
    pub const fn powerline_enabled(&self) -> bool {
        match self.powerline {
            Some(enabled) => enabled,
            None => false,
        }
    }

    /// Whether to capture the mouse.
    #[must_use]
    pub const fn mouse_enabled(&self) -> bool {
        match self.mouse {
            Some(enabled) => enabled,
            None => true,
        }
    }

    /// Where a new agent session should start.
    #[must_use]
    pub fn agent_root(&self) -> PathBuf {
        self.agent_directory.as_deref().map_or_else(
            || std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from),
            crate::paths::expand_home,
        )
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

        if !root.is_dir() && self.vault_is_explicit() {
            anyhow::bail!("no vault at {} — check the path in Settings", root.display());
        }

        // **A test must never write into the real vault.** Every `App::new()`
        // in the suite reaches here, and without this guard the backfill below
        // puts twenty files into whatever sits at `~/.houston/vault` on the
        // machine running it. That is the `$HOME` trap `CLAUDE.md` records,
        // one level further down than the last time it was paid for: the write
        // is not in the test, it is three calls beneath a constructor the test
        // happens to use. A machine with no vault then gets no browser and the
        // vault tests skip themselves, which is the honest outcome.
        if cfg!(test) {
            return Ok(root);
        }

        if !root.is_dir() {
            std::fs::create_dir_all(&root)
                .with_context(|| format!("could not create {}", root.display()))?;
        }

        // Only ever Houston's own folder. Somebody who pointed Houston at
        // their existing Obsidian vault gets an opinion about how to arrange
        // it, not twenty files they did not ask for. `write` itself is a no-op
        // once the vault has the structure.
        //
        // Compared against the default *path* rather than against
        // `vault_is_explicit`, because Houston writes its own default into
        // `config.toml` — so the folder Houston made for you reads as
        // explicitly chosen, and the backfill it was written for skipped the
        // one vault it was meant to reach.
        if crate::hooks::state_dir().is_ok_and(|state| root == state.join("vault")) {
            crate::vault::scaffold::write(&root)?;
        }

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
    environment.or(configured).map_or(default, crate::paths::expand_home)
}

pub fn config_path() -> Result<PathBuf> {
    Ok(crate::hooks::state_dir()?.join("config.toml"))
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
    fn the_agent_directory_defaults_to_home() {
        let home = PathBuf::from(std::env::var("HOME").unwrap());
        assert_eq!(Config::default().agent_root(), home);

        let configured =
            Config { agent_directory: Some(PathBuf::from("~/Projects")), ..Config::default() };
        assert_eq!(configured.agent_root(), home.join("Projects"), "and expands a tilde");
    }

    #[test]
    fn saving_writes_only_where_it_is_told() {
        let path = std::env::temp_dir().join("houston-config-save-test.toml");
        let _ = std::fs::remove_file(&path);

        Config { vault: Some(PathBuf::from("/tmp/x")), ..Config::default() }
            .save_to(&path)
            .unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("/tmp/x"));
        assert_ne!(path, config_path().unwrap(), "a test must never target the real config");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn config_round_trips_through_toml() {
        let config = Config { vault: Some(PathBuf::from("/tmp/x")), ..Config::default() };
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
    fn a_configured_path_that_does_not_exist_is_an_error_not_a_new_folder() {
        let config = Config {
            vault: Some(PathBuf::from("/tmp/houston-does-not-exist-xyz")),
            ..Config::default()
        };
        let result = config.ensure_vault();

        assert!(result.is_err(), "a mistyped path must surface, not be created");
        assert!(!Path::new("/tmp/houston-does-not-exist-xyz").exists());
    }
}
