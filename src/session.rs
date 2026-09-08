//! Sessions: the unit of work in Houston.
//!
//! ADR-0006 — a session is a PTY plus a launch spec. An agent session and a
//! shell session differ only in what they launch, which is why "let me just use
//! the terminal in here" needs no special machinery.

use crate::{
    hooks::{self, Kind as HookKind},
    input,
    provider::{self, Provider},
    pty::{LaunchSpec, PtySession, Size},
};
use alacritty_terminal::{term::TermMode, tty::ChildEvent};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::path::Path;

/// Lines moved per wheel notch. Three is the terminal convention.
const SCROLL_LINES: i32 = 3;

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
    /// Started but not yet asked to do anything, or finished its turn and
    /// waiting for your next message.
    ///
    /// Distinct from `Running` on purpose: an agent that has gone quiet
    /// because it is done is not the same as one that is still thinking, and
    /// conflating them is what made the board look stuck.
    Idle,
    Running,
    /// Set by a hook that says the agent cannot continue. Never guessed at
    /// from terminal output — a hook is a fact, a screen-scrape is a guess.
    AwaitingInput,
    Exited(Option<i32>),
}

impl State {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::AwaitingInput => "needs you",
            Self::Exited(_) => "exited",
        }
    }
}

pub struct Session {
    pub id: SessionId,
    pub name: String,
    /// What the name reverts to when a user-chosen one is cleared.
    default_name: String,
    /// Whether `name` was chosen by the user rather than derived.
    named_by_user: bool,
    pub kind: Kind,
    pub state: State,
    /// Kept so a session can be restarted, and persisted across restarts in
    /// Phase 2's `~/.houston/state.json`.
    pub spec: LaunchSpec,
    /// The worktree this session runs in, by name, if it was given one.
    ///
    /// Lets the worktree manager say which checkouts are still in use, so
    /// cleaning up does not mean guessing.
    pub worktree: Option<String>,
    /// Whether this session's hooks are still the ones installed in its
    /// project.
    ///
    /// Claude Code's hooks live in the project's settings file and carry a
    /// fixed session id, so a second agent started in the same directory
    /// replaces them. The first then reports nothing and its state freezes on
    /// whatever it last said. Recording that lets the board admit the status is
    /// stale rather than showing a stale value as though it were current.
    pub hooks_live: bool,
    /// The branch its directory is on, refreshed periodically.
    ///
    /// Cached rather than read per frame: it is a file read, but sixty of them
    /// a second per session for something that changes hourly is waste.
    pub branch: Option<String>,
    pty: PtySession,
}

impl Session {
    pub const fn pty(&self) -> &PtySession {
        &self.pty
    }

    /// Whether this session's board status can still be trusted.
    ///
    /// Only agents have hook-driven status, so a shell is never stale.
    pub const fn status_is_stale(&self) -> bool {
        matches!(self.kind, Kind::Agent { .. }) && !self.hooks_live
    }

    /// The directory this session runs in.
    pub fn directory(&self) -> &Path {
        &self.spec.cwd
    }

    /// Re-reads the branch. Cheap, but not free — call on a timer, not a frame.
    pub fn refresh_branch(&mut self) {
        self.branch = crate::worktree::branch_of(&self.spec.cwd);
    }

    /// Sets a name the user chose, or clears it back to the default.
    ///
    /// A user-chosen name wins over the child's own terminal title: if you
    /// took the trouble to name a session, a stray OSC sequence from a shell
    /// should not rename it back.
    pub fn rename(&mut self, name: Option<String>) {
        if let Some(name) = name {
            self.name = name;
            self.named_by_user = true;
        } else {
            self.named_by_user = false;
            self.name = self.default_name.clone();
        }
    }

    /// The child's own title (OSC 0/2), falling back to the session name.
    ///
    /// Shells set this to the running command, so an attached session labels
    /// itself with whatever it is doing.
    pub fn display_name(&self) -> String {
        if self.named_by_user {
            return self.name.clone();
        }
        self.pty.title().unwrap_or_else(|| self.name.clone())
    }

    pub fn resize(&mut self, size: Size) {
        self.pty.resize(size);
    }

    /// Scrolls this session's scrollback. Positive goes back in history.
    pub fn scroll(&self, lines: i32) {
        self.pty.scroll(lines);
    }

    pub fn scrollback_offset(&self) -> usize {
        self.pty.scrollback_offset()
    }

    /// Scrolls a whole screen of history.
    pub fn page(&self, up: bool) {
        self.pty.page(up);
    }

