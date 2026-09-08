//! Sessions: the unit of work in Houston.
//!
//! ADR-0006 — a session is a PTY plus a launch spec. An agent session and a
//! shell session differ only in what they launch, which is why "let me just use
//! the terminal in here" needs no special machinery.

use crate::{
    input,
    provider::{self, Provider},
    pty::{LaunchSpec, PtySession, Size},
};
use alacritty_terminal::{term::TermMode, tty::ChildEvent};
use anyhow::Result;
use crossterm::event::KeyEvent;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Agent { provider: &'static str },
    Shell,
}

/// What a session is doing, as far as Houston can honestly tell.
///
/// Agent sessions get `AwaitingInput` from Claude Code hooks in Phase 6.
/// Shell sessions never claim it — we have no way to know, and inventing a
/// state would make the board lie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    /// Set by Claude Code's `PermissionRequest` hook in Phase 6. Nothing
    /// constructs it yet, and nothing should guess at it from terminal output.
    #[expect(dead_code, reason = "Phase 6's hook ingress is what sets this")]
    AwaitingInput,
    Exited(Option<i32>),
}

impl State {
    #[expect(dead_code, reason = "Phase 6 renders these on the board")]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::AwaitingInput => "needs you",
            Self::Exited(_) => "exited",
        }
    }
}

pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub kind: Kind,
    pub state: State,
    /// Kept so a session can be restarted, and persisted across restarts in
    /// Phase 2's `~/.houston/state.json`.
    #[expect(dead_code, reason = "Phase 2 persists and restarts sessions from this")]
    pub spec: LaunchSpec,
    pty: PtySession,
}

impl Session {
    pub const fn pty(&self) -> &PtySession {
        &self.pty
    }

    /// The child's own title (OSC 0/2), falling back to the session name.
    ///
    /// Shells set this to the running command, so an attached session labels
    /// itself with whatever it is doing.
    pub fn display_name(&self) -> String {
        self.pty.title().unwrap_or_else(|| self.name.clone())
    }

    pub fn resize(&mut self, size: Size) {
        self.pty.resize(size);
    }

    /// Sends a key press to the child.
    pub fn send_key(&mut self, key: KeyEvent) -> Result<()> {
        let mode = self.pty.term().lock().map_or_else(|_| TermMode::default(), |term| *term.mode());

        if let Some(bytes) = input::encode(key, mode) {
            self.pty.write(&bytes)?;
        }
        Ok(())
    }

    /// Sends pasted text as one write. The whole point of ADR-0005.
    pub fn send_paste(&mut self, text: &str) -> Result<()> {
        let bracketed = self.pty.wants_bracketed_paste();
        self.pty.write(&input::encode_paste(text, bracketed))
    }

    /// Sends text as if typed, followed by Return.
    ///
    /// Used by the vault bridge in Phase 5 to push `@path` into an agent.
    #[expect(dead_code, reason = "Phase 5 uses this to push @path into an agent")]
    pub fn send_line(&mut self, text: &str) -> Result<()> {
        let bracketed = self.pty.wants_bracketed_paste();
        let mut bytes = input::encode_paste(text, bracketed);
        bytes.push(b'\r');
        self.pty.write(&bytes)
    }

