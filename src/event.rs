//! The event loop.
//!
//! Two properties matter here and both are load-bearing:
//!
//! 1. **Paste is one event, not N key events.** `Event::Paste` arrives whole
//!    and is forwarded to the child in a single write. ADR-0005.
//! 2. **Rendering is frame-budgeted, not event-driven.** Events and child
//!    output mark the app dirty; a tick draws at most once per frame. An agent
//!    emitting thousands of lines a second must not cost thousands of repaints.

use crate::{
    app::{App, InputFocus, Picker, Tab},
    clipboard,
    hooks::{self, Notification},
    pty::Size,
    terminal::Backend,
    ui,
    ui::Theme,
    vault::browser::Mode as VaultMode,
};
use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::Terminal;
use std::time::Duration;

/// ~60fps. The upper bound on how often we draw, not how often we poll.
const FRAME_BUDGET: Duration = Duration::from_millis(16);

pub async fn run(terminal: &mut Terminal<Backend>, mut app: App) -> Result<()> {
    let mut input = EventStream::new();
    let mut frames = tokio::time::interval(FRAME_BUDGET);
    frames.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Agent lifecycle events arrive over a Unix socket. If the listener cannot
    // start, Houston still works — the board just cannot show agent state — so
    // this is reported rather than fatal.
    let (hook_sender, mut hook_events) = tokio::sync::mpsc::unbounded_channel();
    let _listener = match hooks::Listener::start(hook_sender) {
        Ok(listener) => Some(listener),
        Err(error) => {
            app.notify(format!("agent status unavailable: {error}"));
            None
        }
    };

    loop {
        tokio::select! {
            // Keystrokes are checked before the frame tick, so typing never
            // waits on a draw.
            biased;

            Some(event) = input.next() => handle(&mut app, &event?),

            Some(notification) = hook_events.recv() => on_hook(&mut app, &notification),

            _ = frames.tick() => {
                // Children draw on their own schedule; ask them what changed.
                if app.sessions.poll() {
                    app.dirty = true;
                }

                if app.dirty {
                    sync_session_size(terminal, &mut app)?;
                    terminal.draw(|frame| ui::render(frame, &app))?;
                    app.dirty = false;
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

/// Keeps every child's grid the same size as the area it is drawn into.
///
/// Derived from the same layout functions the renderer uses, so the two cannot
/// drift. If they did, child output would wrap in the wrong column.
fn sync_session_size(terminal: &Terminal<Backend>, app: &mut App) -> Result<()> {
    let [_, body, _] = ui::layout(terminal.size()?.into());
    let pane = ui::sessions::terminal_area(body);
    app.resize_sessions(Size::new(pane.height, pane.width));
    Ok(())
}

/// Applies an agent lifecycle event reported by a hook.
fn on_hook(app: &mut App, notification: &Notification) {
    if app.sessions.apply_hook(notification.session, notification.kind) {
        app.dirty = true;
    }
}

fn handle(app: &mut App, event: &Event) {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            app.clear_notice();
            on_key(app, *key);
        }
        Event::Paste(text) => on_paste(app, text),
        Event::Resize(_, _) => app.dirty = true,
        _ => {}
    }
}

/// A pasted block goes to the child in one write, or is ignored if there is no
/// child to receive it. Never re-encoded as individual keystrokes.
fn on_paste(app: &mut App, text: &str) {
    if !app.is_attached() {
        return;
    }
    if let Some(session) = app.sessions.selected_mut()
        && let Err(error) = session.send_paste(text)
    {
        app.notify(format!("paste failed: {error}"));
    }
}

fn on_key(app: &mut App, key: KeyEvent) {
    // An armed quit is cancelled by anything that is not a second `q`. This
    // has to run before the key is dispatched, or the confirmation would
    // survive whatever the key did.
    if app.quit_armed && !matches!(key.code, KeyCode::Char('q')) {
        app.disarm_quit();
        app.dirty = true;
        // Escape is "never mind" and should do nothing else.
        if key.code == KeyCode::Esc {
            return;
        }
    }

    match app.focus() {
        InputFocus::Session => on_key_attached(app, key),
        InputFocus::Overlay => on_key_picker(app, key),
        InputFocus::Text => on_key_typing(app, key),
        InputFocus::Commands => on_key_browsing(app, key),
    }
}

/// The modal session chooser.
fn on_key_picker(app: &mut App, key: KeyEvent) {
    app.dirty = true;
    let count = app.sessions.len();

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.picker = None,
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(picker) = app.picker.as_mut()
                && count > 0
            {
                picker.selected = (picker.selected + 1) % count;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(picker) = app.picker.as_mut()
                && count > 0
            {
                picker.selected = (picker.selected + count - 1) % count;
            }
        }
        // The list is ordered exactly as the Sessions sidebar, so the number
        // beside a session is the number you press.
        KeyCode::Char(digit @ '1'..='9') => {
            let index = digit as usize - '1' as usize;
            if index < count {
                if let Some(picker) = app.picker.as_mut() {
                    picker.selected = index;
                }
                deliver_picked(app);
            }
        }
        KeyCode::Enter => deliver_picked(app),
        _ => {}
    }
}

/// Sends the picker's payload to the chosen session and jumps to it.
fn deliver_picked(app: &mut App) {
    let Some(picker) = app.picker.take() else { return };

    app.sessions.select(picker.selected);
    let Some(session) = app.sessions.selected_mut() else { return };
    let name = session.display_name();

    match session.send_paste(&picker.payload) {
        Ok(()) => {
            app.select_tab(Tab::Sessions);
            app.sessions.attach();
            app.notify(format!("sent to {name}"));
        }
        Err(error) => app.notify(format!("could not send: {error}")),
    }
}

/// While text is being typed — a vault query, or the Settings path field —
/// ordinary letters are content. `q` must not quit and `1` must not switch
/// view.
fn on_key_typing(app: &mut App, key: KeyEvent) {
    if app.renaming.is_some() {
        on_key_renaming(app, key);
        return;
    }
    if app.is_editing_settings() {
        on_key_editing_vault(app, key);
        return;
    }

    let Some(browser) = app.browser.as_mut() else { return };
    app.dirty = true;

    match key.code {
        KeyCode::Esc => {
            browser.end_query();
            browser.show_all();
        }
        KeyCode::Enter => {
            if browser.mode() == VaultMode::Searching {
                browser.run_search();
            } else {
                browser.end_query();
            }
        }
        KeyCode::Backspace => browser.pop_query(),
        KeyCode::Char(character) => browser.push_query(character),
        KeyCode::Down => browser.select_next(),
        KeyCode::Up => browser.select_previous(),
        _ => {}
    }
}

/// While attached, only the detach key is ours. Everything else is the child's,
/// including Ctrl-C, `q` and the digits — intercepting those would make the
/// agent unusable.
fn on_key_attached(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Ctrl-\ is the detach key precisely because almost nothing else uses it:
    // Ctrl-C, Ctrl-D and Escape all belong to the agent.
    //
    // It has to be matched on both spellings. On Unix, crossterm decodes the
    // byte 0x1C as `Char('4')` with CONTROL, not `Char('\\')`
    // (`parse.rs`: `c @ 0x1C..=0x1F => Char(c - 0x1C + b'4')`), which is the
    // historical Ctrl-4 == Ctrl-\ equivalence. Matching only the backslash
    // spelling compiles, passes a hand-written unit test, and never fires on a
    // real keyboard.
    if ctrl && matches!(key.code, KeyCode::Char('\\' | '4')) {
        app.sessions.detach();
        app.dirty = true;
        return;
    }

    if let Some(session) = app.sessions.selected_mut()
        && let Err(error) = session.send_key(key)
    {
        app.notify(format!("session write failed: {error}"));
    }
}

fn on_key_browsing(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        // Two presses to quit: one keystroke should not take down a
        // workspace full of running agents.
        KeyCode::Char('q') => app.request_quit(),
        KeyCode::Char('c') if ctrl => app.quit(),
        KeyCode::Tab => app.cycle_tab(true),
        KeyCode::BackTab => app.cycle_tab(false),
        KeyCode::Char(digit @ '1'..='4') => {
            let index = digit as usize - '1' as usize;
            app.select_tab(Tab::ALL[index]);
        }
        _ if app.tab == Tab::Sessions => on_key_sessions(app, key),
        _ if app.tab == Tab::Vault => on_key_vault(app, key),
        _ if app.tab == Tab::Settings => on_key_settings(app, key),
        _ if app.tab == Tab::Board => on_key_board(app, key),
        _ => {}
    }
}

/// Moving around the board.
///
/// Left and right change column; up and down move within one. The selection is
/// the *session* selection, shared with the sidebar, so opening a card and
/// opening its sidebar row are the same act.
fn on_key_board(app: &mut App, key: KeyEvent) {
    if app.sessions.is_empty() {
        return;
    }
    app.dirty = true;

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => move_column(app, false),
        KeyCode::Char('l') | KeyCode::Right => move_column(app, true),
        KeyCode::Char('j') | KeyCode::Down => move_within_column(app, true),
        KeyCode::Char('k') | KeyCode::Up => move_within_column(app, false),
        KeyCode::Enter => {
            // Jump to the selected session and attach, which is what "open" is
            // going to mean to anyone pressing Enter on a card.
            app.select_tab(Tab::Sessions);
            if !app.sessions.attach() {
                app.notify("that session has exited — press x on the Sessions view to close it");
            }
        }
        _ => {}
    }
}

