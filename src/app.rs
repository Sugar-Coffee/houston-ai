//! Application state.
//!
//! Deliberately thin for now. The domain model that matters is described in
//! ADR-0006 (a session is a PTY plus a launch spec) and arrives in Phase 2.

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

    /// Placeholder copy until each view lands. Names the roadmap phase so the
    /// app itself says what is not built yet.
    pub const fn placeholder(self) -> (&'static str, &'static str) {
        match self {
            Self::Sessions => ("Sessions", "Phase 2 — agent and shell sessions on one PTY path"),
            Self::Vault => ("Vault", "Phase 4 — browse, search and follow wikilinks"),
            Self::Board => ("Board", "Phase 6 — which agents are blocked on you"),
            Self::Settings => ("Settings", "Phase 8 — providers, vault path, theme"),
        }
    }
}

#[derive(Debug)]
pub struct App {
    pub tab: Tab,
    pub should_quit: bool,
    /// Set to `true` whenever state changes in a way that needs a redraw.
    /// The render loop is frame-budgeted, so this coalesces bursts of events
    /// into a single draw rather than one draw per event.
    pub dirty: bool,
}

impl App {
    pub const fn new() -> Self {
        Self { tab: Tab::Sessions, should_quit: false, dirty: true }
    }

    pub fn select_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.dirty = true;
        }
    }

    pub fn cycle_tab(&mut self, forward: bool) {
        let count = Tab::ALL.len();
        let current = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        let next = if forward { (current + 1) % count } else { (current + count - 1) % count };
        self.select_tab(Tab::ALL[next]);
    }

    pub const fn quit(&mut self) {
        self.should_quit = true;
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
    fn every_tab_has_placeholder_copy() {
        for tab in Tab::ALL {
            let (title, detail) = tab.placeholder();
            assert!(!title.is_empty());
            assert!(!detail.is_empty());
        }
    }
}
