//! Stub overlay variant used only in the restart-loop integration test.
//!
//! Behavior: reads one NDJSON message from stdin, writes one Pong to
//! stdout, then exits with a NON-ZERO status to simulate a crash. The
//! integration test asserts that the daemon's overlay supervisor
//! observes the unexpected exit, respawns the binary, and that a second
//! message round-trips through the new child.
//!
//! Because this binary always exits immediately after the first
//! message, the supervisor will always treat the exit as unexpected
//! (status != 0) and trigger the restart path. After the test sends a
//! second message it observes a second Pong, confirming the restart
//! happened.

use cue_core::overlay_ipc::{decode_ndjson, OverlayIpcCommand};
use std::io::{BufRead, BufReader, Write};

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut out = stdout.lock();
    let mut line = String::new();

    // Read one message, ack with Pong, then exit non-zero.
    match reader.read_line(&mut line) {
        Ok(0) => std::process::exit(2), // EOF before any input
        Ok(_) => {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                std::process::exit(3);
            }
            if decode_ndjson(trimmed).is_err() {
                std::process::exit(4);
            }
            let serialized = serde_json::to_string(&OverlayIpcCommand::Pong).unwrap();
            let _ = writeln!(out, "{serialized}");
            let _ = out.flush();
            std::process::exit(7); // simulate crash
        }
        Err(_) => std::process::exit(5),
    }
}
