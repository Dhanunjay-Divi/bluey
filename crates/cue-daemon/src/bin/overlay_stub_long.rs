//! Long-running overlay stub for integration tests.
//!
//! Reads NDJSON messages from stdin in a loop, writes Pong + Echo for each.
//! Includes session token in responses if BLUEY_OVERLAY_SESSION_TOKEN is set.
//! Exits cleanly (code 0) only on stdin EOF.

use cue_core::overlay_ipc::{decode_ndjson, OverlayEvent, OverlayIpcCommand};
use std::io::{BufRead, BufReader, Write};

fn main() {
    let token = std::env::var("BLUEY_OVERLAY_SESSION_TOKEN").unwrap_or_default();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut out = stdout.lock();
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(msg) = decode_ndjson(trimmed) {
                    let _ = write_cmd(&mut out, &OverlayIpcCommand::Pong, &token);
                    let payload = serde_json::to_string(&msg).unwrap_or_default();
                    let _ = write_cmd(&mut out, &OverlayIpcCommand::Echo { payload }, &token);
                }
            }
            Err(_) => break,
        }
    }
}

fn write_cmd(out: &mut impl Write, cmd: &OverlayIpcCommand, token: &str) -> std::io::Result<()> {
    let serialized = if token.is_empty() {
        serde_json::to_string(cmd)
    } else {
        serde_json::to_string(&OverlayEvent {
            token: token.to_string(),
            command: cmd.clone(),
        })
    }
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    writeln!(out, "{serialized}")?;
    out.flush()
}
