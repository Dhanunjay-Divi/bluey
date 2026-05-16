//! Stub overlay variant used only in the restart-loop integration test.
//!
//! Reads one NDJSON message from stdin, writes one Pong to stdout (with
//! token if BLUEY_OVERLAY_SESSION_TOKEN is set), then exits non-zero.

use cue_core::overlay_ipc::{decode_ndjson, OverlayEvent, OverlayIpcCommand};
use std::io::{BufRead, BufReader, Write};

fn main() {
    let token = std::env::var("BLUEY_OVERLAY_SESSION_TOKEN").unwrap_or_default();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut out = stdout.lock();
    let mut line = String::new();

    match reader.read_line(&mut line) {
        Ok(0) => std::process::exit(2),
        Ok(_) => {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                std::process::exit(3);
            }
            if decode_ndjson(trimmed).is_err() {
                std::process::exit(4);
            }
            let serialized = if token.is_empty() {
                serde_json::to_string(&OverlayIpcCommand::Pong).unwrap()
            } else {
                serde_json::to_string(&OverlayEvent {
                    token,
                    command: OverlayIpcCommand::Pong,
                })
                .unwrap()
            };
            let _ = writeln!(out, "{serialized}");
            let _ = out.flush();
            std::process::exit(7);
        }
        Err(_) => std::process::exit(5),
    }
}
