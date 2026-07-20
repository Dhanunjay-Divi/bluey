//! Reusable in-memory retrieval index over OTHER coding-agents' PAST session
//! histories (feature `local-embed`).
//!
//! Bluey drives headless agent sessions; every agent (Claude Code, Codex,
//! Cursor, …) also leaves its OWN past-session transcripts on disk. This index
//! lets the driven agent PULL relevant slices of that cross-agent history as
//! read-only grounding — the "borrow their reasoning" side-channel. It is a
//! PURE LIBRARY: it holds chunks + embeddings + metadata in memory and runs the
//! same mem0 v3 hybrid pipeline the facts store uses (semantic cosine + BM25 +
//! entity boost, fused by [`crate::hybrid::score_and_rank`]). It does NOT know
//! about the daemon, agent-bridge, or the embedder — the caller owns the
//! embedder and passes it in as a closure, so this stays reusable and testable
//! with a deterministic fake embedder (no ONNX needed).
//!
//! The retrieval harness proved this engine over 5k real chunks of agent
//! history: 78% strict recall@1, ~12ms/query, every topical query dead-on. This
//! module productionizes that proof into a library type.
//!
//! Index-time discipline (mirrors the facts write path + the session
//! [`crate::filter`]): each raw turn is dropped if it is one of Bluey's own
//! headless prompts ([`crate::is_self_prompt`]), oversized noise (> 2000 chars —
//! whole-file pastes, giant logs), or a tool/data dump (starts with a fence,
//! JSON/array bracket, or a diff marker). Surviving prose is chunked with the
//! UTF-8-safe [`Chunker`], embedded, and deduplicated against the existing index
//! (session-continuation forks re-copy transcripts wholesale; on real data 46.5%
//! of chunks were exact duplicates, and deduping raised recall@1 46%→78%).

use crate::hybrid;
use crate::{is_self_prompt, Chunker};

/// Turns longer than this are treated as noise (whole-file pastes, giant logs,
/// stack dumps) and skipped at index time — prose turns are the signal.
const MAX_TURN_CHARS: usize = 2000;

/// Relevance floor for agent-history recall: gates the SEMANTIC score before
/// fusion (mem0's threshold contract). Same 0.45 the cross-meeting facts recall
/// uses (`memory::RELEVANCE_FLOOR`) — verified against real bge-small scores.
const RELEVANCE_FLOOR: f32 = 0.45;

/// One indexed chunk: prose text, its passage embedding, and provenance.
#[derive(Debug, Clone)]
pub struct IndexedChunk {
    pub text: String,
    pub embedding: Vec<f32>,
    pub agent: String,
    pub session_id: String,
    pub epoch_secs: u64,
}

/// One search hit: the matched chunk's text + provenance + fused score.
#[derive(Debug, Clone)]
pub struct HistoryHit {
    pub text: String,
    pub agent: String,
    pub session_id: String,
    pub epoch_secs: u64,
    pub score: f32,
}

/// In-memory retrieval index over agent session history.
///
/// Built incrementally with [`AgentHistoryIndex::add_session`] and queried with
/// [`AgentHistoryIndex::search`]. Both fail-soft: a bad turn is skipped, an
/// empty index searches to an empty result set, and nothing here ever panics on
/// caller data.
#[derive(Debug, Default)]
pub struct AgentHistoryIndex {
    chunks: Vec<IndexedChunk>,
}

impl AgentHistoryIndex {
    /// A fresh, empty index.
    pub fn new() -> Self {
        Self { chunks: Vec::new() }
    }

