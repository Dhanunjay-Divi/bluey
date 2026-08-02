//! Bounded parsing and retention for native audio-helper stderr.
//!
//! Native helpers emit newline-delimited JSON lifecycle diagnostics on stderr
//! while stdout remains reserved for raw PCM. This module is deliberately
//! independent from process spawning so capture runtimes can share the same
//! parser without coupling their lifecycle implementations.

use std::collections::VecDeque;

use cue_core::audio::{AudioSourceKind, AudioStreamFormat};
use serde::{Deserialize, Serialize};

/// A structured lifecycle event emitted by a native audio helper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HelperDiagnosticEvent {
    /// The helper initialized its source and can begin producing PCM.
    #[serde(alias = "stream_start")]
    Ready {
        #[serde(default)]
        source: Option<AudioSourceKind>,
        #[serde(default)]
        format: Option<AudioStreamFormat>,
        #[serde(default)]
        backend: Option<String>,
    },
    /// An OS privacy control denied access to the requested source.
    PermissionDenied {
        #[serde(default)]
        source: Option<AudioSourceKind>,
        #[serde(default)]
        permission: Option<String>,
        #[serde(default)]
        message: String,
    },
    /// The helper encountered a source or processing failure.
    Error {
        #[serde(default)]
        source: Option<AudioSourceKind>,
        #[serde(default)]
        code: Option<String>,
        #[serde(default)]
        message: String,
        #[serde(default)]
        recoverable: bool,
    },
    /// The helper stopped normally or after a controlled shutdown.
    #[serde(alias = "stream_stop")]
    Stopped {
        #[serde(default)]
        source: Option<AudioSourceKind>,
        #[serde(default)]
        reason: Option<String>,
    },
}

/// Memory limits for [`HelperStderrDiagnostics`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelperDiagnosticLimits {
    /// Maximum number of complete stderr lines retained in the diagnostic tail.
    pub max_tail_lines: usize,
    /// Maximum total UTF-8 bytes retained across tail lines.
    pub max_tail_bytes: usize,
    /// Maximum bytes retained while waiting for one unterminated stderr line.
    pub max_pending_line_bytes: usize,
}

impl Default for HelperDiagnosticLimits {
    fn default() -> Self {
        Self {
            max_tail_lines: 64,
            max_tail_bytes: 16 * 1024,
            max_pending_line_bytes: 4 * 1024,
        }
    }
}

/// Incremental newline-delimited stderr parser with a strictly bounded tail.
#[derive(Debug)]
pub struct HelperStderrDiagnostics {
    limits: HelperDiagnosticLimits,
    pending: Vec<u8>,
    pending_was_truncated: bool,
    tail: VecDeque<String>,
    tail_bytes: usize,
    dropped_tail_lines: u64,
    oversized_lines: u64,
    malformed_structured_lines: u64,
}

impl HelperStderrDiagnostics {
    pub fn new(limits: HelperDiagnosticLimits) -> Self {
        Self {
            limits,
            pending: Vec::with_capacity(limits.max_pending_line_bytes.min(1024)),
            pending_was_truncated: false,
            tail: VecDeque::new(),
            tail_bytes: 0,
            dropped_tail_lines: 0,
            oversized_lines: 0,
            malformed_structured_lines: 0,
        }
    }

