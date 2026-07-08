//! Hybrid retrieval scoring — mem0 v3's search pipeline ported EXACTLY from
//! source (mem0 HEAD 22f70d5, 2026-07-08: `mem0/utils/scoring.py`,
//! `mem0/utils/lemmatization.py`, `mem0/utils/entity_extraction.py`,
//! `memory/main.py::_search_vector_store/_compute_entity_boosts`).
//!
//! The pipeline: semantic cosine (over-fetched) + BM25 keyword score
//! (sigmoid-normalized with query-length-adaptive params) + entity boost
//! (query entities matched ≥0.5 in a linked entity store, weight 0.5),
//! fused additively and divided by the max possible for the active signals.
//! The relevance threshold gates the SEMANTIC score BEFORE fusion.
//!
//! Deliberate deltas from mem0 (documented, all graceful in mem0 itself):
//! - **Snowball stemming instead of spaCy lemmatization** — same role, and
//!   both index and query sides use the SAME normalizer so matching is
//!   internally consistent (mem0 without spaCy falls back to raw text).
//! - **POS-free entity extraction** — the IDENTIFIER / QUOTED / PROPER-span
//!   extractors are ported (they are casing/regex rules); the spaCy-only
//!   NER and noun-chunk TOPIC extractors are not (mem0 without spaCy
//!   extracts NOTHING, so this is strictly more than its own fallback).
//!
//! Parity: the unit tests pin `get_bm25_params` / `normalize_bm25` /
//! `score_and_rank` to values produced by RUNNING mem0's own scoring.py.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

/// mem0 `ENTITY_BOOST_WEIGHT` (scoring.py).
pub const ENTITY_BOOST_WEIGHT: f32 = 0.5;
/// Entity-store match floor for query-entity boosts (main.py:1745).
pub const ENTITY_MATCH_FLOOR: f32 = 0.5;
/// DELTA from mem0: non-exact matches where either side is a short token
/// (≤4 chars — acronyms, initialisms) need a much higher similarity.
/// bge-small embeds short acronyms too close together ("SLA"≈"SSO" clears
/// 0.5), which mem0's OpenAI-tuned floor never sees — measured live in the
/// retrieval eval 2026-07-08 as a false boost demoting the right fact.
pub const SHORT_ENTITY_MATCH_FLOOR: f32 = 0.85;

/// Accept/reject one query-entity ↔ stored-entity match. Exact normalized
/// text is always accepted; otherwise the mem0 floor applies, tightened to
/// [`SHORT_ENTITY_MATCH_FLOOR`] when either side is ≤4 chars.
pub fn entity_match_accepted(query_text: &str, stored_text: &str, similarity: f32) -> bool {
    let q = normalize_entity_text(query_text);
    let s = normalize_entity_text(stored_text);
    if q == s {
        return similarity >= ENTITY_MATCH_FLOOR;
    }
    let floor = if q.len() <= 4 || s.len() <= 4 {
        SHORT_ENTITY_MATCH_FLOOR
    } else {
        ENTITY_MATCH_FLOOR
    };
    similarity >= floor
}
/// Write-side entity dedup: semantic match threshold (main.py:1104).
pub const ENTITY_DEDUP_SIMILARITY: f32 = 0.95;
/// Max query entities considered for boosting (main.py:1699).
pub const MAX_QUERY_ENTITIES: usize = 8;
/// Semantic over-fetch: `max(limit*4, 60)` (main.py:1593).
pub fn internal_limit(limit: usize) -> usize {
    (limit * 4).max(60)
}

// ---------------------------------------------------------------------------
// Stemming / tokenization (lemmatize_for_bm25 stand-in)
// ---------------------------------------------------------------------------