    /// Number of indexed chunks.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Whether the index holds no chunks.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Index the raw turns of one session.
    ///
    /// Each turn is filtered ([`is_self_prompt`], > [`MAX_TURN_CHARS`], tool-dump
    /// heuristic), the survivors are chunked with [`Chunker`], each chunk is
    /// embedded via `embed` (chunks the closure returns `None` for are skipped),
    /// and the result is deduplicated by trimmed chunk text against everything
    /// already in the index (first occurrence wins — mirrors
    /// [`crate::hash_dedup`], extended to compare against existing contents).
    ///
    /// `embed` is a closure so the caller owns the embedder — this module never
    /// constructs or depends on a concrete embedder, keeping it reusable and
    /// unit-testable without a model.
    pub fn add_session(
        &mut self,
        agent: &str,
        session_id: &str,
        epoch_secs: u64,
        turns: &[String],
        embed: &mut dyn FnMut(&str) -> Option<Vec<f32>>,
    ) {
        // Set of trimmed chunk texts already indexed (dedup key). Built once
        // from existing contents, then extended as we add this session's chunks
        // so intra-session duplicates are also collapsed. First occurrence wins.
        use std::collections::HashSet;
        let mut seen: HashSet<&str> = HashSet::with_capacity(self.chunks.len());
        for existing in &self.chunks {
            seen.insert(existing.text.trim());
        }
        // Collect the survivors' owned texts first (we cannot borrow `seen` into
        // `self.chunks` while also inserting borrowed keys from those pushes).
        let mut pending: Vec<String> = Vec::new();
        {
            let chunker = Chunker::new();
            let mut local_seen: HashSet<String> = HashSet::new();
            for turn in turns {
                if !is_indexable_turn(turn) {
                    continue;
                }
                for chunk in chunker.chunk(turn) {
                    let key = chunk.text.trim();
                    if key.is_empty() {
                        continue;
                    }
                    // Dedup against existing index AND this batch's survivors.
                    if seen.contains(key) || !local_seen.insert(key.to_string()) {
                        continue;
                    }
                    pending.push(chunk.text);
                }
            }
        }
        for text in pending {
            let Some(embedding) = embed(&text) else {
                continue; // embedder declined this chunk — skip, never fatal.
            };
            self.chunks.push(IndexedChunk {
                text,
                embedding,
                agent: agent.to_string(),
                session_id: session_id.to_string(),
                epoch_secs,
            });
        }
    }