    fn poll(&mut self) -> bool {
        let dirty = self.pty.take_dirty();

        let exited = if let Some(ChildEvent::Exited(code)) = self.pty.poll_child_event() {
            self.state = State::Exited(code);
            true
        } else {
            false
        };

        dirty || exited
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Session")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

/// Whether keystrokes drive Houston or the focused child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// Keys control Houston: move the selection, create, kill.
    Browsing,
    /// Keys go to the child. Only the detach key is intercepted.
    Attached,
}

#[derive(Debug)]
pub struct Sessions {
    items: Vec<Session>,
    selected: usize,
    focus: Focus,
    next_id: u64,
}

impl Sessions {
    pub const fn new() -> Self {
        Self { items: Vec::new(), selected: 0, focus: Focus::Browsing, next_id: 1 }
    }

    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub const fn len(&self) -> usize {
        self.items.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Session> {
        self.items.iter()
    }

    pub const fn selected_index(&self) -> usize {
        self.selected
    }

    pub const fn focus(&self) -> Focus {
        self.focus
    }

    pub fn selected(&self) -> Option<&Session> {
        self.items.get(self.selected)
    }

    pub fn selected_mut(&mut self) -> Option<&mut Session> {
        self.items.get_mut(self.selected)
    }

    /// Starts an agent session using the first available provider.
    pub fn spawn_agent(&mut self, cwd: &Path, size: Size) -> Result<SessionId> {
        let provider = provider::default();
        let (kind, spec) = provider.map_or_else(
            // No agent CLI installed: fall back to a shell rather than failing.
            || {
                (
                    Kind::Shell,
                    LaunchSpec::command(provider::login_shell(), Vec::new(), cwd.to_path_buf()),
                )
            },
            |provider: Provider| {
                (Kind::Agent { provider: provider.label }, provider.launch(cwd.to_path_buf()))
            },
        );

        let name = match &kind {
            Kind::Agent { provider } => (*provider).to_string(),
            Kind::Shell => directory_label(cwd),
        };
        self.spawn(name, kind, spec, size)
    }

    pub fn spawn_shell(&mut self, cwd: &Path, size: Size) -> Result<SessionId> {
        let spec = LaunchSpec::command(provider::login_shell(), Vec::new(), cwd.to_path_buf());
        self.spawn(directory_label(cwd), Kind::Shell, spec, size)
    }

    fn spawn(
        &mut self,
        name: String,
        kind: Kind,
        spec: LaunchSpec,
        size: Size,
    ) -> Result<SessionId> {
        let pty = PtySession::spawn(&spec, size)?;
        let id = SessionId(self.next_id);
        self.next_id += 1;

        self.items.push(Session { id, name, kind, state: State::Running, spec, pty });
        self.selected = self.items.len() - 1;
        Ok(id)
    }

    /// Closes the selected session.
    ///
    /// Dropping the `PtySession` is what kills the child: `alacritty_terminal`'s
    /// `Pty::drop` sends SIGHUP and reaps. There is deliberately no explicit
    /// kill call here — adding one would double up on that.
    pub fn close_selected(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.items.remove(self.selected);
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
        if self.items.is_empty() {
            self.focus = Focus::Browsing;
        }
    }

    pub const fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1) % self.items.len();
        }
    }

    pub const fn select_previous(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + self.items.len() - 1) % self.items.len();
        }
    }

    /// Attaching to a dead session would swallow keystrokes with no feedback.
    pub fn attach(&mut self) -> bool {
        let alive = self.selected().is_some_and(|s| !matches!(s.state, State::Exited(_)));
        if alive {
            self.focus = Focus::Attached;
        }
        alive
    }

    pub const fn detach(&mut self) {
        self.focus = Focus::Browsing;
    }

    /// Keeps every child's grid sized to the area it is drawn into.
    pub fn resize_all(&mut self, size: Size) {
        for session in &mut self.items {
            session.resize(size);
        }
    }

    /// Polls every session for new output and child exits.
    ///
    /// Returns `true` if anything changed and the frame needs redrawing.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for session in &mut self.items {
            changed |= session.poll();
        }
        changed
    }
}

impl Default for Sessions {
    fn default() -> Self {
        Self::new()
    }
}

/// A short label for a working directory: its last component.
fn directory_label(path: &Path) -> String {
    path.file_name().map_or_else(|| "shell".to_string(), |name| name.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size() -> Size {
        Size::new(24, 80)
    }

    fn shell_session(sessions: &mut Sessions) -> SessionId {
        sessions.spawn_shell(&std::env::temp_dir(), size()).expect("spawning a shell should work")
    }

    #[test]
    fn spawning_selects_the_new_session() {
        let mut sessions = Sessions::new();
        assert!(sessions.is_empty());

        shell_session(&mut sessions);
        shell_session(&mut sessions);

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions.selected_index(), 1, "the newest session takes focus");
    }

    #[test]
    fn selection_wraps_in_both_directions() {
        let mut sessions = Sessions::new();
        shell_session(&mut sessions);
        shell_session(&mut sessions);

        assert_eq!(sessions.selected_index(), 1, "the newest session starts selected");

        sessions.select_next();
        assert_eq!(sessions.selected_index(), 0, "wrapping forwards lands on the first");

        sessions.select_previous();
        assert_eq!(sessions.selected_index(), 1, "wrapping backwards lands on the last");
    }

    #[test]
    fn closing_the_last_session_keeps_the_index_valid() {
        let mut sessions = Sessions::new();
        shell_session(&mut sessions);
        shell_session(&mut sessions);

        assert_eq!(sessions.selected_index(), 1);
        sessions.close_selected();
        assert_eq!(sessions.selected_index(), 0, "selection must not dangle past the end");

        sessions.close_selected();
        assert!(sessions.is_empty());
        assert_eq!(sessions.focus(), Focus::Browsing, "an empty list cannot stay attached");
    }

    #[test]
    fn cannot_attach_to_an_exited_session() {
        let mut sessions = Sessions::new();
        shell_session(&mut sessions);

        assert!(sessions.attach(), "a live session should accept focus");
        sessions.detach();

        sessions.selected_mut().unwrap().state = State::Exited(Some(0));
        assert!(!sessions.attach(), "a dead session must not swallow keystrokes");
        assert_eq!(sessions.focus(), Focus::Browsing);
    }

    #[test]
    fn attaching_with_no_sessions_is_a_no_op() {
        let mut sessions = Sessions::new();
        assert!(!sessions.attach());
        assert_eq!(sessions.focus(), Focus::Browsing);
    }

    #[test]
    fn a_shell_session_is_named_after_its_directory() {
        let mut sessions = Sessions::new();
        sessions.spawn_shell(Path::new("/usr/local/lib"), size()).unwrap();
        assert_eq!(sessions.selected().unwrap().name, "lib");
    }
}