    /// Consume an arbitrary stderr byte chunk and return all complete events.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<HelperDiagnosticEvent> {
        let mut events = Vec::new();
        for &byte in bytes {
            if byte == b'\n' {
                self.finish_pending_line(&mut events);
                continue;
            }

            if self.pending.len() < self.limits.max_pending_line_bytes {
                self.pending.push(byte);
            } else {
                self.pending_was_truncated = true;
            }
        }
        events
    }

    /// Flush a final unterminated stderr line, normally after helper exit.
    pub fn finish(&mut self) -> Vec<HelperDiagnosticEvent> {
        let mut events = Vec::new();
        if !self.pending.is_empty() || self.pending_was_truncated {
            self.finish_pending_line(&mut events);
        }
        events
    }

    /// Retained stderr lines, oldest first.
    pub fn tail_lines(&self) -> impl Iterator<Item = &str> {
        self.tail.iter().map(String::as_str)
    }

    /// Retained stderr as a newline-separated diagnostic string.
    pub fn tail_text(&self) -> String {
        self.tail_lines().collect::<Vec<_>>().join("\n")
    }

    pub fn dropped_tail_lines(&self) -> u64 {
        self.dropped_tail_lines
    }

    pub fn oversized_lines(&self) -> u64 {
        self.oversized_lines
    }

    pub fn malformed_structured_lines(&self) -> u64 {
        self.malformed_structured_lines
    }

    fn finish_pending_line(&mut self, events: &mut Vec<HelperDiagnosticEvent>) {
        if self.pending.last() == Some(&b'\r') {
            self.pending.pop();
        }

        let mut line = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();

        if self.pending_was_truncated {
            self.pending_was_truncated = false;
            self.oversized_lines = self.oversized_lines.saturating_add(1);
            line.push_str("...[truncated]");
        } else if let Some(event) = parse_helper_diagnostic_line(&line) {
            events.push(event);
        } else if looks_like_structured_event(&line) {
            self.malformed_structured_lines = self.malformed_structured_lines.saturating_add(1);
        }

        self.push_tail_line(line);
    }

    fn push_tail_line(&mut self, mut line: String) {
        if self.limits.max_tail_lines == 0 || self.limits.max_tail_bytes == 0 {
            self.dropped_tail_lines = self.dropped_tail_lines.saturating_add(1);
            return;
        }

        if line.len() > self.limits.max_tail_bytes {
            line = truncate_utf8_with_marker(&line, self.limits.max_tail_bytes);
        }

        self.tail_bytes = self.tail_bytes.saturating_add(line.len());
        self.tail.push_back(line);

        while self.tail.len() > self.limits.max_tail_lines
            || self.tail_bytes > self.limits.max_tail_bytes
        {
            let Some(removed) = self.tail.pop_front() else {
                break;
            };
            self.tail_bytes = self.tail_bytes.saturating_sub(removed.len());
            self.dropped_tail_lines = self.dropped_tail_lines.saturating_add(1);
        }
    }
}

impl Default for HelperStderrDiagnostics {
    fn default() -> Self {
        Self::new(HelperDiagnosticLimits::default())
    }
}

/// Parse one complete helper diagnostic line.
pub fn parse_helper_diagnostic_line(line: &str) -> Option<HelperDiagnosticEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut value = serde_json::from_str::<serde_json::Value>(trimmed).ok()?;
    let object = value.as_object_mut()?;
    if !object.contains_key("event") {
        let event = object
            .get("message_type")
            .cloned()
            .or_else(|| object.get("type").cloned());
        if let Some(event) = event {
            object.insert("event".to_string(), event);
        }
    }

    serde_json::from_value(value).ok()
}

fn looks_like_structured_event(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('{')
        && (trimmed.contains("\"event\"")
            || trimmed.contains("\"message_type\"")
            || trimmed.contains("\"type\""))
}