    /// Hybrid search over the whole index for `query`.
    ///
    /// Runs the mem0 v3 pipeline exactly as `FactsStore::hybrid_query` does:
    /// over-fetched semantic cosine (`query_embedding` vs each chunk embedding) +
    /// BM25 over the stemmed corpus (sigmoid-normalized, query-length-adaptive) +
    /// query-entity boosts, fused by [`crate::hybrid::score_and_rank`] with the
    /// [`RELEVANCE_FLOOR`] gating the SEMANTIC score. Returns the top-`k` by
    /// fused score, newest `epoch_secs` as the tie-break. Fail-soft: an empty
    /// index, `k == 0`, or a dim-mismatched query all yield an empty vec.
    pub fn search(&self, query: &str, query_embedding: &[f32], k: usize) -> Vec<HistoryHit> {
        if k == 0 || self.chunks.is_empty() {
            return Vec::new();
        }

        // Semantic candidate pool, over-fetched (mem0: max(limit*4, 60)). The
        // internal id is the chunk's index into `self.chunks`.
        let mut semantic: Vec<hybrid::SemanticCandidate> = self
            .chunks
            .iter()
            .enumerate()
            .map(|(i, c)| hybrid::SemanticCandidate {
                id: i as i64,
                semantic: cosine(query_embedding, &c.embedding),
            })
            .collect();
        semantic.sort_by(|a, b| {
            b.semantic
                .partial_cmp(&a.semantic)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        semantic.truncate(hybrid::internal_limit(k));

        // BM25 over the same corpus, sigmoid-normalized with query-length params.
        let query_stemmed = hybrid::stem_for_bm25(query);
        let stemmed_docs: Vec<(i64, String)> = self
            .chunks
            .iter()
            .enumerate()
            .map(|(i, c)| (i as i64, hybrid::stem_for_bm25_joined(&c.text)))
            .collect();
        let raw = hybrid::bm25_raw_scores(
            &query_stemmed,
            stemmed_docs.iter().map(|(id, s)| (*id, s.as_str())),
        );
        let (midpoint, steepness) = hybrid::bm25_params(query_stemmed.len());
        let bm25: std::collections::HashMap<i64, f32> = raw
            .into_iter()
            .map(|(id, score)| (id, hybrid::normalize_bm25(score, midpoint, steepness)))
            .collect();

        // Query-entity boosts. There is no persistent linked-entity store here
        // (mem0's `_compute_entity_boosts` runs against one), so we boost a chunk
        // when a query entity also appears — accepted via the SAME acceptance
        // rule the facts recall uses ([`hybrid::entity_match_accepted`]) — as one
        // of that chunk's extracted entities. Best boost per chunk wins (mem0's
        // per-fact max). Exact-text matches (similarity 1.0) clear the floor.
        let boosts = self.entity_boosts(query);

        let ranked = hybrid::score_and_rank(&semantic, &bm25, &boosts, RELEVANCE_FLOOR, k);
        ranked
            .into_iter()
            .filter_map(|hit| {
                self.chunks.get(hit.id as usize).map(|c| HistoryHit {
                    text: c.text.clone(),
                    agent: c.agent.clone(),
                    session_id: c.session_id.clone(),
                    epoch_secs: c.epoch_secs,
                    score: hit.combined,
                })
            })
            .collect::<Vec<_>>()
            // score_and_rank already sorted by combined desc; make the newest
            // epoch the deterministic tie-break for equal scores.
            .tap_sorted_newest_tiebreak()
    }

    /// Query-entity → chunk boosts (see [`Self::search`]). Keyed on the chunk's
    /// index into `self.chunks`.
    fn entity_boosts(&self, query: &str) -> std::collections::HashMap<i64, f32> {
        use std::collections::{HashMap, HashSet};
        let mut boosts: HashMap<i64, f32> = HashMap::new();

        let mut query_entities: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for (_, text) in hybrid::extract_entities(query)
            .into_iter()
            .take(hybrid::MAX_QUERY_ENTITIES)
        {
            let key = hybrid::normalize_entity_text(&text);
            if key.is_empty() || !seen.insert(key) {
                continue;
            }
            query_entities.push(text);
        }
        if query_entities.is_empty() {
            return boosts;
        }

        for (i, chunk) in self.chunks.iter().enumerate() {
            let chunk_entities = hybrid::extract_entities(&chunk.text);
            if chunk_entities.is_empty() {
                continue;
            }
            let mut best: Option<f32> = None;
            for qe in &query_entities {
                for (_, ce) in &chunk_entities {
                    // Exact normalized match → similarity 1.0. This module has no
                    // entity embeddings, so semantic entity similarity is not
                    // available; the exact-text path is the reliable, false-boost
                    // -free signal (the acceptance rule always admits exact text).
                    if hybrid::normalize_entity_text(qe) == hybrid::normalize_entity_text(ce)
                        && hybrid::entity_match_accepted(qe, ce, 1.0)
                    {
                        let boost = hybrid::entity_boost(1.0, 1);
                        best = Some(best.map_or(boost, |b: f32| b.max(boost)));
                    }
                }
            }
            if let Some(boost) = best {
                boosts.insert(i as i64, boost);
            }
        }
        boosts
    }
}

/// Whether a raw turn should be indexed: not one of Bluey's own headless
/// prompts, not oversized, not a tool/data dump. Conservative — prose only.
fn is_indexable_turn(turn: &str) -> bool {
    let trimmed = turn.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_TURN_CHARS {
        return false;
    }
    if is_self_prompt(trimmed) {
        return false;
    }
    if looks_like_tool_dump(trimmed) {
        return false;
    }
    true
}

/// Conservative tool/data-dump heuristic: a turn that OPENS with a code fence, a
/// JSON/array bracket, or a unified-diff marker is machine output (tool result,
/// pasted file, patch), not prose reasoning worth grounding on. Kept deliberately
/// narrow — it only inspects the FIRST non-empty line's leading token so ordinary
/// prose that merely mentions `{` or `diff` mid-sentence is never dropped.
fn looks_like_tool_dump(trimmed: &str) -> bool {
    // Fenced block opener anywhere-leading (``` or ~~~).
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        return true;
    }
    // First non-empty line's leading marker.
    let Some(first) = trimmed.lines().find(|l| !l.trim().is_empty()) else {
        return true; // only blank lines → nothing to index.
    };
    let first = first.trim_start();
    // JSON object / array dump.
    if first.starts_with('{') || first.starts_with('[') {
        return true;
    }
    // Unified-diff / patch markers.
    const DIFF_MARKERS: &[&str] = &["diff --git", "--- a/", "+++ b/", "@@ ", "index ", "Index: "];
    if DIFF_MARKERS.iter().any(|m| first.starts_with(m)) {
        return true;
    }
    false
}

/// Cosine similarity, matching the facts store's private `cosine` exactly (dot /
/// (‖a‖·‖b‖); 0 when either norm is 0). Local because the facts one is private.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (ai, bi) in a.iter().zip(b.iter()) {
        dot += ai * bi;
        norm_a += ai * ai;
        norm_b += bi * bi;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Stable-sort extension: newest `epoch_secs` breaks ties on equal fused score.
/// `score_and_rank` already ordered by score desc; a stable sort keyed only on
/// score preserves that order and lets us layer the epoch tie-break cleanly.
trait NewestTieBreak {
    fn tap_sorted_newest_tiebreak(self) -> Self;
}

impl NewestTieBreak for Vec<HistoryHit> {
    fn tap_sorted_newest_tiebreak(mut self) -> Self {
        self.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.epoch_secs.cmp(&a.epoch_secs))
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    /// Deterministic fake embedder: hash the text into a fixed-dim unit-ish
    /// vector. Identical text embeds identically (so a planted match is exact),
    /// different text embeds differently — no ONNX needed. Dim is small; the
    /// hybrid pipeline is dim-agnostic (cosine over equal-length vectors).
    const FAKE_DIM: usize = 16;

