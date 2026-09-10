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
use alacritty_terminal::{
    index::Side,
    selection::{Selection, SelectionType},
    term::TermMode,
    tty::ChildEvent,
    vi_mode::ViMotion,
};
use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::path::{Path, PathBuf};

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
    /// When this session was started.
    ///
    /// Used to match a Codex rollout file to *this* session rather than to one
    /// from last week that happened to run in the same directory.
    pub started_at: std::time::SystemTime,
    /// The agent's own conversation id, reported by its hooks.
    ///
    /// Claude Code passes `session_id` in every hook payload, and
    /// `claude --resume <id>` reopens that conversation. Capturing it is what
    /// lets a restored session continue rather than start again.
    pub conversation: Option<String>,
    /// Where the child has actually got to, once anyone has looked.
    ///
    /// `None` until the first refresh, and left alone when a look fails —
    /// "cannot tell" is not the same as "moved back to where it started".
    pub live_cwd: Option<PathBuf>,
    /// The branch its directory is on, refreshed periodically.
    ///
    /// Cached rather than read per frame: it is a file read, but sixty of them
    /// a second per session for something that changes hourly is waste.
    pub branch: Option<String>,
    /// How much has changed in its directory. `None` outside a repository.
    ///
    /// **Not on the branch timer.** Reading `.git/HEAD` is a file read; this
    /// is two git subprocesses, so refreshing every session every three
    /// seconds would scale the cost with how busy you are. It is refreshed on
    /// the two occasions the number can have moved: a hook firing, which means
    /// the agent just did something, and the selected session's own tick, so
    /// the card you are looking at stays live even for a shell that fires no
    /// hooks at all.
    pub changes: Option<crate::diff::Changes>,
    pty: PtySession,
}

impl Session {
    pub const fn pty(&self) -> &PtySession {
        &self.pty
    }

    /// Whether the session has finished. Not worth reopening.
    pub const fn has_exited(&self) -> bool {
        matches!(self.state, State::Exited(_))
    }

    /// Whether the name was chosen rather than derived.
    pub const fn named_by_user(&self) -> bool {
        self.named_by_user
    }

    /// Whether this session's board status can still be trusted.
    ///
    /// Only agents have hook-driven status, so a shell is never stale.
    pub const fn status_is_stale(&self) -> bool {
        matches!(self.kind, Kind::Agent { .. }) && !self.hooks_live
    }

    /// The directory this session is in *now*.
    ///
    /// Falls back to the launch directory when the live one is unknown, which
    /// is also what it is before the first refresh. Everything that asks where
    /// a session is — the card, the branch, the diff, the state file — goes
    /// through here, so tracking `cd` was a matter of changing one accessor.
    pub fn directory(&self) -> &Path {
        self.live_cwd.as_deref().unwrap_or(&self.spec.cwd)
    }

    /// Re-reads where the child has got to.
    ///
    /// A subprocess on macOS, so this belongs on a timer. See `crate::cwd`.
    pub fn refresh_directory(&mut self) {
        if let Some(found) = crate::cwd::of(self.pty.child_pid()) {
            self.live_cwd = Some(found);
        }
    }

    /// Re-reads the branch. Cheap, but not free — call on a timer, not a frame.
    pub fn refresh_branch(&mut self) {
        self.branch = crate::worktree::branch_of(self.directory());
    }

    /// Enters copy mode: a cursor you drive with the keyboard, over the
    /// session's own scrollback.
    ///
    /// **Why this exists.** Houston asks the terminal to report the mouse so
    /// the wheel can scroll a session's history, and a terminal reporting the
    /// mouse hands drags to the application instead of using them for its own
    /// selection. So the native click-and-drag stops working, and there is
    /// nothing Houston can do about that from inside — the terminal has
    /// already decided.
    ///
    /// The VT layer has had the machinery all along: alacritty's vi mode moves
    /// a cursor through the grid, its selection tracks a range, and
    /// `selection_to_string` reassembles the text with wide characters and
    /// wrapped lines handled properly. This wires those to keys.
    pub fn enter_copy_mode(&self) {
        let Ok(mut term) = self.pty.term().lock() else { return };
        if !term.mode().contains(TermMode::VI) {
            term.toggle_vi_mode();
        }
        term.selection = None;
    }