    /// Jumps to the start or the end of the history.
    pub fn scroll_to_edge(&self, top: bool) {
        if top {
            self.pty.scroll_to_top();
        } else {
            self.pty.scroll_to_bottom();
        }
    }

    /// Handles a wheel or click while this session is attached.
    ///
    /// Three cases, in order. A child that asked for mouse reporting gets the
    /// event — that is its business, not ours. A full-screen child that asked
    /// for alternate scroll gets arrow keys, which is the convention `less`
    /// and friends rely on. Anything else scrolls *our* scrollback, which is
    /// what makes the wheel work in an agent session at all.
    pub fn send_mouse(&mut self, event: MouseEvent, column: u16, line: u16) -> Result<()> {
        let mode = self.pty.mode();

        let wants_mouse = mode.intersects(
            TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG,
        );
        if wants_mouse {
            if let Some(bytes) = input::encode_mouse(event, column, line, mode) {
                self.pty.write(&bytes)?;
            }
            return Ok(());
        }

        let lines = match event.kind {
            MouseEventKind::ScrollUp => SCROLL_LINES,
            MouseEventKind::ScrollDown => -SCROLL_LINES,
            _ => return Ok(()),
        };

        if mode.contains(TermMode::ALT_SCREEN) {
            // No history on the alternate screen, so scrolling it would do
            // nothing. Arrow keys are what the child expects instead.
            if mode.contains(TermMode::ALTERNATE_SCROLL) {
                let arrow = if lines > 0 { KeyCode::Up } else { KeyCode::Down };
                let key = KeyEvent::new(arrow, KeyModifiers::NONE);
                for _ in 0..SCROLL_LINES {
                    self.send_key(key)?;
                }
            }
            return Ok(());
        }

        self.pty.scroll(lines);
        Ok(())
    }