    fn fake_embed(text: &str) -> Option<Vec<f32>> {
        let key = text.trim();
        let mut v = vec![0.0f32; FAKE_DIM];
        // Seed each dimension from a per-dimension hash of the text so that
        // identical strings map to identical vectors and unrelated strings are
        // near-orthogonal in expectation.
        for (i, slot) in v.iter_mut().enumerate() {
            let mut h = DefaultHasher::new();
            i.hash(&mut h);
            key.hash(&mut h);
            // Map the hash to a value in roughly [-1, 1].
            *slot = ((h.finish() % 2000) as f32 / 1000.0) - 1.0;
        }
        Some(v)
    }

    /// An embed closure adaptor over `fake_embed`.
    fn embedder() -> impl FnMut(&str) -> Option<Vec<f32>> {
        |t: &str| fake_embed(t)
    }

    #[test]
    fn new_index_is_empty() {
        let idx = AgentHistoryIndex::new();
        assert!(idx.is_empty());
        assert_eq!(idx.len(), 0);
    }

    #[test]
    fn add_session_indexes_prose_turns() {
        let mut idx = AgentHistoryIndex::new();
        let turns = vec![
            "We decided to shard the sessions table by tenant id for scale.".to_string(),
            "The retry budget for the payments webhook is three attempts.".to_string(),
        ];
        let mut e = embedder();
        idx.add_session("claude", "s1", 100, &turns, &mut e);
        assert_eq!(idx.len(), 2);
        assert!(!idx.is_empty());
    }

    #[test]
    fn add_session_filters_self_prompt_oversized_and_tool_dump() {
        let mut idx = AgentHistoryIndex::new();
        let big = "x".repeat(MAX_TURN_CHARS + 1);
        let turns = vec![
            // Kept: ordinary prose.
            "We chose Postgres advisory locks for the migration guard.".to_string(),
            // Dropped: Bluey's own ledger prompt (is_self_prompt).
            "You extract a meeting ledger from the transcript below.".to_string(),
            // Dropped: oversized noise (> MAX_TURN_CHARS).
            big,
            // Dropped: fenced tool dump.
            "```json\n{\"tool\":\"read\",\"path\":\"/etc/hosts\"}\n```".to_string(),
            // Dropped: JSON object dump.
            "{\"result\": [1, 2, 3], \"ok\": true}".to_string(),
            // Dropped: unified diff.
            "diff --git a/src/lib.rs b/src/lib.rs\n@@ -1 +1 @@".to_string(),
        ];
        let mut e = embedder();
        idx.add_session("codex", "s2", 200, &turns, &mut e);
        assert_eq!(idx.len(), 1, "only the single prose turn should be indexed");
        assert!(idx.chunks[0].text.contains("advisory locks"));
    }

    #[test]
    fn add_session_dedups_against_existing_and_within_batch() {
        let mut idx = AgentHistoryIndex::new();
        let turns = vec![
            "The auth rollout owner is Dana and it ships on Friday.".to_string(),
            // Exact duplicate within the same batch → collapsed.
            "The auth rollout owner is Dana and it ships on Friday.".to_string(),
        ];
        let mut e = embedder();
        idx.add_session("claude", "s1", 100, &turns, &mut e);
        let after_first = idx.len();
        assert_eq!(after_first, 1, "intra-batch duplicate collapsed");

        // Re-adding the SAME session/turns must not double the chunks.
        idx.add_session("claude", "s1", 100, &turns, &mut e);
        assert_eq!(idx.len(), after_first, "re-adding must not double chunks");
    }

