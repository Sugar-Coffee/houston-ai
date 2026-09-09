//! Powerline-style segments, for terminals with a font that can draw them.
//!
//! The look is the one you see on a Claude Code statusline addon or a vim
//! airline: coloured blocks separated by a solid triangle, so each segment
//! flows into the next instead of sitting in its own box.
//!
//! **It is a font, not a plugin.** The statusline tools people run alongside
//! Claude Code — `ccstatusline`, `claude-powerline` and friends — do not
//! provide anything Houston could consume; Houston draws its own screen. What
//! makes them look like that is a [Nerd Font](https://www.nerdfonts.com/),
//! which patches the [powerline](https://github.com/powerline/powerline)
//! glyphs into an ordinary monospace family. Install one of those and set your
//! terminal to it, and Houston can use the same glyphs.
//!
//! **Off by default, and it has to be.** A terminal without those glyphs draws
//! them as replacement boxes, which does not degrade gracefully — it looks
//! broken, on the very first screen, to somebody who has not opted into
//! anything. So this is a setting, and the plain style is a real style rather
//! than a fallback nobody tested.

/// The characters a style draws its edges with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyphs {
    /// Closes a segment, pointing right. U+E0B0 in powerline.
    pub cap: &'static str,
    /// Opens a segment, pointing right into it. U+E0B2.
    pub notch: &'static str,
    /// Precedes a branch name. U+E0A0.
    pub branch: &'static str,
    /// Whether segments should be drawn as flowing blocks at all.
    pub segmented: bool,
}

/// The plain style: no glyph outside the ASCII-and-common-symbols range that
/// every terminal already draws.
pub const PLAIN: Glyphs = Glyphs { cap: "", notch: "", branch: "⑂", segmented: false };

/// The powerline style. Needs a Nerd Font.
pub const POWERLINE: Glyphs =
    Glyphs { cap: "\u{e0b0}", notch: "\u{e0b2}", branch: "\u{e0a0}", segmented: true };

impl Glyphs {
    /// The style for a setting.
    #[must_use]
    pub const fn for_setting(enabled: bool) -> Self {
        if enabled { POWERLINE } else { PLAIN }
    }

    /// The separator between two segments, and what colours it takes.
    ///
    /// A powerline separator belongs to *both* segments: it is drawn in the
    /// outgoing segment's background, on the incoming segment's background.
    /// Getting that backwards is what produces the notorious one-column seam.
    #[must_use]
    pub const fn is_flowing(self) -> bool {
        self.segmented
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plain_style_uses_nothing_a_terminal_might_not_have() {
        for glyph in [PLAIN.cap, PLAIN.notch, PLAIN.branch] {
            for character in glyph.chars() {
                assert!(
                    (character as u32) < 0xE000 || (character as u32) > 0xF8FF,
                    "{character:?} is in the private use area, which is where Nerd Font glyphs \
                     live and where an unpatched font draws a box"
                );
            }
        }
    }

    #[test]
    fn the_powerline_style_uses_the_glyphs_the_standard_defines() {
        assert_eq!(POWERLINE.cap, "\u{e0b0}", "the right-pointing solid triangle");
        assert_eq!(POWERLINE.notch, "\u{e0b2}", "and its mirror");
        assert_eq!(POWERLINE.branch, "\u{e0a0}", "the branch glyph");
    }

    #[test]
    fn the_setting_picks_one_or_the_other_and_defaults_to_safe() {
        assert_eq!(Glyphs::for_setting(false), PLAIN, "off means nothing exotic is drawn");
        assert!(!PLAIN.is_flowing(), "plain tabs stay as pills");
        assert!(Glyphs::for_setting(true).is_flowing());
    }
}
