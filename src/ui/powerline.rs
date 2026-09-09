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

/// The plain style: no glyph outside what an ordinary monospace font carries.
pub const PLAIN: Glyphs = Glyphs { cap: "", notch: "", branch: "\u{2442}", segmented: false };

/// The powerline style, from the symbols block U+E0A0–U+E0B3.
///
/// **Only this block.** An earlier version also drew Font Awesome icons here,
/// on the assumption that anyone with powerline glyphs has a Nerd Font. That
/// is wrong often enough to matter: the [powerline/fonts][pf] collection
/// patches this range and nothing else, and it is what most people who set up
/// a shell prompt before Nerd Fonts existed are still running.
///
/// The failure was ugly in a specific way. A missing glyph in ordinary Unicode
/// falls back to another installed font and renders fine. A missing glyph in
/// the private use area has nothing to fall back *to* — no font on the machine
/// claims it — so it comes out as a question mark. Icons were the only part of
/// Houston that could produce that, which is why they are gone rather than
/// behind another switch.
///
/// [pf]: https://github.com/powerline/fonts
pub const POWERLINE: Glyphs =
    Glyphs { cap: "\u{e0b0}", notch: "\u{e0b2}", branch: "\u{e0a0}", segmented: true };

/// A sample of the glyphs, for the Settings hint.
///
/// **The preview is the detection.** Houston cannot ask the terminal which
/// font it is using — that lives in the terminal's own profile, in a different
/// place for every one of them. Putting the actual characters in the hint
/// sidesteps it: you look at the row, and if it is boxes or question marks you
/// know before you turn anything on.
pub const SAMPLE: &str = "\u{e0b0} \u{e0b2} \u{e0a0}";

impl Glyphs {
    /// The style for the setting.
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

    /// The plain style has to survive an ordinary monospace font.
    ///
    /// Specifically it must stay out of the private use area. Ordinary Unicode
    /// that a font lacks falls back to another font and renders; a private use
    /// codepoint that no installed font claims renders as a question mark, and
    /// there is nothing the user can do about it short of installing a font.
    #[test]
    fn the_plain_style_stays_out_of_the_private_use_area() {
        for glyph in [PLAIN.cap, PLAIN.notch, PLAIN.branch] {
            for character in glyph.chars() {
                let point = character as u32;
                assert!(
                    !(0xE000..=0xF8FF).contains(&point),
                    "{character:?} is private use, so a font without it has no fallback"
                );
            }
        }
    }

    /// Everything the powerline style draws must be in the one block that
    /// powerline-patched fonts actually carry.
    #[test]
    fn the_powerline_style_uses_only_the_powerline_block() {
        for glyph in [POWERLINE.cap, POWERLINE.notch, POWERLINE.branch] {
            let point = glyph.chars().next().unwrap() as u32;
            assert!(
                (0xE0A0..=0xE0B3).contains(&point),
                "{glyph:?} at U+{point:X} is outside U+E0A0–E0B3, so powerline/fonts lacks it"
            );
        }
    }

    #[test]
    fn the_setting_picks_one_or_the_other_and_defaults_to_safe() {
        assert_eq!(Glyphs::for_setting(false), PLAIN, "off means nothing exotic is drawn");
        assert!(!PLAIN.is_flowing(), "plain tabs stay as pills");
        assert!(Glyphs::for_setting(true).is_flowing());
    }
}
