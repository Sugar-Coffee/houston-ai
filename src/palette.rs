//! The xterm 256-colour palette.
//!
//! Needed because a child program can *ask* the terminal what a colour index
//! actually is (OSC 4). Chloe never answers; we do, so programs that probe for
//! colour support get a truthful reply instead of a timeout.

use alacritty_terminal::vte::ansi::Rgb;

/// `Rgb` is a plain struct with public fields and no constructor.
const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}

/// The 6 levels each channel takes in the 6x6x6 colour cube.
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// The 16 system colours, matching xterm's defaults.
const SYSTEM: [(u8, u8, u8); 16] = [
    (0x00, 0x00, 0x00), // black
    (0x80, 0x00, 0x00), // red
    (0x00, 0x80, 0x00), // green
    (0x80, 0x80, 0x00), // yellow
    (0x00, 0x00, 0x80), // blue
    (0x80, 0x00, 0x80), // magenta
    (0x00, 0x80, 0x80), // cyan
    (0xC0, 0xC0, 0xC0), // white
    (0x80, 0x80, 0x80), // bright black
    (0xFF, 0x00, 0x00), // bright red
    (0x00, 0xFF, 0x00), // bright green
    (0xFF, 0xFF, 0x00), // bright yellow
    (0x00, 0x00, 0xFF), // bright blue
    (0xFF, 0x00, 0xFF), // bright magenta
    (0x00, 0xFF, 0xFF), // bright cyan
    (0xFF, 0xFF, 0xFF), // bright white
];

/// Resolves a palette index to an RGB value.
///
/// Indices at or above 256 are `alacritty_terminal`'s special slots
/// (foreground, background, cursor and friends). We answer those with our own
/// foreground colour rather than failing to reply at all.
#[must_use]
pub fn xterm_256(index: usize) -> Rgb {
    match index {
        0..=15 => {
            let (r, g, b) = SYSTEM[index];
            rgb(r, g, b)
        }
        16..=231 => {
            let offset = index - 16;
            rgb(CUBE_LEVELS[offset / 36], CUBE_LEVELS[(offset / 6) % 6], CUBE_LEVELS[offset % 6])
        }
        232..=255 => {
            let level = 8 + 10 * u8::try_from(index - 232).unwrap_or(0);
            rgb(level, level, level)
        }
        _ => rgb(0xC0, 0xCA, 0xF5),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_colours_match_xterm() {
        assert_eq!(xterm_256(0), rgb(0, 0, 0));
        assert_eq!(xterm_256(1), rgb(0x80, 0, 0));
        assert_eq!(xterm_256(15), rgb(0xFF, 0xFF, 0xFF));
    }

    #[test]
    fn cube_corners_are_right() {
        // 16 is the cube's black corner, 231 its white one.
        assert_eq!(xterm_256(16), rgb(0, 0, 0));
        assert_eq!(xterm_256(231), rgb(255, 255, 255));
        // 21 is pure blue: r=0, g=0, b=max.
        assert_eq!(xterm_256(21), rgb(0, 0, 255));
        // 196 is pure red.
        assert_eq!(xterm_256(196), rgb(255, 0, 0));
    }

    #[test]
    fn greyscale_ramp_is_evenly_spaced() {
        assert_eq!(xterm_256(232), rgb(8, 8, 8));
        assert_eq!(xterm_256(255), rgb(238, 238, 238));
    }

    #[test]
    fn out_of_range_indices_do_not_panic() {
        for index in [256, 300, usize::MAX] {
            let _ = xterm_256(index);
        }
    }
}
