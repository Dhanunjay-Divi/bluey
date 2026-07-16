//! Mock system audio binary for testing `SystemAudioCapture`.
//!
//! Emits the production v1 helper handshake on stderr, then outputs a 440 Hz
//! sine wave as 16 kHz mono i16 LE PCM on stdout for 2 seconds. This keeps the
//! test double honest as the native helper protocol evolves.

use std::io::Write;

fn main() {
    let backend = if cfg!(target_os = "macos") {
        "screen_capture_kit"
    } else if cfg!(target_os = "windows") {
        "wasapi"
    } else {
        "system_audio_stub"
    };
    eprintln!(
        "{{\"event\":\"ready\",\"protocol_version\":1,\"source\":\"system\",\
         \"backend\":\"{backend}\",\"format\":{{\"sample_rate_hz\":16000,\
         \"channel_count\":1,\"sample_format\":\"i16_le\"}}}}"
    );

    let sample_rate = 16_000u32;
    let frequency = 440.0f64;
    let duration_samples = sample_rate * 2; // 2 seconds
    let amplitude = 16_000i16;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for i in 0..duration_samples {
        let t = i as f64 / sample_rate as f64;
        let sample = (t * frequency * 2.0 * std::f64::consts::PI).sin();
        let value = (sample * amplitude as f64) as i16;
        if out.write_all(&value.to_le_bytes()).is_err() {
            // Broken pipe — reader closed
            break;
        }
    }
    let _ = out.flush();
    eprintln!(
        "{{\"event\":\"stopped\",\"protocol_version\":1,\"source\":\"system\",\
         \"reason\":\"duration_complete\",\"exit_code\":0}}"
    );
}