    /// Leaves copy mode, dropping any selection.
    pub fn exit_copy_mode(&self) {
        let Ok(mut term) = self.pty.term().lock() else { return };
        if term.mode().contains(TermMode::VI) {
            term.toggle_vi_mode();
        }
        term.selection = None;
    }

    /// Moves the copy cursor, extending the selection if one is running.
    pub fn copy_motion(&self, motion: ViMotion) {
        let Ok(mut term) = self.pty.term().lock() else { return };
        term.vi_motion(motion);

        // A live selection follows the cursor. Alacritty keeps the two
        // independent, so nothing extends unless we say so.
        let point = term.vi_mode_cursor.point;
        if let Some(selection) = term.selection.as_mut() {
            selection.update(point, Side::Left);
            // Both end cells included, the way vim's visual mode behaves:
            // `v` then four `l` selects five characters, not four. `update`
            // alone leaves the cursor's own cell out, which reads as the
            // selection lagging a column behind the cursor.
            selection.include_all();
        }
    }

    /// Starts a selection at the cursor, or drops the one in progress.
    ///
    /// Returns whether there is now a selection, so the footer can say which
    /// of the two things `v` will do next.
    pub fn toggle_selection(&self) -> bool {
        let Ok(mut term) = self.pty.term().lock() else { return false };

        if term.selection.is_some() {
            term.selection = None;
            return false;
        }

        let point = term.vi_mode_cursor.point;
        term.selection = Some(Selection::new(SelectionType::Simple, point, Side::Left));
        true
    }

    /// The selected text, if any.
    ///
    /// Empty selections come back as `None`: a stray `v` followed by `y`
    /// should say "nothing selected", not silently replace your clipboard
    /// with an empty string.
    #[must_use]
    pub fn selected_text(&self) -> Option<String> {
        let text = self.pty.term().lock().ok()?.selection_to_string()?;
        (!text.trim().is_empty()).then_some(text)
    }

    /// Whether the child has asked to handle the mouse itself.
    ///
    /// When it has, drags belong to it and Houston must keep its hands off —
    /// a full-screen program with its own click handling would be unusable
    /// otherwise.
    #[must_use]
    pub fn wants_mouse(&self) -> bool {
        self.pty.mode().intersects(
            TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG,
        )
    }

    /// Starts a mouse selection at a screen position within the pane.
    ///
    /// `clicks` picks what a single gesture means, the way every terminal
    /// does it: one selects characters, two a word, three a whole line.
    /// Alacritty knows where words and lines begin, including across a wrap,
    /// so none of that is decided here.
    pub fn begin_mouse_selection(&self, row: u16, column: u16, clicks: u8) {
        let Ok(mut term) = self.pty.term().lock() else { return };

        let point = crate::ui::terminal::grid_point(row, column, term.grid().display_offset());
        let kind = match clicks {
            2 => SelectionType::Semantic,
            3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        };

        let mut selection = Selection::new(kind, point, Side::Left);
        // A word or line selection is complete the moment it starts: there is
        // nothing to drag out, and leaving it empty until the mouse moves
        // would make a double-click do nothing at all.
        if clicks > 1 {
            selection.update(point, Side::Right);
        }
        term.selection = Some(selection);
    }

    /// Extends the running mouse selection.
    pub fn drag_mouse_selection(&self, row: u16, column: u16) {
        let Ok(mut term) = self.pty.term().lock() else { return };

        let point = crate::ui::terminal::grid_point(row, column, term.grid().display_offset());
        if let Some(selection) = term.selection.as_mut() {
            selection.update(point, Side::Left);
            selection.include_all();
        }
    }

    /// Drops any selection. Called when typing, so the highlight does not
    /// linger over output that has since scrolled away.
    pub fn clear_selection(&self) {
        if let Ok(mut term) = self.pty.term().lock() {
            term.selection = None;
        }
    }

