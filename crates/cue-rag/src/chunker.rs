//! Character-based semantic chunker with overlap.
//!
//! Uses character boundaries (approximating tokens at ~4 chars/token) for
//! simplicity. Splits on sentence boundaries when possible.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

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
            // Floor every computed byte offset to a UTF-8 char boundary: raw
            // `start + max_chars` arithmetic lands mid-codepoint on multibyte
            // text and panics on slice. For pure-ASCII input every offset is
            // already a boundary, so flooring is a no-op and the chunking is
            // byte-for-byte identical to before.
            let mut end = floor_char_boundary(text, (start + self.max_chars).min(text.len()));
            if end <= start {
                // Degenerate config: a single char wider than max_chars.
                // Take one whole char so the loop always makes progress.
                end = ceil_char_boundary(text, start + 1);
            }
            // Try to break at a sentence boundary (. ! ?) within the last 20% of the chunk
            let actual_end = if end < text.len() {
                let search_start =
                    floor_char_boundary(text, start + (self.max_chars * 4 / 5)).min(end);
                text[search_start..end]
                    .rfind(['.', '!', '?'])
                    // `. ! ?` are single-byte ASCII, so +1 stays on a boundary.
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
            let advance = actual_end
                .saturating_sub(start)
                .saturating_sub(self.overlap_chars);
            let next_start = floor_char_boundary(text, start + advance.max(1));
            start = if next_start > start {
                next_start
            } else {
                // Flooring landed back on `start`: step to the next boundary.
                ceil_char_boundary(text, start + 1)
            };
        }

        chunks
    }
}

