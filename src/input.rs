//! Encodes key events into the byte sequences a terminal child expects.
//!
//! Getting this wrong is invisible until you press an arrow key inside `vim`
//! and get `[[A` in your buffer. The rules are xterm's, which every terminal
//! program assumes.

use alacritty_terminal::term::TermMode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

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

/// Encodes a mouse event for a child that has asked to receive them.
///
/// Two encodings, and which one to use is the child's choice: SGR
/// (`ESC[<b;x;yM`) is unambiguous and handles coordinates past 223, so anything
/// modern asks for it. The legacy X10 form is a fallback for things that did
/// not.
///
/// `column` and `line` are zero-based within the child's grid; both wire
/// formats are one-based.
#[must_use]
pub fn encode_mouse(event: MouseEvent, column: u16, line: u16, mode: TermMode) -> Option<Vec<u8>> {
    let (button, released) = match event.kind {
        MouseEventKind::Down(button) => (mouse_button(button), false),
        MouseEventKind::Up(button) => (mouse_button(button), true),
        MouseEventKind::Drag(button) => (mouse_button(button) + 32, false),
        // Wheel buttons have no release event, by convention.
        MouseEventKind::ScrollUp => (64, false),
        MouseEventKind::ScrollDown => (65, false),
        MouseEventKind::ScrollLeft => (66, false),
        MouseEventKind::ScrollRight => (67, false),
        MouseEventKind::Moved => {
            if !mode.contains(TermMode::MOUSE_MOTION) {
                return None;
            }
            (35, false)
        }
    };

    let mut code = button;
    if event.modifiers.contains(KeyModifiers::SHIFT) {
        code += 4;
    }
    if event.modifiers.contains(KeyModifiers::ALT) {
        code += 8;
    }
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        code += 16;
    }

    if mode.contains(TermMode::SGR_MOUSE) {
        let final_byte = if released { 'm' } else { 'M' };
        return Some(format!("\x1b[<{code};{};{}{final_byte}", column + 1, line + 1).into_bytes());
    }

    // X10 offsets everything by 32 and cannot express a coordinate past 223.
    // The button byte is *not* one-based — only the coordinates are.
    let code = if released { 3 } else { code };
    let button = code.checked_add(32)?;
    let coordinate = |value: u16| -> Option<u8> { u8::try_from(value + 1 + 32).ok() };
    Some(vec![0x1b, b'[', b'M', button, coordinate(column)?, coordinate(line)?])
}

const fn mouse_button(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
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

    fn wheel(kind: MouseEventKind) -> MouseEvent {
        MouseEvent { kind, column: 0, row: 0, modifiers: KeyModifiers::NONE }
    }

    #[test]
    fn sgr_mouse_reporting_is_one_based_and_unambiguous() {
        let bytes = encode_mouse(wheel(MouseEventKind::ScrollUp), 9, 4, TermMode::SGR_MOUSE);
        assert_eq!(
            bytes,
            Some(b"\x1b[<64;10;5M".to_vec()),
            "coordinates are one-based on the wire"
        );

        let down = encode_mouse(wheel(MouseEventKind::ScrollDown), 0, 0, TermMode::SGR_MOUSE);
        assert_eq!(down, Some(b"\x1b[<65;1;1M".to_vec()));
    }

    #[test]
    fn a_release_is_distinguishable_only_in_sgr() {
        let up = MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };

        let sgr = encode_mouse(up, 0, 0, TermMode::SGR_MOUSE).unwrap();
        assert_eq!(sgr.last(), Some(&b'm'), "SGR marks a release with a lowercase m");

        // X10 cannot say which button was released, so it reports button 3.
        let legacy = encode_mouse(up, 0, 0, TermMode::MOUSE_REPORT_CLICK).unwrap();
        assert_eq!(legacy[3], 3 + 32);
    }

    #[test]
    fn modifiers_are_folded_into_the_button_code() {
        let shifted = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::SHIFT | KeyModifiers::CONTROL,
        };
        let bytes = encode_mouse(shifted, 0, 0, TermMode::SGR_MOUSE).unwrap();
        let rendered = String::from_utf8(bytes).unwrap();
        assert!(rendered.starts_with("\x1b[<20;"), "0 + shift(4) + ctrl(16), got {rendered}");
    }

    #[test]
    fn plain_movement_is_reported_only_when_the_child_tracks_motion() {
        let moved = wheel(MouseEventKind::Moved);
        assert!(encode_mouse(moved, 0, 0, TermMode::SGR_MOUSE).is_none());
        assert!(encode_mouse(moved, 0, 0, TermMode::MOUSE_MOTION | TermMode::SGR_MOUSE).is_some());
    }

    #[test]
    fn a_coordinate_past_the_legacy_limit_is_dropped_rather_than_wrapped() {
        // X10 tops out at 223. Sending a wrapped byte would put the child's
        // cursor somewhere arbitrary, which is worse than sending nothing.
        let far =
            encode_mouse(wheel(MouseEventKind::ScrollUp), 300, 0, TermMode::MOUSE_REPORT_CLICK);
        assert!(far.is_none());

        // SGR has no such limit.
        assert!(
            encode_mouse(wheel(MouseEventKind::ScrollUp), 300, 0, TermMode::SGR_MOUSE).is_some()
        );
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