/// Compact English stopword list (spaCy-style function words). Consistency
/// between index and query sides is what matters for BM25, not the exact set.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "if", "then", "than", "so", "as", "of", "at", "by",
    "for", "with", "about", "against", "between", "into", "through", "during", "before", "after",
    "above", "below", "to", "from", "up", "down", "in", "out", "on", "off", "over", "under",
    "again", "further", "once", "here", "there", "when", "where", "why", "how", "all", "any",
    "both", "each", "few", "more", "most", "other", "some", "such", "no", "nor", "not", "only",
    "own", "same", "too", "very", "can", "will", "just", "should", "now", "i", "me", "my",
    "myself", "we", "our", "ours", "you", "your", "yours", "he", "him", "his", "she", "her", "it",
    "its", "they", "them", "their", "what", "which", "who", "whom", "this", "that", "these",
    "those", "am", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
    "having", "do", "does", "did", "doing", "would", "could", "ought", "let", "us",
];

fn is_stopword(token: &str) -> bool {
    STOPWORDS.contains(&token)
}

/// Tokenize + stem for BM25 (the `lemmatize_for_bm25` port): lowercase,
/// alphanumeric tokens, stopwords dropped, Snowball-stemmed — plus the
/// ORIGINAL `-ing` form when it differs from the stem (mem0 keeps both to
/// handle noun/verb ambiguity like meeting/meet).
pub fn stem_for_bm25(text: &str) -> Vec<String> {
    use rust_stemmers::{Algorithm, Stemmer};
    static STEMMER: OnceLock<Stemmer> = OnceLock::new();
    let stemmer = STEMMER.get_or_init(|| Stemmer::create(Algorithm::English));

    let mut out = Vec::new();
    for raw in text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
    {
        if is_stopword(raw) {
            continue;
        }
        let stem = stemmer.stem(raw).to_string();
        if !stem.is_empty() {
            out.push(stem.clone());
        }
        if raw.ends_with("ing") && raw != stem {
            out.push(raw.to_string());
        }
    }
    out
}

/// Space-joined stemmed tokens — the stored `text_stemmed` column format.
pub fn stem_for_bm25_joined(text: &str) -> String {
    stem_for_bm25(text).join(" ")
}

// ---------------------------------------------------------------------------
// BM25 (Okapi) over the current-facts corpus
// ---------------------------------------------------------------------------

/// Okapi BM25 raw scores for `query_tokens` over `docs` (id, stemmed tokens).
/// k1=1.5, b=0.75, and the LUCENE/ES idf variant `ln((N-n+0.5)/(n+0.5) + 1)`
/// — deliberately: mem0's `keyword_search` is store-native (Elasticsearch /
/// pgvector), whose BM25 raw scores are Lucene-shaped, and mem0's sigmoid
/// midpoints (5..12) are calibrated against THAT distribution. (rank_bm25's
/// un-shifted idf floors negatives instead; not what the sigmoid expects.)
/// Only ids with score > 0 are returned (mem0 keeps only positive raw scores).
pub fn bm25_raw_scores<'a>(
    query_tokens: &[String],
    docs: impl Iterator<Item = (i64, &'a str)>,
) -> HashMap<i64, f32> {
    const K1: f32 = 1.5;
    const B: f32 = 0.75;

    let docs: Vec<(i64, Vec<&str>)> = docs
        .map(|(id, joined)| (id, joined.split_whitespace().collect()))
        .collect();
    let n_docs = docs.len();
    if n_docs == 0 || query_tokens.is_empty() {
        return HashMap::new();
    }
    let avg_len: f32 = docs.iter().map(|(_, t)| t.len() as f32).sum::<f32>() / n_docs as f32;
    if avg_len == 0.0 {
        return HashMap::new();
    }

    // Document frequency per query term.
    let mut df: HashMap<&str, usize> = HashMap::new();
    for term in query_tokens {
        let n = docs
            .iter()
            .filter(|(_, tokens)| tokens.contains(&term.as_str()))
            .count();
        df.insert(term.as_str(), n);
    }

    let mut scores = HashMap::new();
    for (id, tokens) in &docs {
        let len = tokens.len() as f32;
        let mut score = 0.0f32;
        for term in query_tokens {
            let n = df[term.as_str()];
            if n == 0 {
                continue;
            }
            let tf = tokens.iter().filter(|t| **t == term.as_str()).count() as f32;
            if tf == 0.0 {
                continue;
            }
            let idf = ((n_docs as f32 - n as f32 + 0.5) / (n as f32 + 0.5) + 1.0).ln();
            score += idf * (tf * (K1 + 1.0)) / (tf + K1 * (1.0 - B + B * len / avg_len));
        }
        if score > 0.0 {
            scores.insert(*id, score);
        }
    }
    scores
}