/// Largest byte index `<= idx` that lies on a char boundary of `text`.
fn floor_char_boundary(text: &str, idx: usize) -> usize {
    if idx >= text.len() {
        return text.len();
    }
    let mut i = idx;
    while !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Smallest byte index `>= idx` that lies on a char boundary of `text`.
fn ceil_char_boundary(text: &str, idx: usize) -> usize {
    if idx >= text.len() {
        return text.len();
    }
    let mut i = idx;
    while !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Drop chunks whose trimmed text exactly matches an earlier chunk's trimmed
/// text. First occurrence wins; the relative order of survivors is preserved.
///
/// Session-continuation forks re-copy transcripts wholesale: on real agent
/// session history 46.5% of chunks were exact duplicates, and deduplicating at
/// index time raised strict recall@1 from 46% to 78% in the retrieval harness.
pub fn hash_dedup(chunks: Vec<Chunk>) -> Vec<Chunk> {
    let mut seen: HashSet<u64> = HashSet::with_capacity(chunks.len());
    let mut out = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let mut hasher = DefaultHasher::new();
        chunk.text.trim().hash(&mut hasher);
        if seen.insert(hasher.finish()) {
            out.push(chunk);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunker_basic() {
        let text = "A".repeat(2000);
        let chunker = Chunker::new();
        let chunks = chunker.chunk(&text);
        assert!(
            chunks.len() >= 3,
            "expected >=3 chunks, got {}",
            chunks.len()
        );
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

    /// Shared invariants for multibyte regression tests: offsets land on char
    /// boundaries, chunk text matches its offsets, budget respected, full
    /// coverage, and consecutive chunks overlap.
    fn assert_chunks_well_formed(text: &str, chunks: &[Chunk], max_chars: usize) {
        assert!(!chunks.is_empty(), "expected at least one chunk");
        for (i, c) in chunks.iter().enumerate() {
            assert!(
                text.is_char_boundary(c.start_char),
                "chunk {i} start_char {} not on a char boundary",
                c.start_char
            );
            assert!(
                text.is_char_boundary(c.end_char),
                "chunk {i} end_char {} not on a char boundary",
                c.end_char
            );
            assert_eq!(
                &text[c.start_char..c.end_char],
                c.text,
                "chunk {i} text does not match its offsets"
            );
            assert!(
                c.text.len() <= max_chars,
                "chunk {i} exceeds max_chars: {} > {max_chars}",
                c.text.len()
            );
        }
        assert_eq!(chunks[0].start_char, 0, "first chunk must start at 0");
        assert_eq!(
            chunks[chunks.len() - 1].end_char,
            text.len(),
            "last chunk must reach end of text"
        );
        for i in 1..chunks.len() {
            assert!(
                chunks[i].start_char < chunks[i - 1].end_char,
                "chunk {i} should overlap with chunk {}",
                i - 1
            );
        }
    }

    #[test]
    fn chunker_splits_long_cjk_text_on_char_boundaries() {
        // 3 bytes per CJK char; 600 chars = 1800 bytes, well over max_chars.
        // Old byte-offset slicing panicked mid-codepoint on input like this.
        let text = "会議の要点を記録します".repeat(60); // 660 chars = 1980 bytes
        let chunker = Chunker::new();
        let chunks = chunker.chunk(&text);
        assert!(chunks.len() >= 2, "expected multiple chunks");
        assert_chunks_well_formed(text.trim(), &chunks, chunker.max_chars);
    }

    #[test]
    fn chunker_splits_emoji_dense_text_on_char_boundaries() {
        // 4 bytes per emoji. The 3-byte ASCII prefix shifts every emoji off
        // 4-byte alignment, so the raw offsets at 640 (sentence-search start)
        // and 800 (max_chars cut) both land mid-emoji — the old byte-offset
        // slicing panicked on exactly this shape.
        let text = format!("ok:{}", "😀😃😄😁😆".repeat(60)); // 3 + 1200 bytes
        let chunker = Chunker::new();
        let chunks = chunker.chunk(&text);
        assert!(chunks.len() >= 2, "expected multiple chunks");
        assert_chunks_well_formed(&text, &chunks, chunker.max_chars);
    }

    #[test]
    fn chunker_floors_boundary_landing_inside_accented_char() {
        // 799 ASCII bytes, then two-byte "é"s: the raw cut at byte 800 lands
        // in the middle of the first "é" (bytes 799..801). The old code
        // panicked here; the fix must floor the cut to byte 799.
        let text = format!("{}{}", "a".repeat(799), "é".repeat(200)); // 1199 bytes
        let chunker = Chunker::new();
        let chunks = chunker.chunk(&text);
        assert!(chunks.len() >= 2, "expected multiple chunks");
        assert_eq!(
            chunks[0].end_char, 799,
            "first cut must floor from mid-é byte 800 to boundary 799"
        );
        assert_chunks_well_formed(&text, &chunks, chunker.max_chars);
    }

    #[test]
    fn chunker_handles_mixed_ascii_and_multibyte_text() {
        let sentence = "The naïve café demo covered 日本語 output and emoji 😀 rendering. ";
        let text = sentence.repeat(40); // ~2900 bytes, multibyte sprinkled throughout
        let text = text.trim();
        let chunker = Chunker::new();
        let chunks = chunker.chunk(text);
        assert!(chunks.len() >= 3, "expected several chunks");
        assert_chunks_well_formed(text, &chunks, chunker.max_chars);
        // Sentence-boundary preference still applies where a period is in range.
        assert!(
            chunks[0].text.ends_with('.') || chunks[0].text.ends_with(". "),
            "first chunk should end at a sentence boundary"
        );
    }

    #[test]
    fn chunker_ascii_output_unchanged_by_boundary_flooring() {
        // ASCII offsets are always char boundaries, so the fixed chunker must
        // produce byte-identical output to the original algorithm.
        let sentence = "This is a test sentence. ";
        let text = sentence.repeat(50);
        let text = text.trim();
        let chunker = Chunker::new();
        let chunks = chunker.chunk(text);
        // Reproduce the original (pre-fix) algorithm inline and compare.
        let mut expected = Vec::new();
        let mut start = 0;
        while start < text.len() {
            let end = (start + chunker.max_chars).min(text.len());
            let actual_end = if end < text.len() {
                let search_start = start + (chunker.max_chars * 4 / 5);
                text[search_start..end]
                    .rfind(['.', '!', '?'])
                    .map(|pos| search_start + pos + 1)
                    .unwrap_or(end)
            } else {
                end
            };
            expected.push((start, actual_end));
            if actual_end >= text.len() {
                break;
            }
            let advance = actual_end
                .saturating_sub(start)
                .saturating_sub(chunker.overlap_chars);
            start += advance.max(1);
        }
        let got: Vec<(usize, usize)> = chunks.iter().map(|c| (c.start_char, c.end_char)).collect();
        assert_eq!(got, expected, "ASCII chunk boundaries must be unchanged");
    }

    fn chunk_at(text: &str, start: usize) -> Chunk {
        Chunk {
            text: text.to_string(),
            start_char: start,
            end_char: start + text.len(),
        }
    }

    #[test]
    fn hash_dedup_drops_exact_duplicate_texts() {
        let chunks = vec![
            chunk_at("alpha", 0),
            chunk_at("beta", 10),
            chunk_at("alpha", 20),
        ];
        let out = hash_dedup(chunks);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "alpha");
        assert_eq!(out[1].text, "beta");
    }

    #[test]
    fn hash_dedup_keeps_first_occurrence() {
        let chunks = vec![chunk_at("same", 0), chunk_at("same", 100)];
        let out = hash_dedup(chunks);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].start_char, 0, "the FIRST occurrence must survive");
    }

    #[test]
    fn hash_dedup_preserves_order_of_survivors() {
        let chunks = vec![
            chunk_at("one", 0),
            chunk_at("two", 10),
            chunk_at("one", 20),
            chunk_at("three", 30),
            chunk_at("two", 40),
        ];
        let out = hash_dedup(chunks);
        let texts: Vec<&str> = out.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, vec!["one", "two", "three"]);
    }

    #[test]
    fn hash_dedup_trims_before_comparing() {
        let chunks = vec![chunk_at("  padded  ", 0), chunk_at("padded", 50)];
        let out = hash_dedup(chunks);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].text, "  padded  ",
            "first occurrence wins even when only whitespace differs"
        );
    }

    #[test]
    fn hash_dedup_empty_input_returns_empty() {
        assert!(hash_dedup(Vec::new()).is_empty());
    }
}