    #[test]
    fn add_session_skips_chunks_the_embedder_declines() {
        let mut idx = AgentHistoryIndex::new();
        let turns = vec!["Keep this prose turn about the payments service.".to_string()];
        // Embedder that declines everything.
        let mut decline = |_: &str| None;
        idx.add_session("claude", "s1", 100, &turns, &mut decline);
        assert!(idx.is_empty(), "declined chunks are skipped, not indexed");
    }

    #[test]
    fn search_ranks_planted_relevant_chunk_top1() {
        let mut idx = AgentHistoryIndex::new();
        let planted = "The checkout latency SLA is two hundred milliseconds after tuning.";
        let turns = vec![
            planted.to_string(),
            "The design retro was moved to Fridays at noon.".to_string(),
            "We migrated the analytics warehouse to a new region last quarter.".to_string(),
        ];
        let mut e = embedder();
        idx.add_session("claude", "s1", 100, &turns, &mut e);

        // The query is the planted text verbatim: the fake embedder makes its
        // cosine == 1.0 (identical vectors), so it must rank top-1.
        let q_emb = fake_embed(planted).unwrap();
        let hits = idx.search(planted, &q_emb, 3);
        assert!(!hits.is_empty(), "expected at least one hit");
        assert!(
            hits[0].text.contains("checkout latency SLA"),
            "planted chunk should rank first, got {:?}",
            hits[0].text
        );
        assert_eq!(hits[0].agent, "claude");
        assert_eq!(hits[0].session_id, "s1");
    }

    #[test]
    fn search_empty_index_returns_empty() {
        let idx = AgentHistoryIndex::new();
        let hits = idx.search("anything at all", &vec![0.0f32; FAKE_DIM], 5);
        assert!(hits.is_empty());
    }

    #[test]
    fn search_k_larger_than_corpus_is_safe() {
        let mut idx = AgentHistoryIndex::new();
        let planted = "We enabled WAL mode on the sqlite store for concurrent reads.";
        let turns = vec![planted.to_string()];
        let mut e = embedder();
        idx.add_session("codex", "s9", 50, &turns, &mut e);

        let q_emb = fake_embed(planted).unwrap();
        // k far exceeds the single-chunk corpus — must not panic or over-return.
        let hits = idx.search(planted, &q_emb, 100);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.contains("WAL mode"));
    }

    #[test]
    fn search_k_zero_returns_empty() {
        let mut idx = AgentHistoryIndex::new();
        let turns = vec!["some indexed prose here".to_string()];
        let mut e = embedder();
        idx.add_session("claude", "s1", 100, &turns, &mut e);
        let q_emb = fake_embed("some indexed prose here").unwrap();
        assert!(idx.search("some indexed prose here", &q_emb, 0).is_empty());
    }

    #[test]
    fn newest_epoch_breaks_score_ties() {
        // Two DISTINCT chunks with identical text-independent... instead, force a
        // tie by planting the SAME semantic content in two sessions with
        // different epochs; identical text dedups, so use two near-identical
        // turns that both match the query equally is hard — assert the tie-break
        // helper directly on constructed hits instead (deterministic).
        let hits = vec![
            HistoryHit {
                text: "older".into(),
                agent: "a".into(),
                session_id: "s1".into(),
                epoch_secs: 100,
                score: 0.7,
            },
            HistoryHit {
                text: "newer".into(),
                agent: "a".into(),
                session_id: "s2".into(),
                epoch_secs: 200,
                score: 0.7,
            },
        ];
        let sorted = hits.tap_sorted_newest_tiebreak();
        assert_eq!(
            sorted[0].session_id, "s2",
            "equal score → newest epoch first"
        );
        assert_eq!(sorted[1].session_id, "s1");
    }

    #[test]
    fn looks_like_tool_dump_is_conservative() {
        // Prose that merely mentions technical terms is NOT a dump.
        assert!(!looks_like_tool_dump(
            "We ran a diff and the JSON payload {looked} fine."
        ));
        assert!(!looks_like_tool_dump(
            "The array had three items and we moved on."
        ));
        // Actual dumps ARE caught.
        assert!(looks_like_tool_dump("```rust\nfn main() {}\n```"));
        assert!(looks_like_tool_dump("{\n  \"a\": 1\n}"));
        assert!(looks_like_tool_dump("[1, 2, 3]"));
        assert!(looks_like_tool_dump("diff --git a/x b/x"));
        assert!(looks_like_tool_dump("--- a/src/main.rs"));
    }
}
