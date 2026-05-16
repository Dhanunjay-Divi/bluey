//! Character-based semantic chunker with overlap.
//!
//! Uses character boundaries (approximating tokens at ~4 chars/token) for
//! simplicity. Splits on sentence boundaries when possible.

/// A chunk of text with character offsets into the original.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub text: String,
    pub start_char: usize,
    pub end_char: usize,
}

/// Configurable chunker. Splits text into overlapping chunks.
/// Uses character counts as a proxy for tokens (~4 chars per token).
pub struct Chunker {
    pub max_chars: usize,
    pub overlap_chars: usize,
}

impl Default for Chunker {
    fn default() -> Self {
        Self::new()
    }
}

impl Chunker {
    /// Defaults: max_tokens=200 (~800 chars), overlap_tokens=50 (~200 chars).
    pub fn new() -> Self {
        Self {
            max_chars: 800,
            overlap_chars: 200,
        }
    }

    /// Chunk the input text into overlapping segments.
    pub fn chunk(&self, text: &str) -> Vec<Chunk> {
        let text = text.trim();
        if text.is_empty() {
            return Vec::new();
        }
        if text.len() <= self.max_chars {
            return vec![Chunk {
                text: text.to_string(),
                start_char: 0,
                end_char: text.len(),
            }];
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < text.len() {
            let end = (start + self.max_chars).min(text.len());
            // Try to break at a sentence boundary (. ! ?) within the last 20% of the chunk
            let actual_end = if end < text.len() {
                let search_start = start + (self.max_chars * 4 / 5);
                text[search_start..end]
                    .rfind(['.', '!', '?'])
                    .map(|pos| search_start + pos + 1)
                    .unwrap_or(end)
            } else {
                end
            };

            chunks.push(Chunk {
                text: text[start..actual_end].to_string(),
                start_char: start,
                end_char: actual_end,
            });

            if actual_end >= text.len() {
                break;
            }
            // Advance by (chunk_size - overlap), ensuring forward progress
            let advance = actual_end.saturating_sub(start).saturating_sub(self.overlap_chars);
            start += advance.max(1);
        }

        chunks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunker_basic() {
        let text = "A".repeat(2000);
        let chunker = Chunker::new();
        let chunks = chunker.chunk(&text);
        assert!(chunks.len() >= 3, "expected >=3 chunks, got {}", chunks.len());
        // All chunks should be <= max_chars
        for c in &chunks {
            assert!(c.text.len() <= chunker.max_chars);
        }
        // Verify overlap: each chunk (except first) should start before the previous ended
        for i in 1..chunks.len() {
            assert!(
                chunks[i].start_char < chunks[i - 1].end_char,
                "chunk {} should overlap with chunk {}",
                i,
                i - 1
            );
        }
    }

    #[test]
    fn chunker_short_text() {
        let chunker = Chunker::new();
        let chunks = chunker.chunk("Hello, world!");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "Hello, world!");
        assert_eq!(chunks[0].start_char, 0);
        assert_eq!(chunks[0].end_char, 13);
    }

    #[test]
    fn chunker_empty() {
        let chunker = Chunker::new();
        assert!(chunker.chunk("").is_empty());
        assert!(chunker.chunk("   ").is_empty());
    }

    #[test]
    fn chunker_sentence_boundary() {
        // Create text with sentences that fit nicely
        let sentence = "This is a test sentence. ";
        let text = sentence.repeat(50); // ~1250 chars
        let chunker = Chunker::new();
        let chunks = chunker.chunk(&text);
        assert!(chunks.len() >= 2);
        // First chunk should end at a sentence boundary (period)
        assert!(
            chunks[0].text.ends_with(".") || chunks[0].text.ends_with(". "),
            "chunk should end at sentence boundary: {:?}",
            &chunks[0].text[chunks[0].text.len().saturating_sub(5)..]
        );
    }
}
