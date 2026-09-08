//! The event loop.
//!
//! Two properties matter here and both are load-bearing later:
//!
//! 1. **Paste is one event, not N key events.** `Event::Paste` is handled
//!    whole. Once sessions exist (Phase 2) it will be forwarded to the child
//!    PTY in a single write. See ADR-0005.
//! 2. **Rendering is frame-budgeted, not event-driven.** Events mark the app
//!    dirty; a tick draws at most once per frame. A busy agent emitting
//!    thousands of lines a second must not cost thousands of repaints.

use crate::{
    app::{App, Tab},
    terminal::Backend,
    ui,
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

    loop {
        tokio::select! {
            biased;

            Some(event) = input.next() => handle(&mut app, event?),

            _ = frames.tick() => {
                if app.dirty {
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

#[expect(
    clippy::match_same_arms,
    reason = "the Paste arm is deliberately explicit and empty: it marks where \
              ADR-0005's single-write passthrough hooks in once sessions exist"
)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Event is taken by value so Event::Paste(String) can be moved into the \
              PTY write rather than cloned; a large paste should not be copied"
)]
fn handle(app: &mut App, event: Event) {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => on_key(app, key),
        Event::Resize(_, _) => app.dirty = true,
        // Handled as a single unit. Nothing consumes it yet — sessions land in
        // Phase 2 — but the shape is right from the start (ADR-0005).
        Event::Paste(_) => {}
        _ => {}
    }
}

fn on_key(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('q') => app.quit(),
        KeyCode::Char('c') if ctrl => app.quit(),
        KeyCode::Tab => app.cycle_tab(true),
        KeyCode::BackTab => app.cycle_tab(false),
        KeyCode::Char(c @ '1'..='4') => {
            let index = c as usize - '1' as usize;
            app.select_tab(Tab::ALL[index]);
        }
        _ => {}
    }
}