/// Which board column the selected session is in, and where in it.
fn board_position(app: &App) -> Option<(usize, usize)> {
    use crate::ui::board::{COLUMNS, Column, members};

    let selected = app.sessions.selected_index();
    let session = app.sessions.iter().nth(selected)?;
    let column = Column::of(&session.kind, session.state);

    let column_index = COLUMNS.iter().position(|(_, candidate)| *candidate == column)?;
    let row = members(&app.sessions, column).iter().position(|index| *index == selected)?;
    Some((column_index, row))
}

/// Moves to the next column that has anything in it.
///
/// Empty columns are skipped rather than swallowing the keypress — landing on
/// nothing and having to press again would be worse than jumping over it.
fn move_column(app: &mut App, forward: bool) {
    use crate::ui::board::{COLUMNS, members};

    let Some((column_index, row)) = board_position(app) else { return };
    let count = COLUMNS.len();

    for offset in 1..=count {
        let next = if forward {
            (column_index + offset) % count
        } else {
            (column_index + count - offset % count) % count
        };

        let candidate = members(&app.sessions, COLUMNS[next].1);
        if candidate.is_empty() {
            continue;
        }
        // Keep the same depth where possible, so crossing a board of equal
        // columns does not reset you to the top every time.
        let target = candidate[row.min(candidate.len() - 1)];
        app.sessions.select(target);
        return;
    }
}