// ---------------------------------------------------------------------------
// Sigmoid normalization + fusion (scoring.py — exact port)
// ---------------------------------------------------------------------------

/// Query-length-adaptive sigmoid params `(midpoint, steepness)` —
/// `get_bm25_params`, keyed on the STEMMED term count.
pub fn bm25_params(num_terms: usize) -> (f32, f32) {
    let num_terms = num_terms.max(1);
    if num_terms <= 3 {
        (5.0, 0.7)
    } else if num_terms <= 6 {
        (7.0, 0.6)
    } else if num_terms <= 9 {
        (9.0, 0.5)
    } else if num_terms <= 15 {
        (10.0, 0.5)
    } else {
        (12.0, 0.5)
    }
}

/// `normalize_bm25`: logistic sigmoid of the raw score.
pub fn normalize_bm25(raw: f32, midpoint: f32, steepness: f32) -> f32 {
    1.0 / (1.0 + (-steepness * (raw - midpoint)).exp())
}

/// Entity boost for one matched entity: `similarity * 0.5 * weight(n_linked)`
/// where `weight = 1/(1 + 0.001*(n_linked-1)^2)` (main.py:1753-1755).
pub fn entity_boost(similarity: f32, num_linked: usize) -> f32 {
    let n = num_linked.max(1) as f32;
    let memory_count_weight = 1.0 / (1.0 + 0.001 * (n - 1.0) * (n - 1.0));
    similarity * ENTITY_BOOST_WEIGHT * memory_count_weight
}

/// One semantic candidate entering fusion.
#[derive(Debug, Clone)]
pub struct SemanticCandidate {
    pub id: i64,
    pub semantic: f32,
}

/// One fused result: combined score for ranking, semantic kept for gating.
#[derive(Debug, Clone)]
pub struct RankedHit {
    pub id: i64,
    pub combined: f32,
    pub semantic: f32,
}

/// `score_and_rank` — exact port. The threshold gates the SEMANTIC score
/// BEFORE combining; `combined = min((semantic+bm25+entity)/max_possible, 1)`
/// with `max_possible = 1.0 + 1.0*has_bm25 + 0.5*has_entity` (signal
/// presence is corpus-wide, not per-candidate).
pub fn score_and_rank(
    semantic: &[SemanticCandidate],
    bm25: &HashMap<i64, f32>,
    entity: &HashMap<i64, f32>,
    threshold: f32,
    top_k: usize,
) -> Vec<RankedHit> {
    let mut max_possible = 1.0f32;
    if !bm25.is_empty() {
        max_possible += 1.0;
    }
    if !entity.is_empty() {
        max_possible += ENTITY_BOOST_WEIGHT;
    }

    let mut scored: Vec<RankedHit> = semantic
        .iter()
        .filter(|c| c.semantic >= threshold)
        .map(|c| {
            let raw = c.semantic
                + bm25.get(&c.id).copied().unwrap_or(0.0)
                + entity.get(&c.id).copied().unwrap_or(0.0);
            RankedHit {
                id: c.id,
                combined: (raw / max_possible).min(1.0),
                semantic: c.semantic,
            }
        })
        .collect();
    scored.sort_by(|a, b| {
        b.combined
            .partial_cmp(&a.combined)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(top_k);
    scored
}

// ---------------------------------------------------------------------------
// Entity extraction (POS-free port of entity_extraction.py)
// ---------------------------------------------------------------------------

/// Entity type labels (mem0's PROPER / QUOTED / IDENTIFIER; TOPIC needs a
/// dependency parse and is not ported).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityType {
    Proper,
    Quoted,
    Identifier,
}

impl EntityType {
    pub fn label(self) -> &'static str {
        match self {
            EntityType::Proper => "PROPER",
            EntityType::Quoted => "QUOTED",
            EntityType::Identifier => "IDENTIFIER",
        }
    }
}

