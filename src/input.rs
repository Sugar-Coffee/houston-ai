//! Encodes key events into the byte sequences a terminal child expects.
//!
//! Getting this wrong is invisible until you press an arrow key inside `vim`
//! and get `[[A` in your buffer. The rules are xterm's, which every terminal
//! program assumes.

use alacritty_terminal::term::TermMode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Encodes a key press. Returns `None` for keys with no terminal meaning
/// (modifier presses on their own, media keys, and so on).
#[must_use]
pub fn encode(key: KeyEvent, mode: TermMode) -> Option<Vec<u8>> {
    let modifiers = key.modifiers;
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let alt = modifiers.contains(KeyModifiers::ALT);

    // Cursor keys switch between CSI (`ESC[A`) and SS3 (`ESC OA`) form
    // depending on a mode the child sets. `vim` and `less` rely on this.
    let cursor_prefix = if mode.contains(TermMode::APP_CURSOR) { b"\x1bO" } else { b"\x1b[" };

    let bytes = match key.code {
        KeyCode::Char(character) => return Some(encode_char(character, ctrl, alt)),

        KeyCode::Enter => vec![b'\r'],
        // Terminals send DEL for backspace, not BS. Ctrl-Backspace sends BS.
        KeyCode::Backspace if ctrl => vec![0x08],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Esc => vec![0x1b],
        KeyCode::Null => vec![0],

        KeyCode::Up
        | KeyCode::Down
        | KeyCode::Right
        | KeyCode::Left
        | KeyCode::Home
        | KeyCode::End => {
            let final_byte = match key.code {
                KeyCode::Up => b'A',
                KeyCode::Down => b'B',
                KeyCode::Right => b'C',
                KeyCode::Left => b'D',
                KeyCode::Home => b'H',
                _ => b'F',
            };
            // Modified cursor keys are always CSI form, never SS3.
            modifier_code(modifiers).map_or_else(
                || {
                    let mut bytes = cursor_prefix.to_vec();
                    bytes.push(final_byte);
                    bytes
                },
                |code| format!("\x1b[1;{code}{}", final_byte as char).into_bytes(),
            )
        }

        KeyCode::Insert => tilde(2, modifiers),
        KeyCode::Delete => tilde(3, modifiers),
        KeyCode::PageUp => tilde(5, modifiers),
        KeyCode::PageDown => tilde(6, modifiers),

        KeyCode::F(number @ 1..=4) => {
            let final_byte = b'P' + (number - 1);
            modifier_code(modifiers).map_or_else(
                || vec![0x1b, b'O', final_byte],
                |code| format!("\x1b[1;{code}{}", final_byte as char).into_bytes(),
            )
        }
        KeyCode::F(number) => {
            // xterm's numbering skips 16, 22, 27, 30 and 35.
            let parameter = match number {
                5 => 15,
                6..=10 => u16::from(number) + 11,
                11..=16 => u16::from(number) + 12,
                17..=20 => u16::from(number) + 13,
                _ => return None,
            };
            tilde(parameter, modifiers)
        }

        _ => return None,
    };

    Some(bytes)
}

/// `ESC[<parameter>~`, with an optional modifier argument.
fn tilde(parameter: u16, modifiers: KeyModifiers) -> Vec<u8> {
    modifier_code(modifiers).map_or_else(
        || format!("\x1b[{parameter}~").into_bytes(),
        |code| format!("\x1b[{parameter};{code}~").into_bytes(),
    )
}

/// xterm's modifier parameter: 1 + shift(1) + alt(2) + ctrl(4).
///
/// `None` means unmodified, which uses the shorter escape form.
fn modifier_code(modifiers: KeyModifiers) -> Option<u8> {
    let mut code = 0;
    if modifiers.contains(KeyModifiers::SHIFT) {
        code |= 1;
    }
    if modifiers.contains(KeyModifiers::ALT) {
        code |= 2;
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        code |= 4;
    }
    (code != 0).then_some(code + 1)
}

fn encode_char(character: char, ctrl: bool, alt: bool) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4);

    // Meta is sent as an ESC prefix, which is how every terminal does it.
    if alt {
        bytes.push(0x1b);
    }

    if ctrl {
        // Ctrl masks off the top three bits: Ctrl-A is 0x01, Ctrl-Z is 0x1a.
        // The symbolic cases below are the ones that mapping does not cover.
        let control = match character {
            ' ' | '@' => Some(0x00),
            '[' => Some(0x1b),
            '\\' => Some(0x1c),
            ']' => Some(0x1d),
            '^' => Some(0x1e),
            '_' | '?' => Some(0x1f),
            'a'..='z' => Some(character as u8 - b'a' + 1),
            'A'..='Z' => Some(character as u8 - b'A' + 1),
            _ => None,
        };
        if let Some(byte) = control {
            bytes.push(byte);
            return bytes;
        }
    }

    let mut buffer = [0u8; 4];
    bytes.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
    bytes
}