fn move_within_column(app: &mut App, forward: bool) {
    use crate::ui::board::{COLUMNS, members};

    let Some((column_index, row)) = board_position(app) else { return };
    let candidate = members(&app.sessions, COLUMNS[column_index].1);
    if candidate.is_empty() {
        return;
    }

    let length = candidate.len();
    let next = if forward { (row + 1) % length } else { (row + length - 1) % length };
    app.sessions.select(candidate[next]);
}

fn on_key_settings(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Char('e') {
        // Seed the field with what is in use, so editing is a tweak rather
        // than a retype.
        app.editing_vault = Some(
            app.config
                .vault_root()
                .map_or_else(|_| String::new(), |root| root.display().to_string()),
        );
        app.dirty = true;
    }
}

/// Renaming the selected session.
fn on_key_renaming(app: &mut App, key: KeyEvent) {
    app.dirty = true;

    match key.code {
        KeyCode::Esc => app.renaming = None,
        KeyCode::Backspace => {
            if let Some(draft) = app.renaming.as_mut() {
                draft.pop();
            }
        }
        KeyCode::Char(character) => {
            if let Some(draft) = app.renaming.as_mut() {
                draft.push(character);
            }
        }
        KeyCode::Enter => {
            let Some(draft) = app.renaming.take() else { return };
            let name = draft.trim().to_string();
            if let Some(session) = app.sessions.selected_mut() {
                // An empty name would leave an unlabelled row, so it means
                // "put it back to the default" rather than "clear it".
                session.rename(if name.is_empty() { None } else { Some(name) });
            }
        }
        _ => {}
    }
}

/// Editing the vault path in Settings.
fn on_key_editing_vault(app: &mut App, key: KeyEvent) {
    app.dirty = true;

    match key.code {
        KeyCode::Esc => app.editing_vault = None,
        KeyCode::Backspace => {
            if let Some(draft) = app.editing_vault.as_mut() {
                draft.pop();
            }
        }
        KeyCode::Char(character) => {
            if let Some(draft) = app.editing_vault.as_mut() {
                draft.push(character);
            }
        }
        KeyCode::Enter => commit_vault_path(app),
        _ => {}
    }
}