/// `_normalize_entity_text` (main.py:540) — lowercase, whitespace-collapsed.
pub fn normalize_entity_text(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Generic role words / capitalized-generic words that never become
/// single-token entities (`_GENERIC_SINGLE_ENTITY_TERMS` + `_GENERIC_CAPS`).
const GENERIC_SINGLE: &[&str] = &[
    "user",
    "assistant",
    "agent",
    "customer",
    "client",
    "person",
    "people",
    "human",
    "memory",
    "message",
    "conversation",
    "chat",
    "session",
    "system",
    "top",
    "works",
    "items",
    "things",
    "stuff",
    "resources",
    "options",
    "tips",
    "ideas",
    "steps",
    "ways",
    "methods",
    "tools",
    "features",
    "benefits",
    "examples",
    "details",
    "notes",
    "instructions",
    "guidelines",
    "recommendations",
    "suggestions",
    "overview",
    "summary",
    "conclusion",
    "introduction",
    "pros",
    "cons",
    "advantages",
    "disadvantages",
];

fn identifier_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // `_looks_like_technical_identifier`: dotted technical ids (a.b, pkg.mod.fn).
    RE.get_or_init(|| Regex::new(r"^[A-Za-z_][\w-]*(?:\.[A-Za-z_][\w-]*)+$").unwrap())
}

fn double_quoted_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#""([^"]+)""#).unwrap())
}

fn single_quoted_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Bare span only — boundaries are validated manually (Rust regex has no
    // lookaround; consuming boundary chars here made adjacent quoted spans
    // unmatchable: "'alpha' 'beta'" lost beta — review finding).
    RE.get_or_init(|| Regex::new(r"'([^']+)'").unwrap())
}

/// Python-lookaround-equivalent boundary checks for single-quoted matches.
fn single_quote_boundaries_ok(text: &str, start: usize, end: usize) -> bool {
    let before_ok = start == 0
        || text[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_whitespace() || "([{,;".contains(c));
    let after_ok = end == text.len()
        || text[end..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || ".,;:!?)]".contains(c));
    before_ok && after_ok
}

/// `_clean_text`: strip leading/trailing asterisks, trailing colons, leading
/// list numbers, collapse whitespace.
fn clean_text(txt: &str) -> String {
    let t = txt.trim();
    let t = t.trim_matches('*').trim();
    let t = t.trim_end_matches(':').trim();
    let t = {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"^\d+\s*\.\s*").unwrap());
        re.replace(t, "").to_string()
    };
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `_has_artifacts`: formatting junk that disqualifies a candidate.
fn has_artifacts(txt: &str) -> bool {
    txt.contains("**")
        || txt.contains("__")
        || txt.contains(":*")
        || txt.contains("  ")
        || txt.contains('\n')
        || txt.contains('\t')
        || txt.len() > 100
        || txt.starts_with(['\u{2022}', '-', '+', '\u{2013}', '\u{2014}'])
}

#[derive(Debug, Clone)]
struct Candidate {
    entity_type: EntityType,
    text: String,
    start: isize,
    end: isize,
    confidence: f32,
    priority: i32,
}

