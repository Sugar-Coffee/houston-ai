//! Whether the terminal can draw powerline glyphs, and installing a font if not.
//!
//! **Houston cannot ask the terminal which font it is using.** That setting
//! lives in the terminal's own profile — a plist for iTerm2, a TOML for
//! Alacritty, a menu for Terminal.app — and there is no portable way in. So
//! this answers a narrower question that is still worth answering: does any
//! font *installed on this machine* carry the powerline block?
//!
//! A "no" there is conclusive. If nothing on disk has the glyphs, the terminal
//! certainly is not drawing them, and switching the setting on would produce
//! question marks. A "yes" is only encouraging — the terminal may still be
//! pointed at some other font — which is why the Settings hint also prints the
//! glyphs themselves and lets you judge.
//!
//! The distinction that makes this worth the trouble: a missing glyph in
//! ordinary Unicode falls back to another installed font and renders fine, but
//! a missing glyph in the private use area has nothing to fall back to, so it
//! comes out as a question mark. The powerline block is private use.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// The glyph a font must have to be useful here: the solid right-pointing
/// separator. Every powerline-patched font carries the whole block, so one
/// codepoint is a fair proxy for all of them.
const PROBE: u32 = 0xE0B0;

/// One well-known patched font, small enough to fetch as a single file.
///
/// The `powerline/fonts` repository is normally installed by cloning it and
/// running `install.sh`, which lands about a hundred and fifty families. One
/// file is 475 KB and takes a second — a better trade for somebody who just
/// wants the tab strip to look right.
const FONT_URL: &str = "https://raw.githubusercontent.com/powerline/fonts/master/\
                        Meslo%20Slashed/Meslo%20LG%20S%20Regular%20for%20Powerline.ttf";

/// What this font is called once installed, for the instructions afterwards.
pub const FONT_NAME: &str = "Meslo LG S for Powerline";

const FONT_FILE: &str = "Meslo LG S Regular for Powerline.ttf";

/// What a scan of the installed fonts found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detection {
    /// At least one installed font has the glyphs. Names one, for the hint.
    Found { family: String },
    /// Nothing installed has them, so turning the setting on would show
    /// question marks.
    Missing,
    /// The font directories could not be read, so nothing can be claimed.
    Unknown,
}

/// Where a user's own fonts live on this platform.
#[must_use]
pub fn install_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;

    Some(if cfg!(target_os = "macos") {
        home.join("Library/Fonts")
    } else {
        home.join(".local/share/fonts")
    })
}

/// Every directory worth scanning for installed fonts.
fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(user) = install_dir() {
        dirs.push(user);
    }
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/Library/Fonts"));
        dirs.push(PathBuf::from("/System/Library/Fonts"));
    } else {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
    }
    dirs
}

/// Scans the installed fonts for the powerline block.
#[must_use]
pub fn detect() -> Detection {
    detect_in(&font_dirs())
}

/// Scans explicit directories. A parameter so tests never depend on what
/// happens to be installed on the machine running them.
#[must_use]
pub fn detect_in(dirs: &[PathBuf]) -> Detection {
    let mut looked = false;

    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        looked = true;

        let mut candidates: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|p| is_font(p))
            .collect();

        // Read the plausible names first. Answering "yes" means stopping at
        // the first match, and a directory of a hundred and fifty fonts is a
        // hundred and fifty file reads if the order is unlucky.
        candidates.sort_by_key(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            u8::from(!(name.contains("powerline") || name.contains("nerd")))
        });

        for path in candidates {
            if covers(&path, PROBE) {
                let family = path.file_stem().map_or_else(
                    || "a font".to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                return Detection::Found { family };
            }
        }
    }

    if looked { Detection::Missing } else { Detection::Unknown }
}

fn is_font(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "ttf" | "otf"))
}

/// Whether a font file maps `codepoint` to a real glyph.
///
/// Reads the `cmap` table directly rather than taking a dependency. Only the
/// two subtable formats that matter are handled — 4 for the basic plane and 12
/// for the extended one — and anything unparseable answers `false`, because
/// "we could not tell" and "it is not there" lead to the same advice.
#[must_use]
pub fn covers(path: &Path, codepoint: u32) -> bool {
    std::fs::read(path).is_ok_and(|data| cmap_covers(&data, codepoint))
}

