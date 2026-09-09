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
    /// A directory.
    pub folder: &'static str,
    /// A note in the vault.
    pub note: &'static str,
    /// A session running an AI agent.
    pub agent: &'static str,
    /// A session running a shell.
    pub shell: &'static str,
    /// Whether segments should be drawn as flowing blocks at all.
    pub segmented: bool,
}

/// The plain style: no glyph outside the ASCII-and-common-symbols range that
/// every terminal already draws.
pub const PLAIN: Glyphs = Glyphs {
    cap: "",
    notch: "",
    branch: "⑂",
    folder: "▪",
    note: "·",
    agent: "",
    shell: "$",
    segmented: false,
};

/// The powerline style. Needs a Nerd Font.
///
/// **Deliberately conservative codepoints.** Every glyph here is from the
/// Font Awesome 4 block that Nerd Fonts have carried since the beginning,
/// rather than the Material or Octicon ranges that moved between v2 and v3.
/// A glyph that is only in the newest patch set draws a box for anyone on an
/// older font, which is exactly the failure this whole style is trying to keep
/// behind an opt-in.
pub const POWERLINE: Glyphs = Glyphs {
    cap: "\u{e0b0}",
    notch: "\u{e0b2}",
    branch: "\u{e0a0}",
    folder: "\u{f07b}",
    note: "\u{f15c}",
    agent: "\u{f0e7}",
    shell: "\u{f120}",
    segmented: true,
};

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
        let plain = [
            PLAIN.cap,
            PLAIN.notch,
            PLAIN.branch,
            PLAIN.folder,
            PLAIN.note,
            PLAIN.agent,
            PLAIN.shell,
        ];
        for glyph in plain {
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

    /// Nerd Fonts moved several ranges between v2 and v3. Everything here is
    /// from blocks that survived, so an older patched font still draws them.
    #[test]
    fn every_icon_comes_from_a_range_older_nerd_fonts_also_carry() {
        for glyph in [POWERLINE.folder, POWERLINE.note, POWERLINE.agent, POWERLINE.shell] {
            let point = glyph.chars().next().unwrap() as u32;
            assert!(
                (0xF000..=0xF2FF).contains(&point),
                "{glyph:?} at U+{point:X} is outside the Font Awesome 4 block"
            );
        }

        for glyph in [POWERLINE.cap, POWERLINE.notch, POWERLINE.branch] {
            let point = glyph.chars().next().unwrap() as u32;
            assert!((0xE0A0..=0xE0D4).contains(&point), "{glyph:?} is not a powerline glyph");
        }
    }

    /// Every icon has to have a plain counterpart, or turning the setting off
    /// would silently lose information rather than only losing decoration.
    #[test]
    fn nothing_is_only_expressible_with_a_nerd_font() {
        assert!(!PLAIN.folder.is_empty(), "a folder still reads as a folder without the font");
        assert!(!PLAIN.note.is_empty());
        assert!(!PLAIN.shell.is_empty(), "a shell is still marked as one");
        assert!(
            PLAIN.agent.is_empty(),
            "an agent is the unmarked default, which is what makes the shell marker mean something"
        );
    }

    #[test]
    fn the_setting_picks_one_or_the_other_and_defaults_to_safe() {
        assert_eq!(Glyphs::for_setting(false), PLAIN, "off means nothing exotic is drawn");
        assert!(!PLAIN.is_flowing(), "plain tabs stay as pills");
        assert!(Glyphs::for_setting(true).is_flowing());
    }
}