fn push_candidate(
    out: &mut Vec<Candidate>,
    entity_type: EntityType,
    text: &str,
    start: isize,
    end: isize,
    confidence: f32,
    priority: i32,
) {
    let cleaned = clean_text(text);
    if cleaned.len() <= 2 || has_artifacts(&cleaned) {
        return;
    }
    out.push(Candidate {
        entity_type,
        text: cleaned,
        start,
        end,
        confidence,
        priority,
    });
}

/// Word-level token for the PROPER-span walker.
struct Tok<'a> {
    text: &'a str,
    index: usize,
    sentence_start: bool,
}

fn word_tokens(text: &str) -> Vec<Tok<'_>> {
    let mut toks = Vec::new();
    let mut sentence_start = true;
    for (index, raw) in text.split_whitespace().enumerate() {
        let this_starts = sentence_start;
        // Next token starts a sentence if this one ends with terminal
        // punctuation / a colon, or is a formatting marker.
        let trimmed = raw.trim_end_matches(['"', '\'', ')', ']']);
        sentence_start = trimmed.ends_with(['.', '!', '?', ':'])
            || matches!(raw, "*" | "-" | "+" | "\u{2022}" | "#" | "##" | "###");
        toks.push(Tok {
            text: raw,
            index,
            sentence_start: this_starts,
        });
    }
    toks
}

fn strip_token_punct(raw: &str) -> &str {
    raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_' && c != '.')
}

fn has_internal_cap_or_digit(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit()) || text.chars().skip(1).any(|c| c.is_uppercase())
}

/// Shared name checks minus the position rule: capitalized, alphabetic
/// content, not a generic/stopword term.
fn name_like_core(tok: &Tok<'_>) -> bool {
    let core = strip_token_punct(tok.text);
    if core.is_empty() {
        return false;
    }
    let first = core.chars().next().unwrap();
    if !first.is_uppercase() {
        return false;
    }
    if !core.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    let lower = core.to_lowercase();
    !(GENERIC_SINGLE.contains(&lower.as_str()) || is_stopword(&lower))
}

/// `_is_name_like_token`, POS-free subset: mid-sentence capitalization or an
/// internal cap/digit marks a name; sentence-start capitalization alone does
/// not (the stand-in for spaCy's PROPN check — see `span_can_start_at` for
/// the sentence-start recovery rule).
fn is_name_like(tok: &Tok<'_>) -> bool {
    name_like_core(tok)
        && (has_internal_cap_or_digit(strip_token_punct(tok.text)) || !tok.sentence_start)
}

