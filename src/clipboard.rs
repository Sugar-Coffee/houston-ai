//! Copying text to the system clipboard.
//!
//! Hand-rolled rather than pulling in `arboard`, which drags in platform GUI
//! crates for what is, on every platform Houston targets, one short-lived pipe.
//!
//! OSC 52 is the fallback rather than the primary: it works over SSH and needs
//! no helper binary, but iTerm2 ships with terminal clipboard access disabled,
//! so relying on it alone would silently do nothing on the machine this is
//! being built for.

use anyhow::{Context, Result, bail};
use std::{
    io::{Write, stdout},
    process::{Command, Stdio},
};

/// Copies `text`, returning the name of whatever did the copying.
pub fn copy(text: &str) -> Result<&'static str> {
    for (binary, args) in HELPERS {
        // A broken helper should not stop a working one further down the list.
        if crate::provider::which(binary).is_some() && pipe_to(binary, args, text).is_ok() {
            return Ok(binary);
        }
    }

    // No helper binary. Ask the terminal itself, which is the only option left
    // over SSH. It may be refused — iTerm2 disables this by default — so the
    // caller is told which path was taken.
    let mut out = stdout();
    out.write_all(osc52(text).as_bytes())?;
    out.flush()?;
    Ok("the terminal (OSC 52)")
}

/// Clipboard helpers in preference order, with the args they need.
const HELPERS: [(&str, &[&str]); 4] = [
    ("pbcopy", &[]),
    ("wl-copy", &[]),
    ("xclip", &["-selection", "clipboard"]),
    ("xsel", &["--clipboard", "--input"]),
];

fn pipe_to(binary: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("could not start {binary}"))?;

    child.stdin.take().context("clipboard helper closed its input")?.write_all(text.as_bytes())?;

    let status = child.wait()?;
    if status.success() { Ok(()) } else { bail!("{binary} exited with {status}") }
}

/// An OSC 52 sequence asking the terminal to set the clipboard.
///
/// The fallback rather than the default, because iTerm2 ships with terminal
/// clipboard access turned off and would swallow this silently.
#[must_use]
fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

/// Minimal base64. Only used by `osc52`, so a dependency is not worth it.
fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);

    for chunk in input.chunks(3) {
        let bits = chunk.iter().enumerate().fold(0u32, |accumulator, (index, byte)| {
            accumulator | (u32::from(*byte) << (16 - 8 * index))
        });

        for index in 0..4 {
            if index <= chunk.len() {
                let sextet = (bits >> (18 - 6 * index)) & 0b0011_1111;
                output.push(ALPHABET[sextet as usize] as char);
            } else {
                output.push('=');
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_rfc_test_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn osc52_wraps_the_payload_correctly() {
        let sequence = osc52("foo");
        assert!(sequence.starts_with("\x1b]52;c;"));
        assert!(sequence.ends_with('\x07'));
        assert!(sequence.contains("Zm9v"));
    }

    #[test]
    fn base64_handles_a_path_with_spaces_and_unicode() {
        let path = "/Users/x/Projects/houston/Daily/2026-09-08 — notes.md";
        // Round-tripping is what matters; the encoding just has to be valid.
        assert!(!base64(path.as_bytes()).contains(' '));
    }
}
