#![allow(clippy::single_match)]
//! Stub overlay process used in integration tests ONLY.
//!
//! Reads NDJSON [`OverlayMessage`]s from stdin (one per line). For each
//! decoded message it writes TWO NDJSON responses to stdout:
//!
//! 1. `Pong` — a plain ack.
//! 2. `Echo { payload }` — carries the JSON form of the decoded message.
//!
//! If BLUEY_OVERLAY_SESSION_TOKEN is set, responses are wrapped in an
//! `OverlayEvent` envelope with the token included.
//!
//! Exits cleanly on stdin EOF.

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
                match decode_ndjson(trimmed) {
                    Ok(msg) => {
                        if write_cmd(&mut out, &OverlayIpcCommand::Pong, &token).is_err() {
                            break;
                        }
                        let payload = serde_json::to_string(&msg).unwrap_or_default();
                        let echo = OverlayIpcCommand::Echo { payload };
                        if write_cmd(&mut out, &echo, &token).is_err() {
                            break;
                        }
                    }
                    Err(_) => {}
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
