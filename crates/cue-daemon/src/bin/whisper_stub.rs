//! Mock whisper helper binary for testing `LocalWhisperProvider`.
//!
//! Reads PCM16 LE 16kHz mono from stdin. After receiving any data,
//! emits a known sequence of NDJSON events and exits.
//! This mirrors the real helper's protocol without requiring whisper.cpp.

use std::io::{self, Read, Write};

fn main() {
    let mut buf = [0u8; 640]; // 320 samples * 2 bytes = one 20ms frame
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // Read at least one chunk to confirm IPC works
    if stdin.lock().read(&mut buf).unwrap_or(0) > 0 {
        // Emit the test sequence
        let _ = writeln!(out, r#"{{"type":"partial","text":"hello"}}"#);
        let _ = writeln!(out, r#"{{"type":"partial","text":"hello world"}}"#);
        let _ = writeln!(
            out,
            r#"{{"type":"final","text":"hello world","confidence":0.95}}"#
        );
        let _ = out.flush();
    }

    // Drain remaining stdin to avoid broken pipe on sender side
    let _ = io::copy(&mut stdin.lock(), &mut io::sink());
}
