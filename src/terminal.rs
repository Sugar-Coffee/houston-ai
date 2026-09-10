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
use std::io::{Stdout, Write, stdout};

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
    let mut out = stdout();
    out.write_all(MOUSE_OFF.as_bytes())?;
    execute!(out, DisableBracketedPaste, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

/// Ask for button presses, drags, and SGR coordinates.
///
/// **`?1002h` was left out once and had to be added back.** The note that
/// removed it said `crossterm`'s `EnableMouseCapture` asks for "`?1002h`
/// (drag) and `?1003h` (every movement)" and that we needed neither. Half of
/// that was right. `?1003h` really does report an event per movement whether
/// or not a button is down, which floods the loop and is the grabbiest thing
/// an app can ask a terminal for. `?1002h` reports motion **only while a
/// button is held** — which is precisely a drag, costs nothing the rest of the
/// time, and is what every terminal application that supports selection asks
/// for.
///
/// Without it a press and a release arrive and nothing in between, so a
/// selection is created where you pressed, never extended, and is empty by the
/// time you let go. Which looks exactly like selection not working at all.
///
/// `?1007l` turns off alternate scroll, so the terminal cannot quietly convert
/// the wheel into arrow keys behind our back — which is what made the wheel
/// cycle an agent's history instead of scrolling.
const MOUSE_ON: &str = "\x1b[?1000h\x1b[?1002h\x1b[?1006h\x1b[?1007l";

/// Everything `MOUSE_ON` might have set, plus the modes we never ask for, in
/// case a previous program left them on.
const MOUSE_OFF: &str = "\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1015l\x1b[?1006l";

/// Turns mouse reporting on or off.
///
/// A genuine trade: with it on, the wheel scrolls a session's scrollback and
/// clicks select rows — with it off, your terminal's own click-drag selection
/// works again. Most terminals let you hold Shift to bypass reporting and
/// select anyway, but not all, so this stays a setting rather than a decision.
pub fn set_mouse(enabled: bool) -> Result<()> {
    let mut out = stdout();
    out.write_all(if enabled { MOUSE_ON.as_bytes() } else { MOUSE_OFF.as_bytes() })?;
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod mouse_mode_tests {
    use super::*;

    /// The mode that makes drag selection possible at all.
    ///
    /// Asserted rather than assumed, because its absence is invisible: every
    /// individual piece works, and the feature simply does nothing.
    #[test]
    fn drag_reporting_is_requested_but_any_motion_reporting_is_not() {
        assert!(MOUSE_ON.contains("?1002h"), "motion while a button is held — that is a drag");
        assert!(
            !MOUSE_ON.contains("?1003h"),
            "any-motion tracking floods the loop with an event per movement"
        );
        assert!(MOUSE_ON.contains("?1006h"), "SGR coordinates, so columns past 223 still work");
        assert!(MOUSE_ON.contains("?1007l"), "no alternate scroll behind our back");
    }

    /// Whatever we turn on has to be turned off again, or a terminal is left
    /// reporting the mouse to whatever runs next.
    #[test]
    fn everything_turned_on_is_turned_off_again() {
        for mode in ["1000", "1002", "1006"] {
            assert!(
                MOUSE_OFF.contains(&format!("?{mode}l")),
                "{mode} is enabled but never disabled"
            );
        }
    }
}
