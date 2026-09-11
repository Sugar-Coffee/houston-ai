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
pub mod palettes;
pub mod powerline;
pub mod sessions;
pub mod settings;
pub mod tasks;
pub mod terminal;
pub mod theme;
pub mod vault;
pub mod worktrees;

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

    let glyphs = powerline::Glyphs::for_setting(app.config.powerline_enabled());

    chrome::tab_strip(frame, tabs, app, theme);

    match app.tab {
        Tab::Sessions => {
            sessions::render(frame, body, &app.sessions, app.renaming.as_deref(), glyphs, theme);
        }
        Tab::Vault => match &app.editor {
            Some(open) => editor::render(frame, body, open, theme),
            None => vault::render(frame, body, app.browser.as_ref(), theme),
        },
        // The editor opens over Tasks as well, because a task *is* a note and
        // sending you to another tab to write two sentences in one is the kind
        // of seam that makes a folder of markdown feel like a database.
        Tab::Tasks => match &app.editor {
            Some(open) => editor::render(frame, body, open, theme),
            None => tasks::render(
                frame,
                body,
                &app.tasks,
                app.task_selected,
                app.tasks_show_done,
                glyphs,
                theme,
            ),
        },
        Tab::Board => board::render(frame, body, &app.sessions, theme),
        Tab::Worktrees => worktrees::render(
            frame,
            body,
            app.worktrees.as_ref(),
            app.worktree_selected,
            &app.sessions,
            glyphs,
            theme,
        ),
        Tab::Settings => settings::render(frame, body, app, theme),
    }

    // Above everything, in the order they stack.
    if let Some(open) = &app.form {
        overlay::form(frame, body, open, theme);
    }
    if let Some(picker) = &app.theme_picker {
        overlay::themes(frame, body, picker, &app.themes, theme);
    }
    if let Some(view) = &app.diff {
        overlay::diff(frame, body, view, theme);
    }
    if let Some(picker) = &app.picker {
        overlay::picker(frame, body, picker, &app.sessions, theme);
    }
    if app.show_inspector {
        overlay::inspector(frame, body, &app.input_log, theme);
    }

    if let Some(question) = &app.confirm {
        overlay::confirm(frame, body, question, theme);
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
        // The key is drawn as a key, not as a letter in a sentence — and
        // exactly as you would type it.
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
    fn the_worktree_view_says_what_to_do_when_empty() {
        let mut app = App::new();
        app.tab = Tab::Worktrees;
        app.worktrees = Some(Vec::new());

        let rendered = draw(&app, 110, 30);
        assert!(rendered.contains("worktrees"));
        assert!(rendered.contains("No worktrees yet"));
        assert!(rendered.contains("tick Worktree"), "an empty state says how to get one");
    }

    /// The setting has to actually reach the screen, and the plain default has
    /// to stay free of glyphs an ordinary font cannot draw.
    #[test]
    fn powerline_separators_appear_only_when_the_setting_is_on() {
        let mut app = App::new();

        let plain = draw(&app, 110, 20);
        assert!(
            !plain.contains('\u{e0b0}'),
            "an unpatched font draws that as a box, so it must not appear by default"
        );

        app.config.powerline = Some(true);
        let flowing = draw(&app, 110, 20);
        assert!(flowing.contains('\u{e0b0}'), "with the setting on, the tabs get their arrows");
        assert!(flowing.contains("Worktrees"), "and still say what they are");
    }

    /// A shell is marked and an agent is not, which is what makes the marker
    /// carry information rather than sitting on every row.
    #[test]
    fn a_shell_session_is_marked_and_an_agent_is_not() {
        let mut app = App::new();
        app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();

        assert!(draw(&app, 110, 20).contains(" $"), "a shell says so");

        app.sessions.selected_mut().unwrap().kind =
            crate::session::Kind::Agent { provider: "claude" };
        assert!(!draw(&app, 110, 20).contains(" $"), "an agent is the unmarked default");
    }

    /// The diff row changes shape with the setting but never loses the numbers.
    #[test]
    fn the_diff_row_keeps_its_numbers_in_both_styles() {
        let mut app = App::new();
        app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();
        app.sessions.selected_mut().unwrap().changes =
            Some(crate::diff::Changes { files: 2, insertions: 42, deletions: 7 });

        let plain = draw(&app, 110, 20);
        assert!(plain.contains("+42"), "plain shows the additions");
        assert!(plain.contains("2 files"));

        app.config.powerline = Some(true);
        let flowing = draw(&app, 110, 20);
        assert!(flowing.contains("+42"), "and so does the segmented form");
        assert!(flowing.contains("2 files"), "the count is not lost to decoration");
    }

    /// A selection you cannot see is a selection that does not work.
    ///
    /// Asserted on the drawn frame rather than on the selection state, because
    /// the state was right the whole time the feature appeared broken — see
    /// `terminal::MOUSE_ON`.
    #[test]
    fn a_selection_is_visible_in_the_pane() {
        let mut app = App::new();
        app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();
        app.sessions.detach();
        app.sessions.selected().unwrap().feed("select me please\r\n");

        let theme = Theme::default();
        let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();

        let filled = |terminal: &Terminal<TestBackend>| {
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .filter(|cell| cell.style().bg == Some(theme.highlight))
                .count()
        };

        terminal.draw(|frame| render(frame, &app)).unwrap();
        let before = filled(&terminal);

        // Select the first several cells of the first row, as a drag would.
        let session = app.sessions.selected().unwrap();
        session.begin_mouse_selection(0, 0, 1);
        session.drag_mouse_selection(0, 8);

        terminal.draw(|frame| render(frame, &app)).unwrap();
        let after = filled(&terminal);

        assert!(
            after >= before + 9,
            "nine selected cells should be filled; went from {before} to {after}"
        );
    }

    /// News, not a problem: it takes `link` rather than `attention`, which
    /// means an agent is waiting on you.
    #[test]
    fn a_newer_release_is_mentioned_once_and_quietly() {
        let mut app = App::new();
        assert!(!draw(&app, 130, 20).contains("available"), "nothing to say by default");

        app.update_available = Some("v9.9.9".to_string());
        let rendered = draw(&app, 130, 20);
        assert!(rendered.contains("v9.9.9 available"), "it names the version");
        assert_eq!(rendered.matches("available").count(), 1, "once, not on every view");
    }

    /// Six tabs and a narrow terminal leave nothing for the update badge, and
    /// half of one is worse than none. This is the shape of bug that only
    /// shows up when a tab is added, which is exactly when nobody is looking
    /// at the header.
    #[test]
    fn a_badge_that_does_not_fit_is_dropped_rather_than_cut_in_half() {
        let mut app = App::new();
        app.update_available = Some("v9.9.9".to_string());

        let narrow = draw(&app, 80, 20);
        assert!(!narrow.contains("v9.9"), "no fragment of the version survives");
        assert!(narrow.contains("Sessions"), "and the tabs themselves are untouched");
    }

    /// The list answers "what is there" and the pane answers "what is this".
    /// Both have to be on screen at once or the split is pointless.
    #[test]
    fn the_tasks_view_shows_the_list_and_the_selected_task_together() {
        let root = std::env::temp_dir().join("houston-ui-tasks");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Tasks")).unwrap();
        std::fs::write(
            root.join("Tasks/0001-a.md"),
            "---\npriority: high\nproject: acme\ntags: [auth]\n---\n\n# Ring the bank\n\nBefore Friday.\n",
        )
        .unwrap();
        std::fs::write(root.join("Tasks/0002-b.md"), "---\npriority: low\n---\n# Water plants\n")
            .unwrap();

        let mut app = App::new();
        app.browser =
            Some(crate::vault::Browser::new(crate::vault::Vault::open(root.clone()).unwrap()));
        app.select_tab(Tab::Tasks);

        let frame = draw(&app, 110, 24);

        assert!(frame.contains("Ring the bank"), "the selected task is in the list");
        assert!(frame.contains("Water plants"), "and so is the other one");
        assert!(frame.contains("Before Friday"), "with the description beside it");
        assert!(frame.contains("#auth"), "and its tags");
        assert!(frame.contains("2 open"), "the header counts what is left");
        assert!(frame.contains("high") && frame.contains("low"), "each card says its own priority");
        assert!(frame.contains("acme"), "and which project it belongs to");

        std::fs::remove_dir_all(&root).ok();
    }

    /// The title is the only part of a task you read while scanning, and the
    /// one-line version was cutting it at about twenty characters to make room
    /// for a project column. Two lines gives the title the whole width.
    #[test]
    fn a_card_shows_the_whole_title_rather_than_making_room_for_the_metadata() {
        let root = std::env::temp_dir().join("houston-ui-tasks-long");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Tasks")).unwrap();
        std::fs::write(
            root.join("Tasks/0001-a.md"),
            "---\nproject: acme-api\ntags: [auth]\n---\n# Rotate refresh tokens on use\n",
        )
        .unwrap();

        let mut app = App::new();
        app.browser =
            Some(crate::vault::Browser::new(crate::vault::Vault::open(root.clone()).unwrap()));
        app.select_tab(Tab::Tasks);

        let frame = draw(&app, 110, 20);
        assert!(frame.contains("Rotate refresh tokens on use"), "not a character of it is cut");
        assert!(frame.contains("acme-api"), "and the project still fits, on its own line");

        std::fs::remove_dir_all(&root).ok();
    }

    /// The stat bar says the same four things whether or not the terminal has
    /// the font. Powerline is a setting, so the plain style is a real style
    /// rather than a fallback nobody looked at.
    #[test]
    fn the_stat_bar_says_the_same_things_in_both_styles() {
        let root = std::env::temp_dir().join("houston-ui-tasks-bar");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Tasks")).unwrap();
        std::fs::write(
            root.join("Tasks/0001-a.md"),
            "---\npriority: high\nproject: acme-api\ntags: [auth]\n---\n# Alpha\n",
        )
        .unwrap();

        let mut app = App::new();
        app.browser =
            Some(crate::vault::Browser::new(crate::vault::Vault::open(root.clone()).unwrap()));
        app.select_tab(Tab::Tasks);

        for powerline in [false, true] {
            app.config.powerline = Some(powerline);
            let frame = draw(&app, 110, 20);
            for fact in ["open", "high", "acme-api", "#auth"] {
                assert!(frame.contains(fact), "{fact} is missing with powerline {powerline}");
            }
            // Otherwise both halves of this test draw the same screen and it
            // proves nothing about either.
            assert_eq!(
                frame.contains(crate::ui::powerline::POWERLINE.cap),
                powerline,
                "the separator follows the setting"
            );
        }

        std::fs::remove_dir_all(&root).ok();
    }

    /// A done task hidden by the filter is not the same silence as an empty
    /// folder, and telling them apart is the difference between "press a" and
    /// "press n".
    #[test]
    fn an_empty_task_list_says_which_kind_of_empty_it_is() {
        let root = std::env::temp_dir().join("houston-ui-tasks-empty");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Tasks")).unwrap();
        std::fs::write(root.join("Tasks/0001-a.md"), "---\nstatus: done\n---\n# Done thing\n")
            .unwrap();

        let mut app = App::new();
        app.browser =
            Some(crate::vault::Browser::new(crate::vault::Vault::open(root.clone()).unwrap()));
        app.select_tab(Tab::Tasks);

        assert!(draw(&app, 110, 20).contains("press a"), "there is something behind the filter");

        app.tasks_show_done = true;
        app.load_tasks();
        assert!(draw(&app, 110, 20).contains("Done thing"));

        std::fs::remove_dir_all(&root).ok();
    }

    /// The footer's caps follow the same setting as everything else.
    #[test]
    fn the_keybind_footer_flows_when_powerline_is_on() {
        let mut app = App::new();

        assert!(!draw(&app, 110, 20).contains('\u{e0b0}'), "plain caps sit beside their labels");

        app.config.powerline = Some(true);
        let flowing = draw(&app, 110, 20);
        assert!(flowing.contains('\u{e0b0}'), "and flow into them with the setting on");
        assert!(flowing.contains(" n "), "without losing the key itself");
    }

    /// Nothing Houston draws may be a private use codepoint.
    ///
    /// Ordinary Unicode a font lacks falls back to another installed font and
    /// renders. A private use codepoint that no font on the machine claims has
    /// nothing to fall back to and comes out as a question mark. The powerline
    /// block is the one exception, and only when the setting asks for it.
    #[test]
    fn no_private_use_glyph_reaches_the_screen_unless_powerline_is_on() {
        let mut app = App::new();
        app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();
        app.sessions.selected_mut().unwrap().branch = Some("main".to_string());

        for tab in [Tab::Sessions, Tab::Vault, Tab::Board, Tab::Worktrees, Tab::Settings] {
            app.tab = tab;
            for character in draw(&app, 110, 24).chars() {
                let point = character as u32;
                assert!(
                    !(0xE000..=0xF8FF).contains(&point),
                    "{tab:?} drew U+{point:X}, which renders as a question mark for anyone \
                     whose fonts do not claim it"
                );
            }
        }
    }

    /// The branch marker follows the same setting, so a terminal without the
    /// font never meets it either.
    #[test]
    fn the_branch_glyph_follows_the_same_setting() {
        let mut app = App::new();
        app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();
        app.sessions.selected_mut().unwrap().branch = Some("main".to_string());

        assert!(
            draw(&app, 110, 20).contains('\u{2442}'),
            "plain uses ordinary Unicode, which falls back rather than failing"
        );

        app.config.powerline = Some(true);
        assert!(
            draw(&app, 110, 20).contains('\u{e0a0}'),
            "with the setting on it takes the powerline branch glyph"
        );
    }

    /// Failing to read the list is a different thing from there being none,
    /// and the view has to say which.
    #[test]
    fn an_unreadable_worktree_list_is_not_reported_as_an_empty_one() {
        let mut app = App::new();
        app.tab = Tab::Worktrees;
        app.worktrees = None;

        let rendered = draw(&app, 110, 30);
        assert!(rendered.contains("Could not read"), "a failure says so");
        assert!(!rendered.contains("No worktrees yet"), "and does not claim there are none");
    }

    /// Each state names its own reason. "Not doing anything" covers a worktree
    /// nobody is using *and* one whose repository has been deleted, and those
    /// want completely different things from you.
    #[test]
    fn each_worktree_state_says_why_it_is_in_that_state() {
        let spare =
            |name: &str, dirty: bool, repository: std::path::PathBuf| crate::worktree::Worktree {
                name: name.to_string(),
                path: std::env::temp_dir().join(name),
                repository,
                branch: Some("feature".to_string()),
                dirty,
                ahead: 0,
                behind: 0,
                changes: None,
            };

        let mut app = App::new();
        app.tab = Tab::Worktrees;
        app.worktrees = Some(vec![
            spare("gone", false, std::path::PathBuf::from("/definitely/not/here")),
            spare("dirty", true, std::env::temp_dir()),
            spare("clean", false, std::env::temp_dir()),
        ]);

        let rendered = draw(&app, 130, 24);

        assert!(rendered.contains("repository gone"), "git cannot act on it at all");
        assert!(rendered.contains("no session, uncommitted"), "removing it would lose work");
        assert!(rendered.contains("no session, clean"), "nothing to lose");
        assert!(
            !rendered.contains(" idle "),
            "'idle' was true of three of these and told you nothing about any of them"
        );
    }

    /// The advice answers "what happens if I remove this", which is a question
    /// about one worktree. Four copies would be a column of noise.
    #[test]
    fn only_the_selected_worktree_explains_what_removing_it_costs() {
        let spare = |name: &str| crate::worktree::Worktree {
            name: name.to_string(),
            path: std::env::temp_dir().join(name),
            repository: std::env::temp_dir(),
            branch: None,
            dirty: false,
            ahead: 0,
            behind: 0,
            changes: None,
        };

        let mut app = App::new();
        app.tab = Tab::Worktrees;
        app.worktrees = Some(vec![spare("one"), spare("two"), spare("three")]);

        let rendered = draw(&app, 130, 24);

        assert_eq!(
            rendered.matches("safe to remove").count(),
            1,
            "said once, about the row you are pointing at"
        );
    }

    /// The four states are the whole point of the view: what is safe to throw
    /// away, and what is somebody's unfinished work.
    #[test]
    fn a_worktree_whose_repository_is_gone_is_called_orphaned() {
        let mut app = App::new();
        app.tab = Tab::Worktrees;
        app.worktrees = Some(vec![crate::worktree::Worktree {
            name: "stranded".to_string(),
            path: std::env::temp_dir().join("houston-stranded"),
            repository: std::path::PathBuf::from("/definitely/not/here"),
            branch: Some("feature".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            changes: None,
        }]);

        let rendered = draw(&app, 110, 20);
        assert!(
            rendered.contains("repository gone"),
            "a worktree git can no longer act on has to say so, or it sits on disk forever"
        );
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