/// The parsing half, split out so it can be tested against bytes rather than
/// against whatever fonts the machine happens to have.
#[must_use]
pub fn cmap_covers(data: &[u8], codepoint: u32) -> bool {
    let read16 = |at: usize| -> Option<u32> {
        Some(u32::from(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?])))
    };
    let read32 = |at: usize| -> Option<u32> {
        Some(u32::from_be_bytes([
            *data.get(at)?,
            *data.get(at + 1)?,
            *data.get(at + 2)?,
            *data.get(at + 3)?,
        ]))
    };

    let Some(magic) = read32(0) else { return false };
    // TrueType, CFF, and the old Mac flavour. A collection is not handled.
    if !matches!(magic, 0x0001_0000 | 0x4F54_544F | 0x7472_7565) {
        return false;
    }

    let Some(tables) = read16(4) else { return false };
    let mut cmap = None;
    for index in 0..tables as usize {
        let record = 12 + index * 16;
        if data.get(record..record + 4) == Some(b"cmap") {
            cmap = read32(record + 8).map(|offset| offset as usize);
            break;
        }
    }
    let Some(cmap) = cmap else { return false };

    let Some(subtables) = read16(cmap + 2) else { return false };
    for index in 0..subtables as usize {
        let record = cmap + 4 + index * 8;
        let Some(offset) = read32(record + 4) else { continue };
        let sub = cmap + offset as usize;

        match read16(sub) {
            Some(4) if codepoint <= 0xFFFF && format4(data, sub, codepoint) => return true,
            Some(12) if format12(data, sub, codepoint) => return true,
            _ => {}
        }
    }
    false
}

/// Format 4: segmented mapping over the basic plane.
///
/// The glyph id is resolved properly rather than stopping at "the codepoint
/// falls inside a segment". Segments routinely span ranges where most entries
/// map to glyph zero, so the cheap check reports coverage a font does not have.
fn format4(data: &[u8], sub: usize, codepoint: u32) -> bool {
    let read16 = |at: usize| -> Option<u32> {
        Some(u32::from(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?])))
    };

    let Some(segments) = read16(sub + 6).map(|count| count as usize / 2) else { return false };
    let ends = sub + 14;
    let starts = ends + segments * 2 + 2;
    let deltas = starts + segments * 2;
    let ranges = deltas + segments * 2;

    for segment in 0..segments {
        let (Some(end), Some(start)) = (read16(ends + segment * 2), read16(starts + segment * 2))
        else {
            return false;
        };
        if codepoint > end || codepoint < start {
            continue;
        }
        // The final segment is a 0xFFFF sentinel, never a real mapping.
        if start == 0xFFFF {
            return false;
        }

        let Some(delta) = read16(deltas + segment * 2) else { return false };
        let Some(range) = read16(ranges + segment * 2) else { return false };

        let glyph = if range == 0 {
            (codepoint + delta) & 0xFFFF
        } else {
            let at = ranges + segment * 2 + range as usize + (codepoint - start) as usize * 2;
            match read16(at) {
                Some(0) | None => return false,
                Some(found) => (found + delta) & 0xFFFF,
            }
        };
        return glyph != 0;
    }
    false
}

/// Format 12: grouped ranges, for anything beyond the basic plane.
fn format12(data: &[u8], sub: usize, codepoint: u32) -> bool {
    let read32 = |at: usize| -> Option<u32> {
        Some(u32::from_be_bytes([
            *data.get(at)?,
            *data.get(at + 1)?,
            *data.get(at + 2)?,
            *data.get(at + 3)?,
        ]))
    };

    let Some(groups) = read32(sub + 12) else { return false };
    for index in 0..groups.min(100_000) as usize {
        let group = sub + 16 + index * 12;
        let (Some(start), Some(end), Some(glyph)) =
            (read32(group), read32(group + 4), read32(group + 8))
        else {
            return false;
        };
        if (start..=end).contains(&codepoint) {
            return glyph != 0;
        }
    }
    false
}

