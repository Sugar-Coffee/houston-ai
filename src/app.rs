//! Application state.

use crate::{
    pty::Size,
    session::{Focus, Sessions},
};
use std::path::PathBuf;

/// The top-level views, rendered as the tab strip.
///
/// Mirrors `docs/roadmap.md`: Sessions (Phase 2), Vault (Phase 4),
/// Board (Phase 6), Settings (Phase 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Sessions,
    Vault,
    Board,
    Settings,
}

impl Tab {
    pub const ALL: [Self; 4] = [Self::Sessions, Self::Vault, Self::Board, Self::Settings];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Sessions => "Sessions",
            Self::Vault => "Vault",
            Self::Board => "Board",
            Self::Settings => "Settings",
        }
    }

    /// Placeholder copy for views that are not built yet. Names the roadmap
    /// phase, so the app itself says what is missing.
    pub const fn placeholder(self) -> (&'static str, &'static str) {
        match self {
            Self::Sessions => ("Sessions", ""),
            Self::Vault => ("Vault", "Phase 4 — browse, search and follow wikilinks"),
            Self::Board => ("Board", "Phase 6 — which agents are blocked on you"),
            Self::Settings => ("Settings", "Phase 8 — providers, vault path, theme"),
        }
    }
}

pub struct App {
    pub tab: Tab,
    pub sessions: Sessions,
    /// Where new sessions start. The directory Houston was launched from.
    pub cwd: PathBuf,
    pub should_quit: bool,
    /// Set whenever state changes in a way that needs a redraw. The render loop
    /// is frame-budgeted, so this coalesces bursts of events into one draw.
    pub dirty: bool,
    /// Shown in the footer when an action cannot be carried out.
    pub notice: Option<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            tab: Tab::Sessions,
            sessions: Sessions::new(),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            should_quit: false,
            dirty: true,
            notice: None,
        }
    }

    /// Whether keystrokes belong to a child rather than to Houston.
    pub fn is_attached(&self) -> bool {
        self.tab == Tab::Sessions && self.sessions.focus() == Focus::Attached
    }

    pub fn select_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.dirty = true;
        }
    }

    pub fn cycle_tab(&mut self, forward: bool) {
        let count = Tab::ALL.len();
        let current = Tab::ALL.iter().position(|tab| *tab == self.tab).unwrap_or(0);
        let next = if forward { (current + 1) % count } else { (current + count - 1) % count };
        self.select_tab(Tab::ALL[next]);
    }

    pub const fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn notify(&mut self, message: impl Into<String>) {
        self.notice = Some(message.into());
        self.dirty = true;
    }

    pub fn clear_notice(&mut self) {
        if self.notice.take().is_some() {
            self.dirty = true;
        }
    }

    /// Keeps every child's grid matched to the area it is drawn into.
    pub fn resize_sessions(&mut self, size: Size) {
        self.sessions.resize_all(size);
    }

    /// The keybinds the footer should show, given the current view and mode.
    pub fn keybinds(&self) -> Vec<(&'static str, &'static str)> {
        if self.is_attached() {
            return vec![("ctrl+\\", "detach"), ("", "all other keys go to the session")];
        }

        let mut binds: Vec<(&'static str, &'static str)> = vec![("tab", "view"), ("1-4", "jump")];

        if self.tab == Tab::Sessions {
            binds.extend([("n", "agent"), ("s", "shell")]);
            if !self.sessions.is_empty() {
                binds.extend([("j/k", "select"), ("↵", "attach"), ("x", "close")]);
            }
        }

        binds.push(("q", "quit"));
        binds
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_cycling_wraps_in_both_directions() {
        let mut app = App::new();
        assert_eq!(app.tab, Tab::Sessions);

        app.cycle_tab(false);
        assert_eq!(app.tab, Tab::Settings, "cycling back from the first tab should wrap");

        app.cycle_tab(true);
        assert_eq!(app.tab, Tab::Sessions, "cycling forward from the last tab should wrap");
    }

    #[test]
    fn selecting_the_current_tab_does_not_dirty_the_frame() {
        let mut app = App::new();
        app.dirty = false;

        app.select_tab(Tab::Sessions);
        assert!(!app.dirty, "re-selecting the active tab should not force a redraw");

        app.select_tab(Tab::Board);
        assert!(app.dirty, "changing tab should force a redraw");
    }

    #[test]
    fn keybinds_follow_the_view() {
        let mut app = App::new();

        let sessions: Vec<_> = app.keybinds().iter().map(|(key, _)| *key).collect();
        assert!(sessions.contains(&"n"), "the sessions view offers a new agent");
        assert!(!sessions.contains(&"x"), "with no sessions there is nothing to close");

        app.select_tab(Tab::Vault);
        let vault: Vec<_> = app.keybinds().iter().map(|(key, _)| *key).collect();
        assert!(!vault.contains(&"n"), "session keys must not leak into other views");
        assert!(vault.contains(&"q"));
    }

    #[test]
    fn an_unattached_app_is_never_treated_as_attached() {
        let app = App::new();
        assert!(!app.is_attached(), "with no sessions there is nothing to be attached to");
    }
}