    /// Writes text straight into the VT, as though the child had printed it.
    ///
    /// Test-only. The grid is otherwise filled by the reader thread from a
    /// real child, which means a test wanting known text on screen would have
    /// to write to a shell and wait for it — timing-dependent, and the thing
    /// under test here is the selection, not the pty.
    #[cfg(test)]
    pub fn feed(&self, text: &str) {
        use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

        // Through the real parser rather than pushing characters in. Escape
        // sequences have to *mean* something here: a test that a child asking
        // for mouse reporting keeps its drags is worthless if `\x1b[?1000h`
        // arrives as six printable characters.
        let Ok(mut term) = self.pty.term().lock() else { return };
        let mut parser: Processor<StdSyncHandler> = Processor::new();
        parser.advance(&mut *term, text.as_bytes());
    }

    /// Whether this session is in copy mode.
    ///
    /// Test-only: the app tracks copy mode by session id, and this exists to
    /// check that leaving it really reaches the session's own VT state rather
    /// than only clearing Houston's flag.
    #[cfg(test)]
    #[must_use]
    pub fn is_copying(&self) -> bool {
        self.pty.term().lock().is_ok_and(|term| term.mode().contains(TermMode::VI))
    }

    /// Re-counts what has changed in its directory.
    pub fn refresh_changes(&mut self) {
        self.changes = crate::diff::changes_in(self.directory());
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

    /// Resolves a conversation id for agents that do not report one via hooks.
    ///
    /// Claude Code hands Houston its `session_id` in every hook payload. Codex
    /// does not, but it writes a rollout file recording both its id and its
    /// working directory, so the id can be found from disk instead.
    ///
    /// Cheap and only called when state is saved, never per frame.
    pub fn resolve_conversation(&mut self) {
        if self.conversation.is_some() || !matches!(self.kind, Kind::Agent { .. }) {
            return;
        }
        if self.spec.command.as_deref() != Some("codex") {
            return;
        }
        if let Some(id) = provider::codex_conversation(&self.spec.cwd, self.started_at) {
            self.conversation = Some(id);
        }
    }

    /// Records the agent's conversation id, the first time it reports one.
    pub fn remember_conversation(&mut self, id: &str) -> bool {
        if id.is_empty() || self.conversation.as_deref() == Some(id) {
            return false;
        }
        self.conversation = Some(id.to_string());
        true
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

    /// Selects the live session working in a worktree.
    ///
    /// Returns `false` if nothing is, which is the common case for a worktree
    /// somebody has finished with.
    pub fn select_by_worktree(&mut self, name: &str) -> bool {
        let found = self.items.iter().position(|session| {
            session.worktree.as_deref() == Some(name) && !matches!(session.state, State::Exited(_))
        });

        if let Some(index) = found {
            self.selected = index;
        }
        found.is_some()
    }

    /// The session with this id, if it is still live.
    pub fn by_id_mut(&mut self, id: u64) -> Option<&mut Session> {
        self.items.iter_mut().find(|item| item.id.0 == id)
    }

    /// Routes a hook notification to the session that raised it.
    ///
    /// Returns `true` if it matched a live session.
    pub fn apply_hook(&mut self, session: u64, kind: HookKind, conversation: Option<&str>) -> bool {
        let Some(target) = self.items.iter_mut().find(|item| item.id.0 == session) else {
            return false;
        };
        target.apply_hook(kind);
        if let Some(id) = conversation {
            target.remember_conversation(id);
        }
        true
    }

    /// Reopens a session Houston remembered from last time.
    ///
    /// An agent with a recorded conversation, whose provider supports it,
    /// comes back as a continuation. Everything else comes back in the right
    /// directory with the right name, which is the honest remainder.
    pub fn restore(&mut self, remembered: &crate::state::Remembered, size: Size) -> Result<()> {
        let (kind, spec) = restore_spec(remembered)?;
        let id = self.spawn(remembered.name.clone(), kind, spec, size)?;

        if let Some(session) = self.items.iter_mut().find(|item| item.id == id) {
            session.worktree.clone_from(&remembered.worktree);
            session.conversation.clone_from(&remembered.conversation);
            if remembered.named_by_user {
                session.rename(Some(remembered.name.clone()));
            }
        }

        // Restored agents get their hooks back, or the board would never hear
        // from them again.
        if remembered.agent {
            let _ = install_hooks(&remembered.cwd, id);
        }
        Ok(())
    }

    /// Starts an agent session using the first available provider.
    ///
    /// Also wires Claude Code's hooks into the project so the board can tell
    /// whether the agent is working or waiting on you. A failure to install
    /// hooks is reported but does not stop the session: a working agent with
    /// no status is far better than no agent.
    pub fn spawn_agent(&mut self, cwd: &Path, size: Size) -> Result<SessionId> {
        let (kind, spec) = agent_launch(cwd);
        self.spawn_agent_as(kind, spec, cwd, size)
    }

    /// Spawns an agent session with the kind already decided.
    ///
    /// Split from [`Self::spawn_agent`] for the same reason `restore_spec` is
    /// split from `restore`: everything interesting here — displacing another
    /// agent's hooks, marking it stale, warning about it — happens only when
    /// the kind is `Agent`, and the kind depends on whether a coding agent is
    /// installed on this machine. The collision test used to call the
    /// detecting version, so it passed on a laptop with Claude Code and failed
    /// on CI, where the fallback to a shell meant no collision ever happened.
    fn spawn_agent_as(
        &mut self,
        kind: Kind,
        spec: LaunchSpec,
        cwd: &Path,
        size: Size,
    ) -> Result<SessionId> {
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
            started_at: std::time::SystemTime::now(),
            conversation: None,
            live_cwd: None,
            hooks_live: true,
            branch: crate::worktree::branch_of(&spec.cwd),
            changes: crate::diff::changes_in(&spec.cwd),
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
    /// `Pty::drop` sends SIGHUP and then **blocks in `child.wait()`**. A child
    /// that is slow to die — or ignores SIGHUP — would therefore freeze the
    /// whole interface, so the drop happens on its own thread.
    pub fn close_selected(&mut self) {
        if self.items.is_empty() {
            return;
        }
        reap(self.items.remove(self.selected));
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

    /// Tears every session down without blocking.
    ///
    /// Called on the way out. Dropping these inline would block the exit path
    /// *before* the terminal is restored, so a stuck child would leave the
    /// user in raw mode with no prompt.
    pub fn shutdown(&mut self) {
        for session in self.items.drain(..) {
            reap(session);
        }
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
            // Directory first: the branch is read from whatever directory the
            // session is in, so looking it up before moving would report the
            // branch of where you used to be.
            session.refresh_directory();
            session.refresh_branch();
        }
    }

    /// Re-counts changes for the selected session only.
    ///
    /// One session rather than all of them, so the cost is a constant two git
    /// subprocesses per tick however many agents are running. The others are
    /// kept current by their hooks; see [`Session::changes`].
    pub fn refresh_selected_changes(&mut self) {
        if let Some(session) = self.selected_mut() {
            session.refresh_changes();
        }
    }

    /// Re-reads where every session has got to.
    ///
    /// A subprocess per session on macOS, so this is for the moments that
    /// matter — quitting — rather than for the frame loop. The timer version
    /// is folded into `refresh_branches`.
    pub fn refresh_directories(&mut self) {
        for session in &mut self.items {
            session.refresh_directory();
        }
    }

    /// Fills in conversation ids for agents that do not report them via hooks.
    pub fn resolve_conversations(&mut self) {
        for session in &mut self.items {
            session.resolve_conversation();
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

/// Builds what a remembered session should be launched as.
///
/// Separate from `restore` so the resume logic can be tested without spawning
/// a real agent — a test that launches `claude` leaves a live process behind,
/// and tearing it down means killing by name, which is a good way to kill
/// somebody's actual work.
pub fn restore_spec(remembered: &crate::state::Remembered) -> Result<(Kind, LaunchSpec)> {
    if !remembered.cwd.is_dir() {
        anyhow::bail!("{} is gone", remembered.cwd.display());
    }

    if !remembered.agent {
        return Ok((
            Kind::Shell,
            LaunchSpec::command(provider::login_shell(), Vec::new(), remembered.cwd.clone()),
        ));
    }

    let command = remembered
        .command
        .clone()
        .or_else(|| provider::default().map(|p| p.command.to_string()))
        .context("no agent available to restore into")?;

    let provider = provider::by_command(&command);
    let label = provider.map_or("agent", |p| p.label);

    // Only resume when the provider has a checked contract for it. Guessing an
    // argument would make the agent fail to launch at all.
    let args = match (provider.and_then(|p| p.resume_arg), &remembered.conversation) {
        (Some(argument), Some(id)) => vec![argument.to_string(), id.clone()],
        _ => Vec::new(),
    };

    Ok((
        Kind::Agent { provider: label },
        LaunchSpec::command(command, args, remembered.cwd.clone()),
    ))
}

/// Drops a session off the interface thread.
///
/// The thread is detached deliberately: the child has been signalled, and
/// whether it takes a millisecond or a minute to go is not something the user
/// should have to wait through.
fn reap(session: Session) {
    let _ = std::thread::Builder::new().name("houston-reaper".into()).spawn(move || drop(session));
}

/// Wires Houston's hooks into a project's Claude Code settings.
fn install_hooks(cwd: &Path, id: SessionId) -> Result<()> {
    let socket = hooks::socket_path()?;
    hooks::install(cwd, id, &socket)?;
    Ok(())
}

/// A short label for a working directory: its last component.
/// What to launch for a new agent session on this machine.
///
/// Falls back to a shell when no agent CLI is installed, rather than failing:
/// a terminal in the right directory is more use than an error.
fn agent_launch(cwd: &Path) -> (Kind, LaunchSpec) {
    provider::default().map_or_else(
        || {
            (
                Kind::Shell,
                LaunchSpec::command(provider::login_shell(), Vec::new(), cwd.to_path_buf()),
            )
        },
        |provider: Provider| {
            (Kind::Agent { provider: provider.label }, provider.launch(cwd.to_path_buf()))
        },
    )
}

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
        assert!(sessions.apply_hook(id.0, HookKind::Working, None));
        assert_eq!(sessions.selected().unwrap().state, State::Running);

        // It asks permission and stops dead.
        assert!(sessions.apply_hook(id.0, HookKind::Attention, None));
        assert_eq!(sessions.selected().unwrap().state, State::AwaitingInput);

        // You approve; the tool runs and reports back. Without this the
        // session would sit on "needs you" for the whole of the work.
        assert!(sessions.apply_hook(id.0, HookKind::Working, None));
        assert_eq!(sessions.selected().unwrap().state, State::Running);

        // The turn ends. This is the one that used to say "running" and pin
        // the board on busy until you next typed.
        assert!(sessions.apply_hook(id.0, HookKind::Idle, None));
        assert_eq!(sessions.selected().unwrap().state, State::Idle);
    }

    #[test]
    fn several_turns_in_a_row_do_not_drift() {
        let mut sessions = sessions_with_one_shell();
        let id = sessions.selected().unwrap().id;

        for _ in 0..5 {
            sessions.apply_hook(id.0, HookKind::Working, None);
            sessions.apply_hook(id.0, HookKind::Attention, None);
            sessions.apply_hook(id.0, HookKind::Working, None);
            sessions.apply_hook(id.0, HookKind::Idle, None);
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
        assert!(!sessions.apply_hook(9999, HookKind::Attention, None));
        assert_eq!(sessions.selected().unwrap().state, State::Running);
    }

    #[test]
    fn a_hook_cannot_resurrect_an_exited_session() {
        let mut sessions = sessions_with_one_shell();
        let id = sessions.selected().unwrap().id;
        sessions.selected_mut().unwrap().state = State::Exited(Some(0));

        sessions.apply_hook(id.0, HookKind::Attention, None);
        assert_eq!(
            sessions.selected().unwrap().state,
            State::Exited(Some(0)),
            "exit is the more truthful state; a late hook must not override it"
        );
    }
}

#[cfg(test)]
mod restore_tests {
    use super::*;
    use crate::state::Remembered;

    fn remembered(cwd: &Path) -> Remembered {
        Remembered {
            name: "notes".to_string(),
            named_by_user: false,
            cwd: cwd.to_path_buf(),
            agent: false,
            command: None,
            worktree: None,
            conversation: None,
        }
    }

    #[test]
    fn a_shell_comes_back_in_the_directory_it_was_in() {
        let mut sessions = Sessions::new();
        let cwd = std::env::temp_dir();

        sessions.restore(&remembered(&cwd), Size::new(24, 80)).unwrap();

        let session = sessions.selected().unwrap();
        assert_eq!(session.directory(), cwd);
        assert!(matches!(session.kind, Kind::Shell));
    }

    #[test]
    fn a_name_you_chose_survives_but_a_derived_one_is_recomputed() {
        let mut sessions = Sessions::new();
        let cwd = std::env::temp_dir();

        let mut named = remembered(&cwd);
        named.name = "queue runner".to_string();
        named.named_by_user = true;
        sessions.restore(&named, Size::new(24, 80)).unwrap();

        assert_eq!(sessions.selected().unwrap().display_name(), "queue runner");
    }

    #[test]
    fn a_directory_that_has_gone_is_reported_rather_than_silently_skipped() {
        let mut sessions = Sessions::new();
        let mut gone = remembered(Path::new("/tmp/houston-definitely-removed"));
        gone.name = "old work".to_string();

        assert!(restore_spec(&gone).is_err());
        assert!(sessions.restore(&gone, Size::new(24, 80)).is_err());
        assert!(sessions.is_empty(), "nothing half-created");
    }

    /// The whole point: a remembered conversation becomes `--resume <id>`.
    ///
    /// Checked on the launch spec rather than by launching, so no real agent
    /// is started and nothing has to be killed afterwards.
    /// The bug this fixes: a shell is launched somewhere, you `cd` out of it,
    /// and quitting remembered where it started rather than where it got to.
    #[test]
    fn a_session_reports_where_it_has_got_to_not_where_it_started() {
        let mut sessions = Sessions::new();
        let launched_in = std::env::temp_dir();
        sessions.spawn_shell(&launched_in, Size::new(24, 80)).unwrap();

        let session = sessions.selected().unwrap();
        assert_eq!(session.directory(), launched_in, "before anyone looks, the launch directory");

        // As though the shell had been `cd`'d.
        let moved_to = launched_in.join("houston-moved-here");
        std::fs::create_dir_all(&moved_to).unwrap();
        sessions.selected_mut().unwrap().live_cwd = Some(moved_to.clone());

        assert_eq!(
            sessions.selected().unwrap().directory(),
            moved_to,
            "everything asks through this accessor, so everything follows"
        );

        let state = crate::state::State::capture(&sessions);
        assert_eq!(
            state.sessions[0].cwd, moved_to,
            "and what gets written is where you would want to come back to"
        );

        std::fs::remove_dir_all(&moved_to).ok();
    }

    /// "Cannot tell" is not "moved back to the start".
    #[test]
    fn a_failed_look_leaves_the_directory_alone() {
        let mut sessions = Sessions::new();
        sessions.spawn_shell(&std::env::temp_dir(), Size::new(24, 80)).unwrap();

        let known = std::env::temp_dir().join("houston-known-cwd");
        std::fs::create_dir_all(&known).unwrap();
        sessions.selected_mut().unwrap().live_cwd = Some(known.clone());

        // The child is alive, so this succeeds and overwrites — but the point
        // is the shape: `refresh_directory` only ever assigns on success.
        sessions.selected_mut().unwrap().refresh_directory();
        assert!(
            sessions.selected().unwrap().live_cwd.is_some(),
            "a refresh never clears what it already knew"
        );

        std::fs::remove_dir_all(&known).ok();
    }

    #[test]
    fn an_agent_with_a_conversation_is_resumed_rather_than_restarted() {
        let mut agent = remembered(&std::env::temp_dir());
        agent.agent = true;
        agent.command = Some("claude".to_string());
        agent.conversation = Some("conv-abc".to_string());

        let (kind, spec) = restore_spec(&agent).unwrap();
        assert!(matches!(kind, Kind::Agent { .. }));
        assert_eq!(spec.args, vec!["--resume".to_string(), "conv-abc".to_string()]);
    }

    /// Codex spells resume as a subcommand rather than a flag, and both must
    /// come out as "one token, then the id".
    #[test]
    fn codex_resumes_with_its_subcommand_not_a_flag() {
        let mut agent = remembered(&std::env::temp_dir());
        agent.agent = true;
        agent.command = Some("codex".to_string());
        agent.conversation = Some("019fc86d-8f05-7e22-baf8-549ce51f0b27".to_string());

        let (_, spec) = restore_spec(&agent).unwrap();
        assert_eq!(
            spec.args,
            vec!["resume".to_string(), "019fc86d-8f05-7e22-baf8-549ce51f0b27".to_string()],
            "codex resume <id>, not codex --resume <id>"
        );
    }

    #[test]
    fn an_agent_without_a_conversation_starts_fresh() {
        let mut agent = remembered(&std::env::temp_dir());
        agent.agent = true;
        agent.command = Some("claude".to_string());

        let (_, spec) = restore_spec(&agent).unwrap();
        assert!(spec.args.is_empty(), "nothing to resume into");
    }

    /// Guessing a resume flag for an agent whose contract has not been checked
    /// would make it fail to launch at all.
    #[test]
    fn a_provider_without_a_resume_contract_starts_fresh_even_with_an_id() {
        let mut agent = remembered(&std::env::temp_dir());
        agent.agent = true;
        agent.command = Some("gemini".to_string());
        agent.conversation = Some("conv-abc".to_string());

        let (_, spec) = restore_spec(&agent).unwrap();
        assert!(spec.args.is_empty(), "no guessed flags");
    }

    /// A rollout from before the session started belongs to a different
    /// conversation, however tempting its directory looks.
    #[test]
    fn a_codex_rollout_older_than_the_session_is_not_adopted() {
        use std::time::{Duration, SystemTime};

        let cwd = std::env::temp_dir();
        let future = SystemTime::now() + Duration::from_secs(3600);

        assert!(
            crate::provider::codex_conversation(&cwd, future).is_none(),
            "nothing can have been written after now plus an hour"
        );
    }

    #[test]
    fn a_conversation_id_is_recorded_once_and_not_churned() {
        let mut sessions = Sessions::new();
        sessions.spawn_shell(&std::env::temp_dir(), Size::new(24, 80)).unwrap();
        let session = sessions.selected_mut().unwrap();

        assert!(session.remember_conversation("abc"));
        assert!(!session.remember_conversation("abc"), "the same id is not a change");
        assert!(!session.remember_conversation(""), "an empty id is not an id");
        assert_eq!(session.conversation.as_deref(), Some("abc"));
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
            started_at: std::time::SystemTime::now(),
            conversation: None,
            changes: None,
            live_cwd: None,
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
            started_at: std::time::SystemTime::now(),
            conversation: None,
            changes: None,
            live_cwd: None,
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
        //
        // Spawned as an agent explicitly rather than by detection. A CI runner
        // has no coding agent installed, so `spawn_agent` would fall back to a
        // shell there — and a shell displaces nobody, so this test would only
        // ever have passed on a developer's laptop.
        sessions
            .spawn_agent_as(
                Kind::Agent { provider: "Claude Code" },
                LaunchSpec::command(provider::login_shell(), Vec::new(), directory.clone()),
                &directory,
                Size::new(24, 80),
            )
            .unwrap();

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
