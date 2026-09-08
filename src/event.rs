//! The event loop.
//!
//! Two properties matter here and both are load-bearing:
//!
//! 1. **Paste is one event, not N key events.** `Event::Paste` arrives whole
//!    and is forwarded to the child in a single write. ADR-0005.
//! 2. **Rendering is frame-budgeted, not event-driven.** Events and child
//!    output mark the app dirty; a tick draws at most once per frame. An agent
//!    emitting thousands of lines a second must not cost thousands of repaints.

use crate::{app::App, app::Tab, pty::Size, terminal::Backend, ui};
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

    loop {
        tokio::select! {
            // Keystrokes are checked before the frame tick, so typing never
            // waits on a draw.
            biased;

            Some(event) = input.next() => handle(&mut app, &event?),

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
    } else {
        on_key_browsing(app, key);
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
        _ => {}
    }
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
