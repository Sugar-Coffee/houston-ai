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
    app::{App, Tab},
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
    if app.is_attached() {
        on_key_attached(app, key);
    } else if app.is_typing() {
        on_key_typing(app, key);
    } else {
        on_key_browsing(app, key);
    }
}

/// While a vault query is being typed, ordinary letters are query text. `q`
/// must not quit and `1` must not switch view.
fn on_key_typing(app: &mut App, key: KeyEvent) {
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
        KeyCode::Char('q') => app.quit(),
        KeyCode::Char('c') if ctrl => app.quit(),
        KeyCode::Tab => app.cycle_tab(true),
        KeyCode::BackTab => app.cycle_tab(false),
        KeyCode::Char(digit @ '1'..='4') => {
            let index = digit as usize - '1' as usize;
            app.select_tab(Tab::ALL[index]);
        }
        _ if app.tab == Tab::Sessions => on_key_sessions(app, key),
        _ if app.tab == Tab::Vault => on_key_vault(app, key),
        _ => {}
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

/// Pushes `@<path>` into the selected session's prompt and jumps to it.
///
/// This is ADR-0004's north star: getting a note into an agent's context
/// without a clipboard round-trip or leaving the app. Deliberately does *not*
/// press Return — you almost always want to type a question after the path.
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
    let Some(session) = app.sessions.selected_mut() else { return };
    let name = session.display_name();

    match session.send_paste(&payload) {
        Ok(()) => {
            app.select_tab(Tab::Sessions);
            app.sessions.attach();
            app.notify(format!("sent to {name}"));
        }
        Err(error) => app.notify(format!("could not send: {error}")),
    }
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
        assert!(app.is_typing());

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

        assert!(!app.is_typing());
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
    fn q_quits_while_browsing() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('q')));
        assert!(app.should_quit);
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