/// Saves the typed vault path and reopens the vault.
///
/// The old vault is kept if the new path does not work, so a typo cannot leave
/// you with no vault at all.
fn commit_vault_path(app: &mut App) {
    let Some(draft) = app.editing_vault.take() else { return };
    let draft = draft.trim().to_string();

    let previous = app.config.vault.clone();
    // An empty field means "go back to the default", which is the only way to
    // undo a bad path without editing the config file by hand.
    app.config.vault = (!draft.is_empty()).then(|| std::path::PathBuf::from(&draft));

    app.load_vault();

    if let Some(error) = app.vault_error.clone() {
        app.config.vault = previous;
        app.load_vault();
        app.notify(error);
        return;
    }

    match app.config.save_to(&app.config_path) {
        Ok(()) => {
            let count = app.browser.as_ref().map_or(0, |browser| browser.vault.len());
            app.notify(format!(
                "vault set — {count} notes indexed, saved to {}",
                app.config_path.display()
            ));
        }
        Err(error) => app.notify(format!("vault opened but could not save config: {error}")),
    }
}

fn on_key_vault(app: &mut App, key: KeyEvent) {
    if app.browser.is_none() {
        return;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let theme = Theme::default();
    app.dirty = true;

    // Split so the borrow of `app.browser` ends before the arms that need
    // `app` as a whole (yank, and sending into a session).
    match key.code {
        KeyCode::Char('y') => yank_selected_path(app),
        KeyCode::Char('i') => send_selected_to_session(app),
        _ => {
            let Some(browser) = app.browser.as_mut() else { return };
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => browser.select_next(),
                KeyCode::Char('k') | KeyCode::Up => browser.select_previous(),
                KeyCode::Enter => {
                    if let Err(error) = browser.open_selected(theme) {
                        app.notify(format!("could not open note: {error}"));
                    }
                }
                KeyCode::Char('/') => browser.begin_find(),
                KeyCode::Char('f') => browser.begin_search(),
                KeyCode::Char('a') => browser.show_all(),
                KeyCode::Char('l') => {
                    if !browser.show_links() {
                        app.notify("this note has no outgoing links");
                    }
                }
                KeyCode::Char('b') => {
                    if !browser.show_backlinks() {
                        app.notify("no backlinks yet — they build up as you visit notes");
                    }
                }
                KeyCode::Backspace => {
                    if !browser.go_back(theme) {
                        app.notify("nowhere to go back to");
                    }
                }
                KeyCode::Char('d') if ctrl => browser.scroll(15),
                KeyCode::Char('u') if ctrl => browser.scroll(-15),
                KeyCode::PageDown => browser.scroll(15),
                KeyCode::PageUp => browser.scroll(-15),
                KeyCode::Char('w') => browser.toggle_wrap(),
                KeyCode::Char('g') => browser.scroll_to_top(),
                KeyCode::Char('G') => browser.scroll_to_bottom(),
                _ => {}
            }
        }
    }
}

/// Copies the selected note's absolute path. The minimum bar the brief set.
fn yank_selected_path(app: &mut App) {
    let Some(path) = selected_path(app) else {
        app.notify("no note selected");
        return;
    };

    match clipboard::copy(&path) {
        Ok(_) => app.notify(format!("copied {path}")),
        Err(error) => app.notify(format!("could not copy: {error}")),
    }
}

/// Pushes `@<path>` into a session's prompt and jumps to it.
///
/// This is ADR-0004's north star: getting a note into an agent's context
/// without a clipboard round-trip or leaving the app. Deliberately does *not*
/// press Return — you almost always want to type a question after the path.
///
/// With one session there is nothing to decide, so it goes straight there.
/// With several, a chooser opens rather than guessing at the selected one.
fn send_selected_to_session(app: &mut App) {
    let Some(path) = selected_path(app) else {
        app.notify("no note selected");
        return;
    };
    if app.sessions.is_empty() {
        app.notify("no session to send to — start one with n on the Sessions view");
        return;
    }

    let payload = format!("@{path} ");

    if app.sessions.len() == 1 {
        app.picker = Some(Picker { prompt: String::new(), payload, selected: 0 });
        deliver_picked(app);
        return;
    }

    let name = app
        .browser
        .as_ref()
        .and_then(|browser| browser.selected_note())
        .map_or_else(String::new, |note| note.stem.clone());

    app.picker = Some(Picker {
        prompt: format!("Send {name} to which session?"),
        payload,
        selected: app.sessions.selected_index(),
    });
    app.dirty = true;
}

