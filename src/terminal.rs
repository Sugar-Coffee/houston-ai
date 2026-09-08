//! Terminal lifecycle: raw mode, alternate screen, bracketed paste, and a panic
//! hook so a crash never leaves the user's terminal wrecked.
//!
//! Bracketed paste is enabled here, from the very first commit, on purpose.
//! It is the root of the paste-performance problem described in ADR-0005:
//! without it crossterm reports a clipboard paste as N separate key events.

use anyhow::Result;
use crossterm::{
    event::{DisableBracketedPaste, EnableBracketedPaste},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{Stdout, stdout};

pub type Backend = CrosstermBackend<Stdout>;

/// Owns the terminal's mutated state and restores it on drop.
pub struct Guard {
    pub terminal: Terminal<Backend>,
    restored: bool,
}

/// Puts the terminal into the state Houston needs to draw.
pub fn enter() -> Result<Guard> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableBracketedPaste)?;

    // A panic while in raw mode leaves the terminal unusable and the backtrace
    // unreadable. Restore first, then let the default hook print.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        default_hook(info);
    }));

    Ok(Guard { terminal: Terminal::new(CrosstermBackend::new(out))?, restored: false })
}

impl Guard {
    /// Restores the terminal. Idempotent; `Drop` will call it if you do not.
    pub fn leave(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        restore()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.leave();
    }
}

fn restore() -> Result<()> {
    execute!(stdout(), DisableBracketedPaste, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
