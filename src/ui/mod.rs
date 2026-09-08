//! Rendering.
//!
//! The chrome — a tab strip on top, a keybind bar on the bottom — is the part
//! of Chloe the brief singled out as making it pleasant to live in, so it is
//! present from the first commit rather than bolted on later.

pub mod board;
mod chrome;
pub mod editor;
pub mod form;
pub mod overlay;
pub mod sessions;
pub mod settings;
pub mod terminal;
mod theme;
pub mod vault;

pub use theme::Theme;

use crate::app::{App, Tab};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

/// Splits the screen into (tab strip, body, keybind bar).
///
/// Shared with the event loop, which needs the body rect to size child grids.
pub fn layout(area: Rect) -> [Rect; 3] {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
        .split(area);
    [rows[0], rows[1], rows[2]]
}

pub fn render(frame: &mut Frame, app: &App) {
    let theme = Theme::default();
    let [tabs, body, footer] = layout(frame.area());

    chrome::tab_strip(frame, tabs, app, theme);

    match app.tab {
        Tab::Sessions => {
            sessions::render(frame, body, &app.sessions, app.renaming.as_deref(), theme);
        }
        Tab::Vault => match &app.editor {
            Some(open) => editor::render(frame, body, open, theme),
            None => vault::render(frame, body, app.browser.as_ref(), theme),
        },
        Tab::Board => board::render(frame, body, &app.sessions, theme),
        Tab::Settings => settings::render(frame, body, app, theme),
    }

    // Above everything, in the order they stack.
    if let Some(open) = &app.form {
        overlay::form(frame, body, open, theme);
    }
    if let Some(list) = &app.worktrees {
        overlay::worktrees(frame, body, list, app.worktree_selected, &app.sessions, theme);
    }
    if let Some(picker) = &app.picker {
        overlay::picker(frame, body, picker, &app.sessions, theme);
    }

    chrome::keybind_bar(frame, footer, app, theme);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn draw(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn chrome_shows_every_tab_and_the_keybinds() {
        let rendered = draw(&App::new(), 100, 24);
        for tab in Tab::ALL {
            assert!(rendered.contains(tab.title()), "tab strip missing {}", tab.title());
        }
        assert!(rendered.contains("houston"));
        assert!(rendered.contains("quit"));
    }

    #[test]
    fn the_sessions_view_prompts_when_empty() {
        let rendered = draw(&App::new(), 100, 24);
        assert!(rendered.contains("No sessions yet"));
        assert!(rendered.contains("start an agent"));
    }

    #[test]
    fn body_follows_the_selected_tab() {
        let mut app = App::new();
        app.select_tab(Tab::Settings);
        let rendered = draw(&app, 100, 24);
        assert!(rendered.contains("detected"));
    }

    #[test]
    fn settings_renders_as_a_menu_of_rows() {
        let mut app = App::new();
        app.select_tab(Tab::Settings);

        let rendered = draw(&app, 110, 30);
        assert!(rendered.contains("Vault folder"));
        assert!(rendered.contains("New sessions start in"));
        assert!(rendered.contains('▸'), "one row is selected");
    }

    #[test]
    fn the_new_session_form_shows_its_fields() {
        let mut app = App::new();
        app.open_new_session_form();

        let rendered = draw(&app, 110, 30);
        assert!(rendered.contains("new session"));
        assert!(rendered.contains("Name"));
        assert!(rendered.contains("Directory"));
        assert!(rendered.contains("Worktree"));
    }

    #[test]
    fn the_worktree_manager_says_what_to_do_when_empty() {
        let mut app = App::new();
        app.worktrees = Some(Vec::new());

        let rendered = draw(&app, 110, 30);
        assert!(rendered.contains("worktrees"));
        assert!(rendered.contains("none yet"), "an empty state says how to get one");
    }

    #[test]
    fn the_session_chooser_lists_every_session_with_its_number() {
        let mut app = App::new();
        for _ in 0..2 {
            app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();
        }
        app.picker = Some(crate::app::Picker {
            prompt: "Send notes to which session?".to_string(),
            payload: "@/tmp/notes.md ".to_string(),
            selected: 1,
        });

        let rendered = draw(&app, 110, 26);
        assert!(rendered.contains("Send notes to which session?"));
        assert!(rendered.contains('1') && rendered.contains('2'), "numbered like the sidebar");
    }

    #[test]
    fn the_footer_asks_for_confirmation_once_quit_is_armed() {
        let mut app = App::new();
        app.quit_armed = true;

        let rendered = draw(&app, 100, 24);
        assert!(rendered.contains("press again to quit"));
    }

    #[test]
    fn renaming_turns_the_sidebar_row_into_a_field() {
        let mut app = App::new();
        app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();
        app.renaming = Some("auth refactor".to_string());

        let rendered = draw(&app, 110, 24);
        assert!(rendered.contains("auth refactor"), "the draft is edited in place");
    }

    #[test]
    fn the_board_shows_its_columns_once_something_is_running() {
        let mut app = App::new();
        app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();
        app.select_tab(Tab::Board);

        let rendered = draw(&app, 120, 24);
        for column in ["Needs you", "Working", "Shells", "Finished"] {
            assert!(rendered.contains(column), "board missing the {column} column");
        }
    }

    #[test]
    fn the_vault_view_says_how_to_fix_a_missing_vault() {
        let mut app = App::new();
        app.browser = None;
        app.select_tab(Tab::Vault);

        let rendered = draw(&app, 100, 24);
        assert!(rendered.contains("No vault found"));
        assert!(rendered.contains("HOUSTON_VAULT"), "the message must say how to fix it");
    }

    /// A terminal can legitimately be one row tall mid-resize, and every view
    /// must survive it. Panicking here would take down the whole app.
    #[test]
    fn survives_a_degenerate_viewport() {
        for tab in Tab::ALL {
            let mut app = App::new();
            app.select_tab(tab);
            for height in 0..=3 {
                for width in [0, 1, 30] {
                    let _ = draw(&app, width, height);
                }
            }
        }
    }
}
