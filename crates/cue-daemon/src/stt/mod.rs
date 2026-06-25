//! Concrete STT provider implementations that sit behind the cue-core SttProvider trait.

pub mod deepgram;
pub mod echo;
pub mod factory;
pub mod mock;
pub mod openai;
#[cfg(feature = "parakeet-stt")]
pub mod parakeet;
pub mod router;
pub mod whisper;