/// A PROPER span may also START on a sentence-start capitalized token when
/// the FOLLOWING token is itself name-like ("Raj Patel owns ..."): spaCy's
/// PROPN tag accepts names in any position; a capitalized bigram is the
/// POS-free equivalent signal.
fn span_can_start_at(toks: &[Tok<'_>], i: usize) -> bool {
    if is_name_like(&toks[i]) {
        return true;
    }
    name_like_core(&toks[i]) && i + 1 < toks.len() && is_name_like(&toks[i + 1])
}

/// Extract typed entities from text — the POS-free port of
/// `extract_entities` (IDENTIFIER + PROPER spans + QUOTED; spaCy NER and
/// noun-chunk TOPIC phrases are not portable without a tagger).
pub fn extract_entities(text: &str) -> Vec<(EntityType, String)> {
    let mut candidates = Vec::new();
    let toks = word_tokens(text);

    // IDENTIFIER (priority 1, confidence 0.9): dotted technical identifiers.
    for tok in &toks {
        let core = strip_token_punct(tok.text);
        if identifier_re().is_match(core) {
            push_candidate(
                &mut candidates,
                EntityType::Identifier,
                core,
                tok.index as isize,
                tok.index as isize + 1,
                0.9,
                1,
            );
        }
    }

    // PROPER spans (priority 2, confidence 0.8): runs of name-like tokens,
    // allowing inner connectors {of,the,for,at,in} between name-like tokens.
    const CONNECTORS: &[&str] = &["of", "the", "for", "at", "in"];
    let mut i = 0;
    while i < toks.len() {
        if !span_can_start_at(&toks, i) {
            i += 1;
            continue;
        }
        let start = i;
        let mut span: Vec<&str> = vec![strip_token_punct(toks[i].text)];
        let mut j = i + 1;
        while j < toks.len() {
            if is_name_like(&toks[j]) {
                span.push(strip_token_punct(toks[j].text));
                j += 1;
                continue;
            }
            let lower = strip_token_punct(toks[j].text).to_lowercase();
            if CONNECTORS.contains(&lower.as_str())
                && j + 1 < toks.len()
                && is_name_like(&toks[j + 1])
            {
                span.push(strip_token_punct(toks[j].text));
                span.push(strip_token_punct(toks[j + 1].text));
                j += 2;
                continue;
            }
            break;
        }
        // Single-token spans must not be generic (mirrors the
        // `_is_bad_single_name_token` re-check on the collapsed span).
        let single_generic = span.len() == 1
            && (GENERIC_SINGLE.contains(&span[0].to_lowercase().as_str())
                || is_stopword(&span[0].to_lowercase()));
        if !single_generic {
            push_candidate(
                &mut candidates,
                EntityType::Proper,
                &span.join(" "),
                start as isize,
                j as isize,
                0.8,
                2,
            );
        }
        i = j.max(i + 1);
    }

    // QUOTED (priority 3, confidence 0.75): double- and single-quoted spans.
    for m in double_quoted_re().captures_iter(text) {
        let inner = m[1].trim();
        if inner.len() > 2 {
            push_candidate(&mut candidates, EntityType::Quoted, inner, -1, -1, 0.75, 3);
        }
    }
    for m in single_quoted_re().captures_iter(text) {
        let whole = m.get(0).unwrap();
        if !single_quote_boundaries_ok(text, whole.start(), whole.end()) {
            continue;
        }
        let inner = m[1].trim();
        if inner.len() > 2 {
            push_candidate(&mut candidates, EntityType::Quoted, inner, -1, -1, 0.75, 3);
        }
    }

    resolve_candidates(candidates)
}

/// `_resolve_candidates` — dedupe by normalized text (best priority wins),
/// then suppress span overlaps, then order by position.
fn resolve_candidates(candidates: Vec<Candidate>) -> Vec<(EntityType, String)> {
    let mut by_text: HashMap<String, Candidate> = HashMap::new();
    for c in candidates {
        let key = normalize_entity_text(&c.text);
        match by_text.get(&key) {
            Some(cur)
                if (cur.priority, -(cur.confidence * 1000.0) as i64)
                    <= (c.priority, -(c.confidence * 1000.0) as i64) => {}
            _ => {
                by_text.insert(key, c);
            }
        }
    }
    let mut ordered: Vec<Candidate> = by_text.into_values().collect();
    ordered.sort_by(|a, b| {
        (
            a.priority,
            -(a.confidence * 1000.0) as i64,
            -(a.end - a.start),
            a.start,
        )
            .cmp(&(
                b.priority,
                -(b.confidence * 1000.0) as i64,
                -(b.end - b.start),
                b.start,
            ))
    });

    let mut accepted: Vec<Candidate> = Vec::new();
    for c in ordered {
        let overlaps = accepted
            .iter()
            .any(|e| c.start >= 0 && e.start >= 0 && c.start < e.end && e.start < c.end);
        if !overlaps {
            accepted.push(c);
        }
    }
    accepted.sort_by_key(|c| (if c.start >= 0 { c.start } else { isize::MAX }, c.end));
    accepted
        .into_iter()
        .map(|c| (c.entity_type, c.text))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------ PARITY GATE: values produced by RUNNING mem0's scoring.py ------

    #[test]
    fn bm25_params_match_mem0_table() {
        // Captured from get_bm25_params (mem0 HEAD 22f70d5) at each boundary.
        for (n, expected) in [
            (1, (5.0, 0.7)),
            (3, (5.0, 0.7)),
            (4, (7.0, 0.6)),
            (6, (7.0, 0.6)),
            (7, (9.0, 0.5)),
            (9, (9.0, 0.5)),
            (10, (10.0, 0.5)),
            (15, (10.0, 0.5)),
            (16, (12.0, 0.5)),
        ] {
            assert_eq!(bm25_params(n), expected, "n={n}");
        }
    }

    #[test]
    fn sigmoid_matches_mem0_normalize_bm25() {
        // (raw, midpoint, steepness, expected) — expected values captured by
        // executing mem0's normalize_bm25.
        for (raw, mid, steep, expected) in [
            (5.0, 5.0, 0.7, 0.5),
            (8.2, 7.0, 0.6, 0.672_607_017_067_760_3),
            (1.3, 9.0, 0.5, 0.020_836_344_518_680_425),
            (14.0, 12.0, 0.5, 0.731_058_578_630_004_9),
        ] {
            let got = normalize_bm25(raw, mid, steep);
            assert!(
                (got as f64 - expected).abs() < 1e-6,
                "raw={raw}: got {got}, want {expected}"
            );
        }
    }

    #[test]
    fn fusion_matches_mem0_score_and_rank() {
        let sem = vec![
            SemanticCandidate {
                id: 1,
                semantic: 0.82,
            }, // "a"
            SemanticCandidate {
                id: 2,
                semantic: 0.61,
            }, // "b"
            SemanticCandidate {
                id: 3,
                semantic: 0.30,
            }, // "c" — below gate
            SemanticCandidate {
                id: 4,
                semantic: 0.55,
            }, // "d"
        ];
        let bm25 = HashMap::from([(2, 0.9f32), (4, 0.4f32)]);
        let ents = HashMap::from([(1, 0.35f32), (4, 0.5f32)]);

        // All three signals: max_possible = 2.5. Captured expected order and
        // scores from mem0: b=0.604, d=0.58, a=0.468 (c gated out).
        let out = score_and_rank(&sem, &bm25, &ents, 0.45, 3);
        assert_eq!(out.iter().map(|h| h.id).collect::<Vec<_>>(), vec![2, 4, 1]);
        for (hit, expected) in out.iter().zip([0.604f32, 0.58, 0.468]) {
            assert!(
                (hit.combined - expected).abs() < 1e-5,
                "{hit:?} != {expected}"
            );
        }

        // Semantic only: scores pass through unchanged, c still gated.
        let out = score_and_rank(&sem, &HashMap::new(), &HashMap::new(), 0.45, 4);
        assert_eq!(out.iter().map(|h| h.id).collect::<Vec<_>>(), vec![1, 2, 4]);
        assert!((out[0].combined - 0.82).abs() < 1e-6);

        // BM25 only: max_possible = 2.0 → b=0.755, d=0.475, a=0.41.
        let out = score_and_rank(&sem, &bm25, &HashMap::new(), 0.45, 4);
        assert_eq!(out.iter().map(|h| h.id).collect::<Vec<_>>(), vec![2, 4, 1]);
        for (hit, expected) in out.iter().zip([0.755f32, 0.475, 0.41]) {
            assert!(
                (hit.combined - expected).abs() < 1e-5,
                "{hit:?} != {expected}"
            );
        }
    }

    // ------------------------- port-behavior tests -------------------------

    #[test]
    fn entity_boost_matches_mem0_formula() {
        // boost = sim * 0.5 * 1/(1+0.001*(n-1)^2)
        assert!((entity_boost(1.0, 1) - 0.5).abs() < 1e-6);
        assert!((entity_boost(0.8, 1) - 0.4).abs() < 1e-6);
        let b = entity_boost(1.0, 11); // weight = 1/(1+0.1) = 0.9090..
        assert!((b - 0.454_545_5).abs() < 1e-5);
    }

    #[test]
    fn stemming_keeps_ing_originals_and_drops_stopwords() {
        let toks = stem_for_bm25("The team is attending the planning meeting for checkout");
        // stopwords gone
        assert!(!toks.contains(&"the".to_string()));
        assert!(!toks.contains(&"is".to_string()));
        // -ing originals kept alongside stems (meeting/meet ambiguity rule)
        assert!(toks.contains(&"meeting".to_string()));
        assert!(toks.contains(&"attending".to_string()));
        // stems present
        assert!(toks.iter().any(|t| t.starts_with("checkout")));
    }

    #[test]
    fn bm25_ranks_exact_keyword_doc_highest() {
        let q = stem_for_bm25("checkout latency SLA");
        let docs = [
            (
                1i64,
                stem_for_bm25_joined("the checkout latency SLA is 200ms"),
            ),
            (
                2i64,
                stem_for_bm25_joined("Dana owns the authentication rollout"),
            ),
            (
                3i64,
                stem_for_bm25_joined("checkout flow uses the payments service"),
            ),
        ];
        let scores = bm25_raw_scores(&q, docs.iter().map(|(id, s)| (*id, s.as_str())));
        assert!(scores[&1] > scores[&3], "{scores:?}");
        assert!(!scores.contains_key(&2), "no shared terms → no score");
    }

    #[test]
    fn extracts_identifiers_quoted_and_proper_spans() {
        let ents = extract_entities(
            r#"Raj Patel owns cue_daemon.memory now. See "the payments runbook" for details."#,
        );
        let texts: Vec<&str> = ents.iter().map(|(_, t)| t.as_str()).collect();
        assert!(texts.contains(&"cue_daemon.memory"), "{texts:?}");
        assert!(texts.contains(&"the payments runbook"), "{texts:?}");
        assert!(texts.contains(&"Raj Patel"), "{texts:?}");
        // Sentence-start capitalization alone ("See") is NOT an entity.
        assert!(!texts.contains(&"See"), "{texts:?}");
    }

    #[test]
    fn generic_capitalized_words_are_not_entities() {
        let ents = extract_entities("The User asked the Assistant about Tips");
        let texts: Vec<&str> = ents.iter().map(|(_, t)| t.as_str()).collect();
        assert!(texts.is_empty(), "{texts:?}");
    }

    #[test]
    fn internal_limit_matches_mem0() {
        assert_eq!(internal_limit(4), 60);
        assert_eq!(internal_limit(20), 80);
    }

    #[test]
    fn adjacent_single_quoted_spans_both_extract() {
        // The consuming-boundary regression: "'alpha' 'beta'" lost beta.
        let ents = extract_entities("see 'alpha flow' 'beta flow' docs");
        let texts: Vec<&str> = ents.iter().map(|(_, t)| t.as_str()).collect();
        assert!(texts.contains(&"alpha flow"), "{texts:?}");
        assert!(texts.contains(&"beta flow"), "{texts:?}");
        // Apostrophes inside words are not quote boundaries.
        let ents = extract_entities("that's Raj's plan and it's fine");
        assert!(
            !ents.iter().any(|(t, _)| *t == EntityType::Quoted),
            "{ents:?}"
        );
    }

    #[test]
    fn short_entity_matches_need_the_stricter_floor() {
        // Exact text: mem0's floor applies regardless of length.
        assert!(entity_match_accepted("SLA", "sla", 0.6));
        // Non-exact acronym pair at mem0's floor: rejected (the live
        // "SLA"≈"SSO" false-boost failure).
        assert!(!entity_match_accepted("SLA", "SSO", 0.66));
        assert!(entity_match_accepted("SLA", "SLAs", 0.9));
        // Long entities keep mem0's original floor.
        assert!(entity_match_accepted(
            "payments runbook",
            "payment runbooks",
            0.62
        ));
    }
}
