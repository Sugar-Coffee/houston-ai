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

/// Separators, from the powerline symbols block U+E0A0–U+E0B3.
///
/// **A smaller ask than the icons below.** The [powerline/fonts][pf]
/// collection patches exactly this block into a couple of dozen families and
/// nothing else, and plenty of people installed it years ago for a shell
/// prompt without ever meeting a Nerd Font.
///
/// [pf]: https://github.com/powerline/fonts
const SEPARATORS: Glyphs =
    Glyphs { cap: "\u{e0b0}", notch: "\u{e0b2}", branch: "\u{e0a0}", ..PLAIN };

/// Icons, from the Font Awesome 4 block.
///
/// **A bigger ask, and a separate one.** This block only exists in a
/// [Nerd Font][nf]; a powerline-patched font does not have it. Conflating the
/// two is how a release shipped icons to somebody whose separators worked
/// perfectly — the tabs were right and the vault list was a row of boxes.
///
/// Codepoints stay inside Font Awesome 4 because Nerd Fonts moved several
/// ranges between v2 and v3, and a glyph only in the newest patch set draws a
/// box for anyone on an older one.
///
/// [nf]: https://www.nerdfonts.com/
const ICONS: Glyphs =
    Glyphs { folder: "\u{f07b}", note: "\u{f15c}", agent: "\u{f0e7}", shell: "\u{f120}", ..PLAIN };

/// Everything on, for anyone running a Nerd Font.
///
/// Only the tests name this directly; the app composes the two settings.
#[cfg(test)]
pub const POWERLINE: Glyphs = Glyphs {
    cap: SEPARATORS.cap,
    notch: SEPARATORS.notch,
    branch: SEPARATORS.branch,
    folder: ICONS.folder,
    note: ICONS.note,
    agent: ICONS.agent,
    shell: ICONS.shell,
    segmented: true,
};

/// A sample of the separator glyphs, for the Settings hint.
///
/// **The preview is the detection.** Houston cannot ask the terminal which
/// font it is using, and a missing glyph usually renders as a one-cell
/// replacement box, so measuring cursor advance does not distinguish it
/// either. Putting the actual characters in the hint sidesteps all of that:
/// you look at the row, and if it is boxes you know before you turn it on.
pub const SEPARATOR_SAMPLE: &str = "\u{e0b0} \u{e0b2} \u{e0a0}";

/// A sample of the icon glyphs. Different font, so a separate sample.
pub const ICON_SAMPLE: &str = "\u{f07b} \u{f15c} \u{f120} \u{f0e7}";

/// Builds the glyph set from the two independent settings.
///
/// Two settings rather than one because they are two different fonts. A
/// powerline-patched font has the separators and not the icons, which is the
/// common case for anybody who set up a shell prompt before Nerd Fonts
/// existed.
#[must_use]
pub const fn resolve(separators: bool, icons: bool) -> Glyphs {
    Glyphs {
        cap: if separators { SEPARATORS.cap } else { PLAIN.cap },
        notch: if separators { SEPARATORS.notch } else { PLAIN.notch },
        branch: if separators { SEPARATORS.branch } else { PLAIN.branch },
        folder: if icons { ICONS.folder } else { PLAIN.folder },
        note: if icons { ICONS.note } else { PLAIN.note },
        agent: if icons { ICONS.agent } else { PLAIN.agent },
        shell: if icons { ICONS.shell } else { PLAIN.shell },
        segmented: separators,
    }
}

impl Glyphs {
    /// The style for a pair of settings.
    #[must_use]
    pub const fn for_setting(separators: bool, icons: bool) -> Self {
        resolve(separators, icons)
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
    fn both_settings_off_draws_nothing_exotic() {
        assert_eq!(Glyphs::for_setting(false, false), PLAIN);
        assert!(!PLAIN.is_flowing(), "plain tabs stay as pills");
    }

    /// The bug this split exists to prevent: a powerline-patched font has the
    /// separators and not the icons, and turning on one used to turn on both.
    #[test]
    fn separators_can_be_on_while_icons_stay_off() {
        let separators_only = Glyphs::for_setting(true, false);

        assert_eq!(separators_only.cap, SEPARATORS.cap, "the tabs get their arrows");
        assert!(separators_only.is_flowing());
        assert_eq!(
            separators_only.folder, PLAIN.folder,
            "and the vault list stays on characters a powerline font actually has"
        );
        assert_eq!(separators_only.shell, PLAIN.shell);
    }

    #[test]
    fn icons_can_be_on_while_separators_stay_off() {
        let icons_only = Glyphs::for_setting(false, true);

        assert_eq!(icons_only.folder, ICONS.folder);
        assert_eq!(icons_only.cap, PLAIN.cap, "no arrows without the separator setting");
        assert!(!icons_only.is_flowing(), "and tabs stay as pills");
    }

    #[test]
    fn both_on_is_the_full_set() {
        assert_eq!(Glyphs::for_setting(true, true), POWERLINE);
    }

    /// The two blocks come from different fonts, so nothing may straddle them.
    #[test]
    fn separators_and_icons_live_in_ranges_that_do_not_overlap() {
        for glyph in [SEPARATORS.cap, SEPARATORS.notch, SEPARATORS.branch] {
            let point = glyph.chars().next().unwrap() as u32;
            assert!(
                (0xE0A0..=0xE0D4).contains(&point),
                "{glyph:?} is outside the powerline block, so a powerline font would not have it"
            );
        }

        for glyph in [ICONS.folder, ICONS.note, ICONS.agent, ICONS.shell] {
            let point = glyph.chars().next().unwrap() as u32;
            assert!(
                (0xF000..=0xF2FF).contains(&point),
                "{glyph:?} at U+{point:X} is outside the Font Awesome 4 block"
            );
        }
    }
}