fn truncate_utf8_with_marker(value: &str, max_bytes: usize) -> String {
    const MARKER: &str = "...[truncated]";
    if value.len() <= max_bytes {
        return value.to_string();
    }
    if max_bytes <= MARKER.len() {
        return MARKER[..max_bytes].to_string();
    }

    let mut end = max_bytes - MARKER.len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = String::with_capacity(max_bytes);
    truncated.push_str(&value[..end]);
    truncated.push_str(MARKER);
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_core::audio::AudioSampleFormat;

    fn compact_limits() -> HelperDiagnosticLimits {
        HelperDiagnosticLimits {
            max_tail_lines: 3,
            max_tail_bytes: 256,
            max_pending_line_bytes: 128,
        }
    }

    #[test]
    fn parses_all_structured_lifecycle_events() {
        let ready = parse_helper_diagnostic_line(
            r#"{"event":"ready","source":"system","format":{"sample_rate_hz":16000,"channel_count":1,"sample_format":"i16"},"backend":"screen_capture_kit"}"#,
        );
        assert_eq!(
            ready,
            Some(HelperDiagnosticEvent::Ready {
                source: Some(AudioSourceKind::System),
                format: Some(AudioStreamFormat::native_helper_pcm16_mono()),
                backend: Some("screen_capture_kit".to_string()),
            })
        );

        assert_eq!(
            parse_helper_diagnostic_line(
                r#"{"event":"permission_denied","source":"microphone","permission":"microphone","message":"access denied"}"#,
            ),
            Some(HelperDiagnosticEvent::PermissionDenied {
                source: Some(AudioSourceKind::Microphone),
                permission: Some("microphone".to_string()),
                message: "access denied".to_string(),
            })
        );
        assert_eq!(
            parse_helper_diagnostic_line(
                r#"{"event":"error","source":"system","code":"stream_lost","message":"capture stopped","recoverable":true}"#,
            ),
            Some(HelperDiagnosticEvent::Error {
                source: Some(AudioSourceKind::System),
                code: Some("stream_lost".to_string()),
                message: "capture stopped".to_string(),
                recoverable: true,
            })
        );
        assert_eq!(
            parse_helper_diagnostic_line(
                r#"{"event":"stopped","source":"system","reason":"requested"}"#,
            ),
            Some(HelperDiagnosticEvent::Stopped {
                source: Some(AudioSourceKind::System),
                reason: Some("requested".to_string()),
            })
        );
    }

    #[test]
    fn accepts_protocol_aliases_and_alternate_discriminator_keys() {
        assert_eq!(
            parse_helper_diagnostic_line(r#"{"message_type":"stream_start","source":"system"}"#),
            Some(HelperDiagnosticEvent::Ready {
                source: Some(AudioSourceKind::System),
                format: None,
                backend: None,
            })
        );
        assert_eq!(
            parse_helper_diagnostic_line(r#"{"type":"stream_stop","reason":"eof"}"#),
            Some(HelperDiagnosticEvent::Stopped {
                source: None,
                reason: Some("eof".to_string()),
            })
        );
    }

    #[test]
    fn incrementally_parses_fragmented_crlf_and_unterminated_lines() {
        let mut diagnostics = HelperStderrDiagnostics::new(compact_limits());
        assert!(diagnostics.push(br#"{"event":"rea"#).is_empty());
        let events = diagnostics
            .push(b"dy\",\"source\":\"system\"}\r\nplain log\r\n{\"event\":\"stopped\"}");
        assert_eq!(
            events,
            vec![HelperDiagnosticEvent::Ready {
                source: Some(AudioSourceKind::System),
                format: None,
                backend: None,
            }]
        );
        assert_eq!(
            diagnostics.finish(),
            vec![HelperDiagnosticEvent::Stopped {
                source: None,
                reason: None,
            }]
        );
        assert_eq!(diagnostics.tail_lines().count(), 3);
    }

    #[test]
    fn tail_is_bounded_by_line_count_and_tracks_evictions() {
        let mut diagnostics = HelperStderrDiagnostics::new(HelperDiagnosticLimits {
            max_tail_lines: 2,
            max_tail_bytes: 100,
            max_pending_line_bytes: 32,
        });
        diagnostics.push(b"one\ntwo\nthree\n");

        assert_eq!(diagnostics.tail_text(), "two\nthree");
        assert_eq!(diagnostics.dropped_tail_lines(), 1);
    }

    #[test]
    fn tail_is_bounded_by_bytes_without_splitting_utf8() {
        let mut diagnostics = HelperStderrDiagnostics::new(HelperDiagnosticLimits {
            max_tail_lines: 4,
            max_tail_bytes: 18,
            max_pending_line_bytes: 64,
        });
        diagnostics.push("🙂🙂🙂🙂🙂\n".as_bytes());

        let tail = diagnostics.tail_text();
        assert!(tail.is_char_boundary(tail.len()));
        assert!(tail.len() <= 18);
        assert!(tail.ends_with("...[truncated]"));
    }

    #[test]
    fn oversized_line_is_discarded_beyond_bound_and_parser_recovers() {
        let mut diagnostics = HelperStderrDiagnostics::new(HelperDiagnosticLimits {
            max_tail_lines: 4,
            max_tail_bytes: 256,
            max_pending_line_bytes: 32,
        });
        let mut input = vec![b'x'; 200];
        input.extend_from_slice(b"\n{\"event\":\"ready\"}\n");

        let events = diagnostics.push(&input);
        assert_eq!(
            events,
            vec![HelperDiagnosticEvent::Ready {
                source: None,
                format: None,
                backend: None,
            }]
        );
        assert_eq!(diagnostics.oversized_lines(), 1);
        assert!(diagnostics.tail_text().contains("...[truncated]"));
    }

    #[test]
    fn malformed_structured_lines_are_retained_and_counted() {
        let mut diagnostics = HelperStderrDiagnostics::new(compact_limits());
        let events = diagnostics.push(b"{\"event\":\"ready\",broken}\nnot json\n");

        assert!(events.is_empty());
        assert_eq!(diagnostics.malformed_structured_lines(), 1);
        assert!(diagnostics.tail_text().contains("broken"));
        assert!(diagnostics.tail_text().contains("not json"));
    }

    #[test]
    fn invalid_utf8_is_lossy_but_never_unbounded_or_fatal() {
        let mut diagnostics = HelperStderrDiagnostics::new(compact_limits());
        assert!(diagnostics.push(&[0xff, 0xfe, b'\n']).is_empty());
        assert!(diagnostics.tail_text().contains('\u{fffd}'));
    }

    #[test]
    fn ready_format_round_trips_through_json() {
        let event = HelperDiagnosticEvent::Ready {
            source: Some(AudioSourceKind::Microphone),
            format: Some(AudioStreamFormat::new(16_000, 1, AudioSampleFormat::I16)),
            backend: Some("core_audio".to_string()),
        };
        let encoded = serde_json::to_string(&event).unwrap();
        assert_eq!(parse_helper_diagnostic_line(&encoded), Some(event));
    }
}
