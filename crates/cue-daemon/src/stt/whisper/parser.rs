//! NDJSON parser for whisper helper output.

use cue_core::pcm::AudioSource;
use cue_core::stt::TranscriptEvent;
use serde::Deserialize;

/// Raw NDJSON event from the whisper helper binary.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WhisperEvent {
    Partial {
        text: String,
    },
    Final {
        text: String,
        confidence: Option<f64>,
    },
}

impl WhisperEvent {
    /// Convert to a core TranscriptEvent with the given audio source.
    pub fn into_transcript_event(self, source: AudioSource) -> TranscriptEvent {
        match self {
            WhisperEvent::Partial { text } => TranscriptEvent::Partial {
                text,
                confidence: None,
                source,
            },
            WhisperEvent::Final { text, confidence } => TranscriptEvent::Final {
                text,
                confidence: confidence.map(|c| c as f32),
                source,
                words: Vec::new(),
            },
        }
    }
}

/// Parse a single NDJSON line into a WhisperEvent.
pub fn parse_line(line: &str) -> Result<WhisperEvent, serde_json::Error> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_partial_event() {
        let line = r#"{"type":"partial","text":"hello"}"#;
        let evt = parse_line(line).unwrap();
        assert!(matches!(evt, WhisperEvent::Partial { ref text } if text == "hello"));
    }

    #[test]
    fn parse_final_event_with_confidence() {
        let line = r#"{"type":"final","text":"hello world","confidence":0.91}"#;
        let evt = parse_line(line).unwrap();
        match evt {
            WhisperEvent::Final {
                ref text,
                confidence,
            } => {
                assert_eq!(text, "hello world");
                assert_eq!(confidence, Some(0.91));
            }
            _ => panic!("expected Final"),
        }
    }

    #[test]
    fn parse_final_event_without_confidence() {
        let line = r#"{"type":"final","text":"test"}"#;
        let evt = parse_line(line).unwrap();
        assert!(matches!(
            evt,
            WhisperEvent::Final {
                confidence: None,
                ..
            }
        ));
    }

    #[test]
    fn into_transcript_event_partial() {
        let evt = WhisperEvent::Partial { text: "hi".into() };
        let te = evt.into_transcript_event(AudioSource::Microphone);
        assert!(matches!(te, TranscriptEvent::Partial { ref text, .. } if text == "hi"));
    }

    #[test]
    fn into_transcript_event_final() {
        let evt = WhisperEvent::Final {
            text: "done".into(),
            confidence: Some(0.85),
        };
        let te = evt.into_transcript_event(AudioSource::System);
        match te {
            TranscriptEvent::Final {
                text,
                confidence,
                source,
                ..
            } => {
                assert_eq!(text, "done");
                assert_eq!(confidence, Some(0.85f32));
                assert_eq!(source, AudioSource::System);
            }
            _ => panic!("expected Final"),
        }
    }
}