fn selected_path(app: &App) -> Option<String> {
    app.browser.as_ref()?.selected_note().map(|note| note.path.to_string_lossy().into_owned())
}

fn on_key_sessions(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.sessions.select_next();
            app.dirty = true;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.sessions.select_previous();
            app.dirty = true;
        }
        KeyCode::Char('n') => spawn(app, false),
        KeyCode::Char('s') => spawn(app, true),
        KeyCode::Char('r') => {
            if let Some(session) = app.sessions.selected() {
                app.renaming = Some(session.name.clone());
                app.dirty = true;
            } else {
                app.notify("no session to rename");
            }
        }
        KeyCode::Char('x') => {
            app.sessions.close_selected();
            app.dirty = true;
        }
        KeyCode::Enter => {
            if app.sessions.is_empty() {
                app.notify("no session to attach to — press n or s to start one");
            } else if app.sessions.attach() {
                app.dirty = true;
            } else {
                app.notify("that session has exited — press x to close it");
            }
        }
        _ => {}
    }
}

fn spawn(app: &mut App, shell: bool) {
    // A real size arrives on the next frame; this only has to be non-degenerate
    // so the child's first output does not wrap against a 1x1 grid.
    let size = Size::new(24, 80);
    let cwd = app.cwd.clone();

    let result = if shell {
        app.sessions.spawn_shell(&cwd, size)
    } else {
        app.sessions.spawn_agent(&cwd, size)
    };

    match result {
        Ok(_) => {
            app.sessions.attach();
            app.dirty = true;
            // Hook installation is best-effort, but a silent failure would
            // leave the board quietly wrong for the rest of the session.
            if let Some(warning) = app.sessions.take_hook_warning() {
                app.notify(format!("agent status unavailable: {warning}"));
            }
        }
        Err(error) => app.notify(format!("could not start a session: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Focus;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn typing_a_query_does_not_trigger_commands() {
        let mut app = App::new();
        // Only meaningful when a vault exists on this machine.
        if app.browser.is_none() {
            return;
        }

        app.select_tab(Tab::Vault);
        on_key(&mut app, press(KeyCode::Char('/')));
        assert_eq!(app.focus(), InputFocus::Text);

        // Every one of these is a command while browsing.
        for character in "q1f".chars() {
            on_key(&mut app, press(KeyCode::Char(character)));
        }

        assert!(!app.should_quit, "q inside a query is text, not a command");
        assert_eq!(app.tab, Tab::Vault, "1 inside a query must not switch view");
        assert_eq!(app.browser.as_ref().unwrap().query(), "q1f");
    }

    #[test]
    fn escape_leaves_a_query_without_quitting() {
        let mut app = App::new();
        if app.browser.is_none() {
            return;
        }
        app.select_tab(Tab::Vault);
        on_key(&mut app, press(KeyCode::Char('/')));
        on_key(&mut app, press(KeyCode::Esc));

        assert_ne!(app.focus(), InputFocus::Text);
        assert!(!app.should_quit);
    }

    #[test]
    fn sending_a_note_with_no_session_says_so() {
        let mut app = App::new();
        if app.browser.is_none() {
            return;
        }
        app.select_tab(Tab::Vault);
        on_key(&mut app, press(KeyCode::Char('i')));

        assert!(app.notice.as_deref().is_some_and(|notice| notice.contains("no session")));
        assert_eq!(app.tab, Tab::Vault, "a failed send must not switch view");
    }

    #[test]
    fn editing_the_vault_path_captures_every_key() {
        let mut app = App::new();
        app.select_tab(Tab::Settings);
        on_key(&mut app, press(KeyCode::Char('e')));

        assert!(app.is_editing_settings());
        assert!(!app.editing_vault.as_ref().unwrap().is_empty(), "seeded with the current path");

        // `q` and `1` are commands everywhere else.
        for character in "q1".chars() {
            on_key(&mut app, press(KeyCode::Char(character)));
        }
        assert!(!app.should_quit);
        assert_eq!(app.tab, Tab::Settings);
        assert!(app.editing_vault.as_ref().unwrap().ends_with("q1"));

        on_key(&mut app, press(KeyCode::Esc));
        assert!(!app.is_editing_settings(), "escape abandons the edit");
    }

    #[test]
    fn a_bad_vault_path_is_rejected_and_the_old_one_kept() {
        let mut app = App::new();
        app.config_path = std::env::temp_dir().join("houston-bad-path-config.toml");
        let before = app.config.vault.clone();
        let indexed_before = app.browser.as_ref().map(|browser| browser.vault.len());

        app.select_tab(Tab::Settings);
        app.editing_vault = Some("/tmp/houston-nope-not-here".to_string());
        commit_vault_path(&mut app);

        assert!(app.notice.is_some(), "the failure is reported");
        assert_eq!(app.config.vault, before, "the config is rolled back");
        assert_eq!(
            app.browser.as_ref().map(|browser| browser.vault.len()),
            indexed_before,
            "a typo must not leave you with no vault"
        );
    }

    #[test]
    fn pointing_at_a_real_folder_switches_the_vault() {
        let root = std::env::temp_dir().join("houston-settings-switch");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("only-note.md"), "# Only note").unwrap();

        let mut app = App::new();
        // Never the real config: an earlier version of this test repointed a
        // live install at a temp folder that it then deleted.
        app.config_path = std::env::temp_dir().join("houston-settings-switch-config.toml");

        app.select_tab(Tab::Settings);
        app.editing_vault = Some(root.display().to_string());
        commit_vault_path(&mut app);

        assert_eq!(app.browser.as_ref().unwrap().vault.len(), 1);
        assert!(app.vault_error.is_none());
        assert!(app.config_path.exists(), "the setting is persisted");

        std::fs::remove_file(&app.config_path).ok();
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn quitting_takes_two_presses() {
        let mut app = App::new();

        on_key(&mut app, press(KeyCode::Char('q')));
        assert!(!app.should_quit, "one press only arms it");
        assert!(app.quit_armed);

        on_key(&mut app, press(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn any_other_key_disarms_a_pending_quit() {
        for interrupting in [KeyCode::Esc, KeyCode::Char('2'), KeyCode::Char('j')] {
            let mut app = App::new();
            on_key(&mut app, press(KeyCode::Char('q')));
            assert!(app.quit_armed);

            on_key(&mut app, press(interrupting));
            assert!(!app.quit_armed, "{interrupting:?} should cancel the pending quit");

            // And a later `q` starts over rather than completing the old one.
            on_key(&mut app, press(KeyCode::Char('q')));
            assert!(!app.should_quit, "the confirmation must not survive an interruption");
        }
    }

    #[test]
    fn escape_only_cancels_the_quit_and_does_nothing_else() {
        let mut app = App::new();
        app.select_tab(Tab::Vault);
        on_key(&mut app, press(KeyCode::Char('q')));

        on_key(&mut app, press(KeyCode::Esc));
        assert!(!app.quit_armed);
        assert_eq!(app.tab, Tab::Vault, "escape should not have side effects");
    }

    #[test]
    fn renaming_a_session_sticks_and_beats_the_terminal_title() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        app.sessions.detach();

        on_key(&mut app, press(KeyCode::Char('r')));
        assert_eq!(app.focus(), InputFocus::Text, "renaming captures keys");

        // Clear the seeded name, then type a new one including a `q`.
        for _ in 0..40 {
            on_key(&mut app, press(KeyCode::Backspace));
        }
        for character in "queue runner".chars() {
            on_key(&mut app, press(KeyCode::Char(character)));
        }
        assert!(!app.should_quit, "q inside a name is text");

        on_key(&mut app, press(KeyCode::Enter));
        assert_eq!(app.sessions.selected().unwrap().display_name(), "queue runner");
    }

    #[test]
    fn an_empty_rename_restores_the_default_name() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        app.sessions.detach();
        let original = app.sessions.selected().unwrap().name.clone();

        app.renaming = Some("temporary".to_string());
        on_key(&mut app, press(KeyCode::Enter));
        assert_eq!(app.sessions.selected().unwrap().display_name(), "temporary");

        app.renaming = Some("   ".to_string());
        on_key(&mut app, press(KeyCode::Enter));
        assert_eq!(app.sessions.selected().unwrap().name, original);
    }

    #[test]
    fn sending_a_note_with_several_sessions_opens_a_chooser() {
        let mut app = App::new();
        if app.browser.is_none() {
            return;
        }
        for _ in 0..2 {
            on_key(&mut app, press(KeyCode::Char('s')));
            app.sessions.detach();
        }
        assert_eq!(app.sessions.len(), 2);

        app.select_tab(Tab::Vault);
        on_key(&mut app, press(KeyCode::Char('i')));

        assert!(app.picker.is_some(), "with a choice to make, ask");
        assert_eq!(app.focus(), InputFocus::Overlay);
        assert!(app.picker.as_ref().unwrap().payload.starts_with('@'));

        on_key(&mut app, press(KeyCode::Esc));
        assert!(app.picker.is_none(), "escape closes the chooser");
        assert_eq!(app.tab, Tab::Vault, "cancelling does not move you");
    }

    #[test]
    fn sending_with_one_session_skips_the_chooser() {
        let mut app = App::new();
        if app.browser.is_none() {
            return;
        }
        on_key(&mut app, press(KeyCode::Char('s')));
        app.sessions.detach();

        app.select_tab(Tab::Vault);
        on_key(&mut app, press(KeyCode::Char('i')));

        assert!(app.picker.is_none(), "no choice to make, so no question asked");
        assert_eq!(app.tab, Tab::Sessions, "it jumps straight to the session");
    }

    #[test]
    fn digits_jump_between_views() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('3')));
        assert_eq!(app.tab, Tab::Board);
    }

    #[test]
    fn attaching_with_no_sessions_explains_itself() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Enter));
        assert!(app.notice.is_some(), "the user should be told why nothing happened");
        assert!(!app.is_attached());
    }

    #[test]
    fn a_pressed_key_clears_a_stale_notice() {
        let mut app = App::new();
        app.notify("something went wrong");
        handle(&mut app, &Event::Key(press(KeyCode::Char('2'))));
        assert!(app.notice.is_none());
    }

    #[test]
    fn paste_while_browsing_is_dropped_not_typed() {
        let mut app = App::new();
        // Nothing to receive it, and it must never be re-encoded as keystrokes.
        handle(&mut app, &Event::Paste("rm -rf /".to_string()));
        assert!(!app.should_quit);
        assert!(app.notice.is_none());
    }

    #[test]
    fn spawning_a_shell_attaches_to_it() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));

        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.sessions.focus(), Focus::Attached);
        assert!(app.is_attached());
    }

    #[test]
    fn while_attached_houston_keys_belong_to_the_child() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        assert!(app.is_attached());

        // `q` would quit while browsing. Attached, it is just a character.
        on_key(&mut app, press(KeyCode::Char('q')));
        assert!(!app.should_quit, "q must reach the child, not quit Houston");

        // Same for the view-switching digits.
        on_key(&mut app, press(KeyCode::Char('2')));
        assert_eq!(app.tab, Tab::Sessions, "digits must reach the child too");
    }

    /// Both spellings must work. The `'4'` case is the one that actually
    /// reaches us from a real terminal — see the comment in `on_key_attached`.
    #[test]
    fn ctrl_backslash_detaches_in_both_of_crossterms_spellings() {
        for code in [KeyCode::Char('\\'), KeyCode::Char('4')] {
            let mut app = App::new();
            on_key(&mut app, press(KeyCode::Char('s')));
            assert!(app.is_attached());

            on_key(&mut app, KeyEvent::new(code, KeyModifiers::CONTROL));
            assert!(!app.is_attached(), "ctrl+{code:?} should detach");
            assert_eq!(app.sessions.focus(), Focus::Browsing);
        }
    }

    /// A plain `4` while attached is a character the child should receive, not
    /// a detach. Only the control-modified form is ours.
    #[test]
    fn an_unmodified_four_does_not_detach() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        on_key(&mut app, press(KeyCode::Char('4')));
        assert!(app.is_attached(), "plain 4 belongs to the child");
    }

    #[test]
    fn closing_the_only_session_returns_to_browsing() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        app.sessions.detach();

        on_key(&mut app, press(KeyCode::Char('x')));
        assert!(app.sessions.is_empty());
        assert!(!app.is_attached());
    }
}
