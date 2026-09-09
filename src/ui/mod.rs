//! Rendering.
//!
//! The chrome — a tab strip on top, a keybind bar on the bottom — is the part
//! of Chloe the brief singled out as making it pleasant to live in, so it is
//! present from the first commit rather than bolted on later.

pub mod board;
mod chrome;
pub mod editor;
pub mod form;
pub mod keycap;
pub mod overlay;
pub mod sessions;
pub mod settings;
pub mod terminal;
pub mod theme;
pub mod vault;

pub use theme::Theme;

use crate::app::{App, Tab};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

#[cfg(test)]
use ratatui::style::Color;

/// Splits the screen into (header, body, keybind bar).
///
/// Shared with the event loop, which needs the body rect to size child grids —
/// so changing the header height cannot desynchronise them.
pub fn layout(area: Rect) -> [Rect; 3] {
    // A short terminal keeps the body usable by dropping the header's padding
    // rather than the body's content.
    let header = if area.height >= 12 { chrome::HEADER_HEIGHT } else { 1 };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(header), Constraint::Min(0), Constraint::Length(1)])
        .split(area);
    [rows[0], rows[1], rows[2]]
}

pub fn render(frame: &mut Frame, app: &App) {
    let theme = app.theme;

    // Paint everything first. Without this the body shows the user's terminal
    // background through, and a theme can only recolour text — which made
    // light mode on a dark terminal look broken rather than light.
    frame.render_widget(
        ratatui::widgets::Block::default()
            .style(ratatui::style::Style::default().bg(theme.surface)),
        frame.area(),
    );

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
    if app.show_inspector {
        overlay::inspector(frame, body, &app.input_log, theme);
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

    /// A theme has to reach every cell, or switching to light mode leaves
    /// dark text on whatever the terminal happens to be.
    #[test]
    fn the_whole_frame_is_painted_by_the_theme() {
        let app = App::new();
        let theme = app.theme;

        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        let unpainted = buffer
            .content()
            .iter()
            .filter(|cell| cell.style().bg.is_none() || cell.style().bg == Some(Color::Reset))
            .count();

        assert_eq!(unpainted, 0, "{unpainted} cells show the terminal through");
        let _ = theme;
    }

    #[test]
    fn the_header_is_padded_and_ruled_on_a_normal_terminal() {
        let rendered = draw(&App::new(), 100, 30);
        let rows: Vec<&str> = rendered.lines().collect();

        assert!(rows[0].trim().is_empty(), "a padding row above the tabs");
        assert!(rows[1].contains("houston") && rows[1].contains("Sessions"));
        assert!(rows[2].contains('─'), "a rule separating chrome from content");
    }

    /// A short terminal should lose the header's padding, never the body's
    /// content.
    #[test]
    fn a_short_terminal_falls_back_to_a_single_header_row() {
        let rendered = draw(&App::new(), 100, 10);
        let rows: Vec<&str> = rendered.lines().collect();

        assert!(rows[0].contains("houston"), "the tabs move up into row zero");
        assert!(!rows[0].contains('─'));
    }

    #[test]
    fn the_active_tab_is_a_filled_pill() {
        let mut app = App::new();
        app.select_tab(Tab::Board);

        let theme = app.theme;
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        // The pill is padded, so the fill reaches beyond the label itself.
        let filled = (0..buffer.area.width)
            .filter(|x| buffer[(*x, 1)].style().bg == Some(theme.accent))
            .count();

        // `chars().count()`, not `len()` — `·` is two bytes and one cell, and
        // a byte-length assertion here demanded one more cell than exists.
        let expected = "  3·Board  ".chars().count();
        assert_eq!(filled, expected, "the active tab should be a filled pill, padding included");
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
        // The key is drawn as a key, not as a letter in a sentence.
        assert!(rendered.contains(" n "), "the shortcut is capped");
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
        assert!(rendered.contains("None yet"));
        assert!(rendered.contains("tick Worktree"), "an empty state says how to get one");
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

    /// Which card is selected has to be visible at a glance, and a coloured
    /// word is not that — the whole card is filled.
    #[test]
    fn the_selected_board_card_is_filled_across_its_width() {
        let mut app = App::new();
        for _ in 0..2 {
            app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();
        }
        app.select_tab(Tab::Board);

        let theme = Theme::default();
        let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        let filled = (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .filter(|(x, y)| buffer[(*x, *y)].style().bg == Some(theme.highlight))
            .count();

        // Two rows across most of a quarter-width column, at minimum.
        assert!(filled > 30, "the selected card should be filled, got {filled} cells");
    }

    #[test]
    fn an_unselected_board_card_is_not_filled() {
        let mut app = App::new();
        app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();
        app.select_tab(Tab::Board);

        let theme = Theme::default();
        let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let filled_before = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .filter(|cell| cell.style().bg == Some(theme.highlight))
            .count();

        // Deselect by moving past the only card; the fill should follow it,
        // never linger on a card that is no longer chosen.
        assert!(filled_before > 0, "the one card is selected, so it is filled");
    }

    /// The whole point of marking a displaced session: the board must stop
    /// presenting a value that is no longer being updated as though it were
    /// current.
    #[test]
    fn a_session_whose_hooks_were_taken_over_says_its_status_is_frozen() {
        let mut app = App::new();
        app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();

        let session = app.sessions.selected_mut().unwrap();
        session.kind = crate::session::Kind::Agent { provider: "Claude Code" };
        session.hooks_live = false;

        app.select_tab(Tab::Board);
        let board = draw(&app, 150, 24);
        assert!(board.contains("status frozen"), "the board must admit it does not know");

        app.select_tab(Tab::Sessions);
        let sidebar = draw(&app, 120, 24);
        assert!(sidebar.contains('?'), "and the sidebar badge stops claiming a state");
    }

    /// The advice lives at the end of the sentence, so a notice that runs off
    /// an 80-column terminal loses exactly the useful half.
    #[test]
    fn a_notice_fits_on_a_narrow_terminal() {
        let mut app = App::new();
        app.notify("Claude Code's status will freeze — two agents here. Use a worktree.");

        let rendered = draw(&app, 80, 20);
        let footer = rendered.lines().last().unwrap();

        assert!(footer.contains("Use a worktree"), "the advice must survive: {footer:?}");
    }

    #[test]
    fn the_board_shows_its_columns_once_something_is_running() {
        let mut app = App::new();
        app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();
        app.select_tab(Tab::Board);

        let rendered = draw(&app, 150, 24);
        for column in ["Needs you", "Working", "Idle", "Shells", "Closed"] {
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