/// Wraps pasted text so the child receives it as one atomic block.
///
/// This is the heart of ADR-0005. Two things matter:
///
/// - The payload is scanned for a paste terminator. Without that, a crafted
///   clipboard could close the bracket early and have the remainder run as
///   typed input — in a shell session, that is command execution.
/// - `\r\n` and `\n` both become `\r`, which is what a terminal delivers for
///   Return. Passing `\n` through makes some readers see a literal newline
///   instead of a submit.
#[must_use]
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    let cleaned = text.replace("\x1b[201~", "").replace("\r\n", "\r").replace('\n', "\r");

    if !bracketed {
        return cleaned.into_bytes();
    }

    let mut bytes = Vec::with_capacity(cleaned.len() + 12);
    bytes.extend_from_slice(b"\x1b[200~");
    bytes.extend_from_slice(cleaned.as_bytes());
    bytes.extend_from_slice(b"\x1b[201~");
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn plain(code: KeyCode) -> Option<Vec<u8>> {
        encode(key(code, KeyModifiers::NONE), TermMode::default())
    }

    #[test]
    fn plain_characters_are_utf8() {
        assert_eq!(plain(KeyCode::Char('a')), Some(b"a".to_vec()));
        assert_eq!(plain(KeyCode::Char('£')), Some("£".as_bytes().to_vec()));
    }

    #[test]
    fn control_characters_use_the_ascii_mask() {
        let ctrl_c = encode(key(KeyCode::Char('c'), KeyModifiers::CONTROL), TermMode::default());
        assert_eq!(ctrl_c, Some(vec![0x03]));

        let ctrl_d = encode(key(KeyCode::Char('d'), KeyModifiers::CONTROL), TermMode::default());
        assert_eq!(ctrl_d, Some(vec![0x04]));

        // Ctrl-Space is NUL, which the a-z mask does not produce.
        let ctrl_space =
            encode(key(KeyCode::Char(' '), KeyModifiers::CONTROL), TermMode::default());
        assert_eq!(ctrl_space, Some(vec![0x00]));
    }

    #[test]
    fn alt_prefixes_with_escape() {
        let alt_b = encode(key(KeyCode::Char('b'), KeyModifiers::ALT), TermMode::default());
        assert_eq!(alt_b, Some(vec![0x1b, b'b']));
    }

    #[test]
    fn backspace_sends_del_not_bs() {
        assert_eq!(plain(KeyCode::Backspace), Some(vec![0x7f]));
    }

    #[test]
    fn cursor_keys_follow_application_mode() {
        assert_eq!(plain(KeyCode::Up), Some(b"\x1b[A".to_vec()));

        let app = encode(key(KeyCode::Up, KeyModifiers::NONE), TermMode::APP_CURSOR);
        assert_eq!(app, Some(b"\x1bOA".to_vec()));
    }

    #[test]
    fn modified_cursor_keys_are_always_csi() {
        // Even in application mode, a modified arrow uses the CSI form.
        let shift_up = encode(key(KeyCode::Up, KeyModifiers::SHIFT), TermMode::APP_CURSOR);
        assert_eq!(shift_up, Some(b"\x1b[1;2A".to_vec()));

        let ctrl_right = encode(key(KeyCode::Right, KeyModifiers::CONTROL), TermMode::default());
        assert_eq!(ctrl_right, Some(b"\x1b[1;5C".to_vec()));
    }

    #[test]
    fn function_keys_split_at_f5() {
        assert_eq!(plain(KeyCode::F(1)), Some(b"\x1bOP".to_vec()));
        assert_eq!(plain(KeyCode::F(4)), Some(b"\x1bOS".to_vec()));
        assert_eq!(plain(KeyCode::F(5)), Some(b"\x1b[15~".to_vec()));
        assert_eq!(plain(KeyCode::F(12)), Some(b"\x1b[24~".to_vec()));
    }

    #[test]
    fn navigation_keys_use_tilde_sequences() {
        assert_eq!(plain(KeyCode::Delete), Some(b"\x1b[3~".to_vec()));
        assert_eq!(plain(KeyCode::PageUp), Some(b"\x1b[5~".to_vec()));
    }

    #[test]
    fn paste_is_bracketed_when_the_child_asks() {
        let bytes = encode_paste("hello", true);
        assert_eq!(bytes, b"\x1b[200~hello\x1b[201~".to_vec());

        let raw = encode_paste("hello", false);
        assert_eq!(raw, b"hello".to_vec());
    }

    #[test]
    fn paste_cannot_be_escaped_by_a_crafted_clipboard() {
        // A clipboard containing the terminator would otherwise close the
        // bracket early and have `rm -rf /` arrive as typed input.
        let hostile = "safe\x1b[201~rm -rf /";
        let bytes = encode_paste(hostile, true);
        let rendered = String::from_utf8(bytes).unwrap();

        assert_eq!(rendered.matches("\x1b[201~").count(), 1, "only our own terminator may appear");
        assert!(rendered.ends_with("\x1b[201~"), "the terminator must be last");
        assert!(rendered.contains("saferm -rf /"), "the text itself is preserved");
    }

    #[test]
    fn paste_normalises_line_endings_to_carriage_returns() {
        assert_eq!(encode_paste("a\r\nb\nc", false), b"a\rb\rc".to_vec());
    }
}