/// Downloads one powerline-patched font into the user's font directory.
///
/// Shells out to `curl`, which is present on macOS and on essentially every
/// Linux that has a terminal worth using — the same reasoning that has
/// `worktree` shelling out to `git` rather than linking a library.
pub fn install() -> Result<PathBuf> {
    let dir = install_dir().context("HOME is not set, so there is nowhere to install a font")?;
    install_into(&dir)
}

/// Installs into an explicit directory, so tests never write to the real one.
pub fn install_into(dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;

    let destination = dir.join(FONT_FILE);
    if destination.exists() {
        return Ok(destination);
    }

    // To a temporary name first: a half-written file in the font directory is
    // a font the system will try to load.
    let partial = dir.join(format!("{FONT_FILE}.part"));
    let status = std::process::Command::new("curl")
        .args(["--fail", "--location", "--silent", "--show-error", "--max-time", "120", "--output"])
        .arg(&partial)
        .arg(FONT_URL)
        .status()
        .context("could not run curl — install the font by hand from powerline/fonts")?;

    if !status.success() {
        let _ = std::fs::remove_file(&partial);
        bail!("could not download the font — check your connection, or install it by hand");
    }

    // Trust nothing: a captive portal will happily return an HTML error page
    // with a 200, and naming that `.ttf` would be worse than failing.
    let downloaded = std::fs::read(&partial).context("the download could not be read back")?;
    if !cmap_covers(&downloaded, PROBE) {
        let _ = std::fs::remove_file(&partial);
        bail!("what downloaded was not a powerline font — install it by hand instead");
    }

    std::fs::rename(&partial, &destination)
        .with_context(|| format!("could not move the font into {}", dir.display()))?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal TrueType file whose format-4 cmap covers one range.
    ///
    /// Bytes rather than a fixture file, and certainly rather than whatever is
    /// installed on the machine running the suite — a test that passes only on
    /// a laptop with the right fonts is not a test.
    const PROBE_16: u16 = 0xE0B0;

    fn font_covering(range: std::ops::RangeInclusive<u16>, maps_to_glyph: bool) -> Vec<u8> {
        let (first, last) = (*range.start(), *range.end());
        let segments: u16 = 2; // the range, then the mandatory 0xFFFF sentinel

        let mut sub: Vec<u8> = Vec::new();
        sub.extend(4u16.to_be_bytes()); // format
        sub.extend(0u16.to_be_bytes()); // length, patched below
        sub.extend(0u16.to_be_bytes()); // language
        sub.extend((segments * 2).to_be_bytes());
        sub.extend([0u8; 6]); // searchRange, entrySelector, rangeShift
        sub.extend(last.to_be_bytes());
        sub.extend(0xFFFFu16.to_be_bytes()); // sentinel end
        sub.extend(0u16.to_be_bytes()); // reservedPad
        sub.extend(first.to_be_bytes());
        sub.extend(0xFFFFu16.to_be_bytes()); // sentinel start
        // idDelta 0 leaves glyph == codepoint. To make the probe resolve to
        // glyph zero instead, pick the delta that wraps it exactly to zero —
        // which is what a real font does for a codepoint it does not draw.
        let delta: u16 = if maps_to_glyph {
            0
        } else {
            u16::try_from(0x1_0000u32 - u32::from(PROBE_16)).unwrap_or(0)
        };
        sub.extend(delta.to_be_bytes());
        sub.extend(1u16.to_be_bytes());
        sub.extend([0u8; 4]); // idRangeOffset for both segments
        let length = u16::try_from(sub.len()).unwrap();
        sub[2..4].copy_from_slice(&length.to_be_bytes());

        let mut cmap: Vec<u8> = Vec::new();
        cmap.extend(0u16.to_be_bytes()); // version
        cmap.extend(1u16.to_be_bytes()); // one subtable
        cmap.extend(3u16.to_be_bytes()); // platform
        cmap.extend(1u16.to_be_bytes()); // encoding
        cmap.extend(12u32.to_be_bytes()); // offset to the subtable
        cmap.extend(sub);

        let mut font: Vec<u8> = Vec::new();
        font.extend(0x0001_0000u32.to_be_bytes());
        font.extend(1u16.to_be_bytes()); // one table
        font.extend([0u8; 6]); // searchRange, entrySelector, rangeShift
        font.extend(b"cmap");
        font.extend(0u32.to_be_bytes()); // checksum
        font.extend(28u32.to_be_bytes()); // offset
        font.extend(u32::try_from(cmap.len()).unwrap().to_be_bytes());
        assert_eq!(font.len(), 28, "the cmap has to start where the record says");
        font.extend(cmap);
        font
    }

    #[test]
    fn a_font_carrying_the_separator_is_recognised() {
        let font = font_covering(0xE0A0..=0xE0B3, true);

        assert!(cmap_covers(&font, PROBE), "U+E0B0 is inside the range this font maps");
        assert!(cmap_covers(&font, 0xE0A0), "and so is the branch glyph");
    }

    #[test]
    fn a_font_without_the_block_is_not_mistaken_for_one_that_has_it() {
        let font = font_covering(0x0020..=0x007E, true);

        assert!(!cmap_covers(&font, PROBE), "ASCII only, so no powerline glyphs");
        assert!(cmap_covers(&font, 0x0041), "but it does have the letter A");
    }

    /// The cheap version of this check — "the codepoint falls inside a
    /// segment" — reports coverage a font does not have, because segments
    /// routinely span ranges most of which map to glyph zero.
    #[test]
    fn a_segment_that_maps_to_no_glyph_does_not_count_as_coverage() {
        let font = font_covering(0xE0A0..=0xE0B3, false);

        assert!(
            !cmap_covers(&font, PROBE),
            "the segment contains it but resolves to glyph zero, which draws nothing"
        );
    }

    #[test]
    fn the_final_sentinel_segment_is_never_a_real_mapping() {
        let font = font_covering(0xE0A0..=0xE0B3, true);

        assert!(
            !cmap_covers(&font, 0xFFFF),
            "every format 4 cmap ends with a 0xFFFF segment that means nothing"
        );
    }

    #[test]
    fn anything_that_is_not_a_font_is_refused_rather_than_parsed() {
        assert!(!cmap_covers(b"", PROBE), "an empty file");
        assert!(!cmap_covers(b"<!doctype html><html>oops</html>", PROBE), "a captive portal page");
        assert!(!cmap_covers(&[0xFF; 64], PROBE), "noise");
    }

    #[test]
    fn a_directory_with_no_fonts_reports_missing_not_unknown() {
        let dir = std::env::temp_dir().join("houston-fonts-empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        assert_eq!(
            detect_in(std::slice::from_ref(&dir)),
            Detection::Missing,
            "the directory was readable and had nothing, which is a definite answer"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn directories_that_cannot_be_read_are_unknown_rather_than_missing() {
        let nowhere = PathBuf::from("/definitely/not/a/font/directory");

        assert_eq!(
            detect_in(std::slice::from_ref(&nowhere)),
            Detection::Unknown,
            "claiming a font is missing because we could not look would be a lie"
        );
    }

    #[test]
    fn a_font_in_the_directory_is_found_and_named() {
        let dir = std::env::temp_dir().join("houston-fonts-found");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Test Powerline.ttf"), font_covering(0xE0A0..=0xE0B3, true))
            .unwrap();

        match detect_in(std::slice::from_ref(&dir)) {
            Detection::Found { family } => assert_eq!(family, "Test Powerline"),
            other => panic!("expected the font to be found, got {other:?}"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_is_not_a_font_is_skipped_rather_than_failing_the_scan() {
        let dir = std::env::temp_dir().join("houston-fonts-mixed");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("readme.txt"), "not a font").unwrap();
        std::fs::write(dir.join("broken.ttf"), "also not a font").unwrap();
        std::fs::write(dir.join("Good.ttf"), font_covering(0xE0A0..=0xE0B3, true)).unwrap();

        assert!(matches!(detect_in(std::slice::from_ref(&dir)), Detection::Found { .. }));

        std::fs::remove_dir_all(&dir).ok();
    }
}
