//! Stub overlay process used in integration tests ONLY.
//!
//! Reads NDJSON [`OverlayMessage`]s from stdin (one per line). For each
//! decoded message it writes TWO NDJSON [`OverlayIpcCommand`]s to stdout:
//!
//! 1. `OverlayIpcCommand::Pong` — a plain ack.
//! 2. `OverlayIpcCommand::Echo { payload }` — carries the JSON form of the
//!    decoded message. Tests use this to assert the full payload arrived
//!    intact (not just that the enum variant was decodable).
//!
//! Exits cleanly on stdin EOF, so the daemon's `shutdown()` (which closes
//! the child's stdin) causes the stub to terminate.
//!
//! This binary is the testing counterpart to the Swift/C overlay that
//! ships with the real app. It has NO other purpose and MUST NOT be used
//! as a runtime overlay — shipping it would break real overlay behavior.

use cue_core::overlay_ipc::{decode_ndjson, OverlayIpcCommand};
use std::io::{BufRead, BufReader, Write};

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut out = stdout.lock();
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF — parent closed stdin
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match decode_ndjson(trimmed) {
                    Ok(msg) => {
                        // 1. Ack
                        if write_ipc_command(&mut out, &OverlayIpcCommand::Pong).is_err() {
                            break;
                        }
                        // 2. Echo the decoded message back as JSON so tests
                        //    can verify exact payload fidelity.
                        let payload = serde_json::to_string(&msg).unwrap_or_default();
                        let echo = OverlayIpcCommand::Echo { payload };
                        if write_ipc_command(&mut out, &echo).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        // Malformed: swallow silently, do not crash the stub.
                    }
                }
            }
            Err(_) => break,
        }
    }
}

fn write_ipc_command(out: &mut impl Write, cmd: &OverlayIpcCommand) -> std::io::Result<()> {
    let serialized = serde_json::to_string(cmd)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    writeln!(out, "{serialized}")?;
    out.flush()
}
