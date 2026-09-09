//! Remembering sessions across restarts.
//!
//! Houston is a long-lived workspace, so quitting it should not mean losing
//! what you had open. What gets kept is deliberately small: where each session
//! was running, what you called it, and — for agents that support it — the
//! conversation id needed to resume.
//!
//! **Scrollback is not kept.** Restoring a terminal's output would mean
//! persisting megabytes to fake something that was never real: the child
//! process is gone either way. A shell that reopens in the right directory is
//! honest about that; a shell that reopens showing yesterday's output is not.

use crate::{
    hooks,
    session::{Kind, Sessions},
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One session, as remembered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Remembered {
    pub name: String,
    /// Whether the name was chosen rather than derived. A derived name is
    /// recomputed on restore, so a renamed shell keeps its name and an
    /// unnamed one picks up its directory again.
    #[serde(default)]
    pub named_by_user: bool,
    pub cwd: PathBuf,
    /// `true` for an agent, `false` for a shell.
    pub agent: bool,
    /// The executable, so a restored agent uses the provider it was started
    /// with rather than whatever happens to be first on `PATH` today.
    #[serde(default)]
    pub command: Option<String>,
    /// The worktree it runs in, by name.
    #[serde(default)]
    pub worktree: Option<String>,
    /// The agent's own conversation id, captured from its hooks. This is what
    /// makes a restored session a continuation rather than a fresh start.
    #[serde(default)]
    pub conversation: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub sessions: Vec<Remembered>,
}

pub fn path() -> Result<PathBuf> {
    Ok(hooks::state_dir()?.join("state.json"))
}

impl State {
    /// What is currently open, in sidebar order.
    #[must_use]
    pub fn capture(sessions: &Sessions) -> Self {
        Self {
            sessions: sessions
                .iter()
                // A session that has already exited is not worth reopening.
                .filter(|session| !session.has_exited())
                .map(|session| Remembered {
                    name: session.name.clone(),
                    named_by_user: session.named_by_user(),
                    cwd: session.directory().to_path_buf(),
                    agent: matches!(session.kind, Kind::Agent { .. }),
                    command: session.spec.command.clone(),
                    worktree: session.worktree.clone(),
                    conversation: session.conversation.clone(),
                })
                .collect(),
        }
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes the state.
    ///
    /// Takes the path rather than resolving it, so tests never write the real
    /// `~/.houston/state.json`.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text)
            .with_context(|| format!("could not write {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_through_json() {
        let state = State {
            sessions: vec![Remembered {
                name: "auth refactor".to_string(),
                named_by_user: true,
                cwd: PathBuf::from("/tmp/project"),
                agent: true,
                command: Some("claude".to_string()),
                worktree: Some("auth-refactor".to_string()),
                conversation: Some("abc-123".to_string()),
            }],
        };

        let path = std::env::temp_dir().join("houston-state-roundtrip.json");
        state.save_to(&path).unwrap();
        let back = State::load_from(&path);

        assert_eq!(back.sessions.len(), 1);
        assert_eq!(back.sessions[0].name, "auth refactor");
        assert_eq!(back.sessions[0].conversation.as_deref(), Some("abc-123"));
        assert_eq!(back.sessions[0].worktree.as_deref(), Some("auth-refactor"));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_missing_or_broken_state_file_is_simply_empty() {
        // Losing your session list is annoying; refusing to start is worse.
        assert!(State::load_from(Path::new("/tmp/houston-no-such-state.json")).sessions.is_empty());

        let path = std::env::temp_dir().join("houston-state-broken.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(State::load_from(&path).sessions.is_empty());

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn older_state_files_without_the_newer_fields_still_load() {
        let path = std::env::temp_dir().join("houston-state-old.json");
        std::fs::write(&path, r#"{"sessions":[{"name":"x","cwd":"/tmp","agent":false}]}"#).unwrap();

        let state = State::load_from(&path);
        assert_eq!(state.sessions.len(), 1);
        assert!(state.sessions[0].conversation.is_none());
        assert!(!state.sessions[0].named_by_user);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn exited_sessions_are_not_remembered() {
        let mut sessions = Sessions::new();
        sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();
        assert_eq!(State::capture(&sessions).sessions.len(), 1);

        sessions.selected_mut().unwrap().state = crate::session::State::Exited(Some(0));
        assert!(
            State::capture(&sessions).sessions.is_empty(),
            "reopening something that already finished is not persistence"
        );
    }

    #[test]
    fn a_shell_records_where_it_ran_but_no_conversation() {
        let mut sessions = Sessions::new();
        sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();

        let captured = State::capture(&sessions);
        assert!(!captured.sessions[0].agent);
        assert!(captured.sessions[0].conversation.is_none(), "shells have no conversation");
        assert_eq!(captured.sessions[0].cwd, std::env::temp_dir());
    }
}
