//! Proves the hook payload survives the whole trip: agent pipes JSON to
//! `houston notify`, and the conversation id comes out the other end of the
//! socket.
//!
//! This lives outside the unit tests on purpose. Every unit test passed while
//! the shipped binary sent `"conversation": null` — because the bug was in the
//! wiring, not in any single function. `parse` read stdin (draining the pipe)
//! and then `dispatch` read the empty remains. Only running the real binary
//! catches that.

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};

#[test]
fn a_piped_hook_payload_reaches_the_socket_as_a_conversation_id() {
    let socket = std::env::temp_dir().join(format!("houston-hook-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).expect("bind");

    let mut child = Command::new(env!("CARGO_BIN_EXE_houston"))
        .args(["notify", "working", "--session", "7", "--socket"])
        .arg(&socket)
        .stdin(Stdio::piped())
        .spawn()
        .expect("spawn");

    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(br#"{"session_id":"11111111-2222-3333-4444-555555555555","cwd":"/tmp"}"#)
        .expect("write payload");

    let mut received = String::new();
    listener.accept().expect("accept").0.read_to_string(&mut received).expect("read");
    child.wait().expect("wait");
    let _ = std::fs::remove_file(&socket);

    assert!(
        received.contains("11111111-2222-3333-4444-555555555555"),
        "the conversation id should have travelled, got: {received}"
    );
}
