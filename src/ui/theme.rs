//! Colour palette.
//!
//! A deliberate, minimal starting point rather than terminal defaults. The full
//! theme system is Phase 8; this exists so nothing hardcodes a colour.

use ratatui::style::Color;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub accent: Color,
    pub text: Color,
    pub dim: Color,
    pub surface: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Rgb(0x7A, 0xA2, 0xF7),
            text: Color::Rgb(0xC0, 0xCA, 0xF5),
            dim: Color::Rgb(0x56, 0x5F, 0x89),
            surface: Color::Rgb(0x1A, 0x1B, 0x26),
        }
    }
}