    /// Sends a key press to the child.
    pub fn send_key(&mut self, key: KeyEvent) -> Result<()> {
        let mode = self.pty.term().lock().map_or_else(|_| TermMode::default(), |term| *term.mode());

        if let Some(bytes) = input::encode(key, mode) {
            // Typing into a scrolled-back view and not seeing where it went is
            // disorienting, so any keypress returns to the live output.
            self.pty.scroll_to_bottom();
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

    /// Applies a lifecycle event reported by an agent hook.
    pub const fn apply_hook(&mut self, kind: HookKind) {
        // A hook from a session that has already exited is stale; the exit is
        // the more truthful state.
        if matches!(self.state, State::Exited(_)) {
            return;
        }
        self.state = match kind {
            HookKind::Working => State::Running,
            HookKind::Attention => State::AwaitingInput,
            HookKind::Idle => State::Idle,
        };
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
    hook_warning: Option<String>,
}

impl Sessions {
    pub const fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            focus: Focus::Browsing,
            next_id: 1,
            hook_warning: None,
        }
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

    /// Whether a live agent session is already running in `directory`.
    ///
    /// Matters because Claude Code's hooks live in the project's settings file
    /// and carry a fixed session id. Installing hooks for a second agent in the
    /// same directory replaces the first's, so the first goes silent and its
    /// state freezes on whatever it last reported — which reads as the board
    /// getting stuck. A worktree gives each agent its own directory and avoids
    /// this entirely.
    pub fn agent_in(&self, directory: &Path) -> Option<&Session> {
        self.items.iter().find(|session| {
            matches!(session.kind, Kind::Agent { .. })
                && !matches!(session.state, State::Exited(_))
                && session.spec.cwd == directory
        })
    }

    /// Whether any live session is using a worktree.
    pub fn uses_worktree(&self, name: &str) -> bool {
        self.items.iter().any(|session| {
            session.worktree.as_deref() == Some(name) && !matches!(session.state, State::Exited(_))
        })
    }

    /// Routes a hook notification to the session that raised it.
    ///
    /// Returns `true` if it matched a live session.
    pub fn apply_hook(&mut self, session: u64, kind: HookKind) -> bool {
        let Some(target) = self.items.iter_mut().find(|item| item.id.0 == session) else {
            return false;
        };
        target.apply_hook(kind);
        true
    }

    /// Starts an agent session using the first available provider.
    ///
    /// Also wires Claude Code's hooks into the project so the board can tell
    /// whether the agent is working or waiting on you. A failure to install
    /// hooks is reported but does not stop the session: a working agent with
    /// no status is far better than no agent.
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

        let agent = matches!(kind, Kind::Agent { .. });

        // Checked before spawning, because afterwards the new session is
        // itself "an agent in this directory".
        let displaced = agent
            .then(|| {
                self.agent_in(cwd).map(|session| {
                    // Bounded: an agent's display name is whatever terminal
                    // title it set, which can be a whole path. An unbounded
                    // name pushes the advice off the end of the footer.
                    let name: String = session.display_name().chars().take(18).collect();
                    (session.id, name)
                })
            })
            .flatten();

        let id = self.spawn(name, kind, spec, size)?;

        if agent {
            self.hook_warning = install_hooks(cwd, id)
                .err()
                .map(|error| format!("agent status unavailable: {error}"));

            if let Some((displaced_id, name)) = displaced {
                // Its hooks have just been overwritten by ours. Mark it, so the
                // board shows "status stale" rather than a frozen value that
                // looks current.
                if let Some(session) =
                    self.items.iter_mut().find(|session| session.id == displaced_id)
                {
                    session.hooks_live = false;
                }

                // Sized for an 80-column terminal. The footer is one row and
                // does not wrap, so a longer sentence quietly loses its own
                // ending — and the ending is where the advice lives.
                self.hook_warning =
                    Some(format!("{name}'s status will freeze — two agents here. Use a worktree."));
            }
        }
        Ok(id)
    }

    /// A complete, ready-to-show message about the most recent agent session's
    /// status reporting, if there is one.
    ///
    /// Complete on purpose: callers used to prefix it with "agent status
    /// unavailable:", which was wrong for a collision and ate twenty-six cells
    /// of a footer that does not wrap — pushing the actual advice off the end
    /// on an 80-column terminal.
    pub const fn take_hook_warning(&mut self) -> Option<String> {
        self.hook_warning.take()
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

        // An agent has not been asked to do anything yet; a shell is simply
        // live. Claiming a fresh agent is "working" would be the same lie the
        // `Stop` mapping used to tell.
        let state = if matches!(kind, Kind::Agent { .. }) { State::Idle } else { State::Running };

        self.items.push(Session {
            id,
            default_name: name.clone(),
            name,
            named_by_user: false,
            kind,
            state,
            hooks_live: true,
            branch: crate::worktree::branch_of(&spec.cwd),
            spec,
            worktree: None,
            pty,
        });
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

    pub const fn select(&mut self, index: usize) {
        if index < self.items.len() {
            self.selected = index;
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

    /// Re-reads every session's branch. Driven on a slow timer by the event
    /// loop, because a branch changes when you switch it, not when you blink.
    pub fn refresh_branches(&mut self) {
        for session in &mut self.items {
            session.refresh_branch();
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

/// Wires Houston's hooks into a project's Claude Code settings.
fn install_hooks(cwd: &Path, id: SessionId) -> Result<()> {
    let socket = hooks::socket_path()?;
    hooks::install(cwd, id, &socket)?;
    Ok(())
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

#[cfg(test)]
mod hook_tests {
    use super::*;

    fn sessions_with_one_shell() -> Sessions {
        let mut sessions = Sessions::new();
        sessions.spawn_shell(&std::env::temp_dir(), Size::new(24, 80)).unwrap();
        sessions
    }

    #[test]
    fn a_full_turn_walks_through_the_states_and_ends_idle() {
        let mut sessions = sessions_with_one_shell();
        let id = sessions.selected().unwrap().id;

        // You send a prompt.
        assert!(sessions.apply_hook(id.0, HookKind::Working));
        assert_eq!(sessions.selected().unwrap().state, State::Running);

        // It asks permission and stops dead.
        assert!(sessions.apply_hook(id.0, HookKind::Attention));
        assert_eq!(sessions.selected().unwrap().state, State::AwaitingInput);

        // You approve; the tool runs and reports back. Without this the
        // session would sit on "needs you" for the whole of the work.
        assert!(sessions.apply_hook(id.0, HookKind::Working));
        assert_eq!(sessions.selected().unwrap().state, State::Running);

        // The turn ends. This is the one that used to say "running" and pin
        // the board on busy until you next typed.
        assert!(sessions.apply_hook(id.0, HookKind::Idle));
        assert_eq!(sessions.selected().unwrap().state, State::Idle);
    }

    #[test]
    fn several_turns_in_a_row_do_not_drift() {
        let mut sessions = sessions_with_one_shell();
        let id = sessions.selected().unwrap().id;

        for _ in 0..5 {
            sessions.apply_hook(id.0, HookKind::Working);
            sessions.apply_hook(id.0, HookKind::Attention);
            sessions.apply_hook(id.0, HookKind::Working);
            sessions.apply_hook(id.0, HookKind::Idle);
            assert_eq!(
                sessions.selected().unwrap().state,
                State::Idle,
                "every cycle must land back on idle, not accumulate a wrong state"
            );
        }
    }

    #[test]
    fn a_hook_for_an_unknown_session_is_ignored() {
        let mut sessions = sessions_with_one_shell();
        assert!(!sessions.apply_hook(9999, HookKind::Attention));
        assert_eq!(sessions.selected().unwrap().state, State::Running);
    }

    #[test]
    fn a_hook_cannot_resurrect_an_exited_session() {
        let mut sessions = sessions_with_one_shell();
        let id = sessions.selected().unwrap().id;
        sessions.selected_mut().unwrap().state = State::Exited(Some(0));

        sessions.apply_hook(id.0, HookKind::Attention);
        assert_eq!(
            sessions.selected().unwrap().state,
            State::Exited(Some(0)),
            "exit is the more truthful state; a late hook must not override it"
        );
    }
}

#[cfg(test)]
mod collision_tests {
    use super::*;

    #[test]
    fn a_second_agent_in_the_same_directory_is_flagged() {
        // Two agents sharing a directory share one hooks file, so the second
        // silences the first — the board then freezes on its last state.
        let mut sessions = Sessions::new();
        let directory = std::env::temp_dir();

        assert!(sessions.agent_in(&directory).is_none(), "nothing there yet");

        sessions.items.push(Session {
            id: SessionId(1),
            name: "first".to_string(),
            default_name: "first".to_string(),
            named_by_user: false,
            kind: Kind::Agent { provider: "Claude Code" },
            state: State::Idle,
            hooks_live: true,
            branch: None,
            spec: LaunchSpec::command("claude", Vec::new(), directory.clone()),
            worktree: None,
            pty: PtySession::spawn(
                &LaunchSpec::command(provider::login_shell(), Vec::new(), directory.clone()),
                Size::new(24, 80),
            )
            .unwrap(),
        });

        assert!(sessions.agent_in(&directory).is_some(), "the live agent is found");

        // An exited one does not count: its hooks are nobody's concern.
        sessions.items[0].state = State::Exited(Some(0));
        assert!(sessions.agent_in(&directory).is_none());
    }

    #[test]
    fn the_displaced_session_is_marked_stale_rather_than_left_looking_current() {
        let mut sessions = Sessions::new();
        let directory = std::env::temp_dir();

        sessions.items.push(Session {
            id: SessionId(1),
            name: "first".to_string(),
            default_name: "first".to_string(),
            named_by_user: false,
            kind: Kind::Agent { provider: "Claude Code" },
            state: State::Running,
            hooks_live: true,
            branch: None,
            spec: LaunchSpec::command("claude", Vec::new(), directory.clone()),
            worktree: None,
            pty: PtySession::spawn(
                &LaunchSpec::command(provider::login_shell(), Vec::new(), directory.clone()),
                Size::new(24, 80),
            )
            .unwrap(),
        });
        sessions.next_id = 2;

        assert!(!sessions.items[0].status_is_stale(), "it owns its hooks for now");

        // A second agent in the same directory overwrites its hooks.
        sessions.spawn_agent(&directory, Size::new(24, 80)).unwrap();

        assert!(
            sessions.items[0].status_is_stale(),
            "the displaced session must admit its status has stopped updating"
        );

        let warning = sessions.take_hook_warning().expect("the user is told");
        assert!(warning.contains("worktree"), "and pointed at the fix: {warning}");
        assert!(warning.contains("freeze"), "and told what goes wrong: {warning}");
        assert!(warning.len() < 80, "must fit a narrow footer: {} chars", warning.len());
    }

    #[test]
    fn a_shell_is_never_reported_as_stale() {
        let mut sessions = Sessions::new();
        sessions.spawn_shell(&std::env::temp_dir(), Size::new(24, 80)).unwrap();

        assert!(!sessions.selected().unwrap().status_is_stale(), "shells have no hook status");
    }

    #[test]
    fn a_shell_in_the_same_directory_is_not_a_collision() {
        let mut sessions = Sessions::new();
        let directory = std::env::temp_dir();
        sessions.spawn_shell(&directory, Size::new(24, 80)).unwrap();

        assert!(sessions.agent_in(&directory).is_none(), "shells install no hooks");
    }
}
