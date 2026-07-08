//! Cross-meeting facts memory — daemon orchestration (feature `local-memory`).
//!
//! The long-term tier of the two-tier memory model (PLAN-CONTEXT-WARMUP
//! Appendix A/E): quote-verified ledger facts are embedded with the LOCAL
//! bge-small ONNX embedder (keyless, on-device — never OpenAI) and stored in
//! `facts_memory.db`, then recalled semantically across ALL past meetings on
//! the answer path. The store holds EXTRACTED FACTS, never raw transcript
//! (the measured rule: facts ≈100% top-3 recall; raw transcript confidently
//! mismatches).
//!
//! Model files (`model_int8.onnx` + `tokenizer.json`, ~35MB) download once on
//! first run — same install-and-it-just-works pattern as the STT model — into
//! `<data_dir>/models/bge-small-en/` (override: `BLUEY_EMBED_MODEL_DIR`;
//! mirror: `BLUEY_EMBED_MODEL_URL_BASE`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cue_core::app_paths::AppPaths;
use cue_rag::{EmbeddingProvider, FactHit, FactRow, FactsStore, LocalBgeEmbedder};
use tracing::{debug, info};

/// Xenova's ONNX export of BAAI/bge-small-en-v1.5 (verified public, int8 +
/// tokenizer.json). int8 keeps the download small (~34MB) at negligible
/// retrieval cost for extracted-fact inputs.
const DEFAULT_MODEL_URL_BASE: &str = "https://huggingface.co/Xenova/bge-small-en-v1.5/resolve/main";

const MODEL_FILE: &str = "model_int8.onnx";
const TOKENIZER_FILE: &str = "tokenizer.json";

/// The assembled long-term memory: local embedder + facts store.
pub struct FactsMemory {
    embedder: LocalBgeEmbedder,
    store: FactsStore,
}

impl FactsMemory {
    /// Ensure model files (downloading on first run), load the embedder, open
    /// the store. Any failure means "memory off" — never fatal to the daemon.
    pub async fn ensure(paths: &AppPaths) -> Result<Self> {
        let model_dir = resolve_model_dir(paths);
        tokio::fs::create_dir_all(&model_dir)
            .await
            .with_context(|| format!("create {}", model_dir.display()))?;
        let base = std::env::var("BLUEY_EMBED_MODEL_URL_BASE")
            .ok()
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_else(|| DEFAULT_MODEL_URL_BASE.to_string());

        let model_path = model_dir.join(MODEL_FILE);
        if !model_path.is_file() {
            info!("embedding model not found; downloading on first run (~34MB, one-time)");
            download_file(&format!("{base}/onnx/{MODEL_FILE}"), &model_path).await?;
        }
        let tokenizer_path = model_dir.join(TOKENIZER_FILE);
        if !tokenizer_path.is_file() {
            download_file(&format!("{base}/{TOKENIZER_FILE}"), &tokenizer_path).await?;
        }

        // Session load is blocking CPU work — keep it off the async runtime.
        let embedder = {
            let (m, t) = (model_path.clone(), tokenizer_path.clone());
            tokio::task::spawn_blocking(move || LocalBgeEmbedder::load(&m, &t))
                .await
                .context("embedder load task")?
                .map_err(|e| anyhow::anyhow!("load local embedder: {e}"))?
        };
        let store = FactsStore::open(
            &paths.data_dir.join("facts_memory.db"),
            LocalBgeEmbedder::DIM,
        )?;
        info!(
            facts = store.current_len().unwrap_or(0),
            "cross-meeting facts memory ready (local bge-small, keyless)"
        );
        Ok(Self { embedder, store })
    }

    /// Phase-2 preparation (the Mem0 update phase, PLAN-CONTEXT-WARMUP E.1):
    /// embed each candidate fact, drop exact-known ones pre-agent (mem0 v3's
    /// hash dedup — those are NONE without costing a drive), retrieve the
    /// top-similar CURRENT facts per candidate (paper s=10, union capped at
    /// 10 presented), and build the update-decision prompt. `prompt` is
    /// `None` when there is nothing to ask the agent about (no candidates
    /// left, or empty neighborhood → everything is a plain ADD) — the caller
    /// then applies [`Self::apply_heuristic`] directly.
    pub async fn prepare_update(&self, candidate_texts: &[String]) -> Result<UpdatePlan> {
        let mut candidates = Vec::new();
        for text in candidate_texts {
            let text = text.trim();
            if text.is_empty() || self.store.contains_exact(text)? {
                continue;
            }
            let embedding = self
                .embedder
                .embed(text)
                .await
                .map_err(|e| anyhow::anyhow!("embed candidate: {e}"))?;
            candidates.push(Candidate {
                text: text.to_string(),
                embedding,
            });
        }
        if candidates.is_empty() {
            return Ok(UpdatePlan {
                candidates,
                existing: Vec::new(),
                prompt: None,
            });
        }
        // Union of per-candidate neighborhoods, best score per fact id.
        let mut by_id: std::collections::HashMap<i64, FactRow> = std::collections::HashMap::new();
        for candidate in &candidates {
            for row in self
                .store
                .similar_current(&candidate.embedding, SIMILAR_PER_FACT)?
            {
                let keep = by_id
                    .get(&row.id)
                    .map(|held| row.score > held.score)
                    .unwrap_or(true);
                if keep {
                    by_id.insert(row.id, row);
                }
            }
        }
        let mut existing: Vec<FactRow> = by_id.into_values().collect();
        existing.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        existing.truncate(MAX_PRESENTED);
        let prompt = (!existing.is_empty()).then(|| build_update_prompt(&existing, &candidates));
        Ok(UpdatePlan {
            candidates,
            existing,
            prompt,
        })
    }

    /// Apply the agent's update decisions. Display ids ("0","1",…) map back
    /// to real row ids through the plan (mem0's anti-hallucination
    /// indirection); rows citing unknown ids or events are skipped, counted,
    /// and never touch the store. Errors only on store failure — a fully
    /// unusable response errs so the caller can fall back to the heuristic.
    pub async fn apply_agent_ops(
        &self,
        plan: &UpdatePlan,
        raw: &str,
        meeting_id: &str,
    ) -> Result<UpdateReport> {
        let ops = parse_update_ops(raw)?;
        let mut report = UpdateReport::default();
        for op in ops {
            match op.event.to_ascii_uppercase().as_str() {
                "ADD" => {
                    let Some(text) = op.text.as_deref().map(str::trim).filter(|t| !t.is_empty())
                    else {
                        report.skipped += 1;
                        continue;
                    };
                    let embedding = self.embedding_for(plan, text).await?;
                    match self.store.insert_fact(meeting_id, text, &embedding)? {
                        Some(id) => {
                            report.added += 1;
                            self.link_entities(id, text).await;
                        }
                        None => report.none += 1, // exact dup — already known
                    }
                }
                "UPDATE" => {
                    let (Some(old_id), Some(text)) = (
                        op.display_id
                            .and_then(|idx| plan.existing.get(idx))
                            .map(|r| r.id),
                        op.text.as_deref().map(str::trim).filter(|t| !t.is_empty()),
                    ) else {
                        report.skipped += 1;
                        continue;
                    };
                    let embedding = self.embedding_for(plan, text).await?;
                    match self
                        .store
                        .supersede_fact(old_id, meeting_id, text, &embedding)?
                    {
                        Some(id) => {
                            report.updated += 1;
                            self.link_entities(id, text).await;
                        }
                        None => report.skipped += 1, // no longer current
                    }
                }
                "DELETE" => {
                    let Some(old_id) = op
                        .display_id
                        .and_then(|idx| plan.existing.get(idx))
                        .map(|r| r.id)
                    else {
                        report.skipped += 1;
                        continue;
                    };
                    if self.store.invalidate_fact(old_id)? {
                        report.deleted += 1;
                    } else {
                        report.skipped += 1;
                    }
                }
                "NONE" => report.none += 1,
                _ => report.skipped += 1,
            }
        }
        Ok(report)
    }

    /// Fallback consolidation when no agent is attached or its output was
    /// unusable: the similarity heuristic (module docs in [`cue_rag::facts`])
    /// per candidate, reusing the embeddings computed at prepare time.
    pub async fn apply_heuristic(
        &self,
        plan: &UpdatePlan,
        meeting_id: &str,
    ) -> Result<UpdateReport> {
        let mut report = UpdateReport::default();
        for candidate in &plan.candidates {
            match self
                .store
                .add_fact(meeting_id, &candidate.text, &candidate.embedding)?
            {
                cue_rag::AddOutcome::Added { id } => {
                    report.added += 1;
                    self.link_entities(id, &candidate.text).await;
                }
                cue_rag::AddOutcome::Superseded { id, .. } => {
                    report.updated += 1;
                    self.link_entities(id, &candidate.text).await;
                }
                cue_rag::AddOutcome::Duplicate => report.none += 1,
            }
        }
        Ok(report)
    }

    /// Pure-cosine baseline search (the pre-hybrid behavior). Kept as an
    /// eval/debug surface so retrieval changes stay MEASURED against the
    /// baseline (see `facts_memory_real::hybrid_beats_cosine_baseline`).
    pub async fn search_semantic_only(
        &self,
        question: &str,
        k: usize,
        exclude_meeting: Option<&str>,
    ) -> Result<Vec<FactHit>> {
        let embedding = self
            .embedder
            .embed_query(question)
            .await
            .map_err(|e| anyhow::anyhow!("embed query: {e}"))?;
        self.store.query(&embedding, k, exclude_meeting)
    }

    /// Extract + embed + upsert this fact's entities into the linked entity
    /// store (mem0's write-side entity linking, main.py phase 7). Best-effort:
    /// entity failures must never fail the fact write itself.
    async fn link_entities(&self, fact_id: i64, text: &str) {
        // Our facts carry a leading "[Decision]/[Constraint]/[Owner]" kind
        // label — strip it so the label never fuses into an entity span
        // ("Owner Raj") or becomes an entity itself.
        let (label, text) = match text.trim().strip_prefix('[') {
            Some(rest) => match rest.split_once(']') {
                Some((label, t)) => (Some(label.trim()), t.trim()),
                None => (None, text),
            },
            None => (None, text),
        };
        let mut entities = cue_rag::hybrid::extract_entities(text);
        // Owner facts carry the name FIRST ("Raj owns …" / "Raj — billing"),
        // where the POS-free extractor cannot tell a sentence-initial name
        // from a capitalized sentence opener (found by adversarial review:
        // owner entities silently never linked). The ledger's own structure
        // is the reliable signal — the owner name is the text before the
        // verb/dash — so upsert it deterministically.
        if label == Some("Owner") {
            let name: String = text
                .split([',', '—', '-'])
                .next()
                .unwrap_or(text)
                .split_whitespace()
                .take_while(|w| w.chars().next().is_some_and(char::is_uppercase))
                .collect::<Vec<_>>()
                .join(" ");
            if name.len() > 2
                && !entities.iter().any(|(_, t)| {
                    cue_rag::hybrid::normalize_entity_text(t)
                        .contains(&cue_rag::hybrid::normalize_entity_text(&name))
                })
            {
                entities.push((cue_rag::hybrid::EntityType::Proper, name));
            }
        }
        for (entity_type, entity_text) in entities {
            match self.embedder.embed(&entity_text).await {
                Ok(embedding) => {
                    if let Err(error) = self.store.upsert_entity(
                        entity_type.label(),
                        &entity_text,
                        &embedding,
                        fact_id,
                    ) {
                        debug!("entity upsert failed for {entity_text:?}: {error:#}");
                    }
                }
                Err(error) => debug!("entity embed failed for {entity_text:?}: {error}"),
            }
        }
    }

    /// Embedding for an op's final text: reuse the candidate's embedding when
    /// the agent kept the text verbatim, embed fresh when it merged/revised.
    async fn embedding_for(&self, plan: &UpdatePlan, text: &str) -> Result<Vec<f32>> {
        if let Some(candidate) = plan
            .candidates
            .iter()
            .find(|c| normalized_eq(&c.text, text))
        {
            return Ok(candidate.embedding.clone());
        }
        self.embedder
            .embed(text)
            .await
            .map_err(|e| anyhow::anyhow!("embed revised fact: {e}"))
    }

    /// Cross-meeting recall for a question, excluding the active meeting (its
    /// ledger is already pinned in context). Runs the full mem0 v3 hybrid
    /// pipeline: semantic + BM25 (sigmoid-normalized) + entity boosts, fused
    /// by [`cue_rag::hybrid::score_and_rank`] — the relevance floor gates the
    /// SEMANTIC score (mem0's threshold contract), fusion decides RANKING.
    pub async fn search(
        &self,
        question: &str,
        k: usize,
        exclude_meeting: Option<&str>,
    ) -> Result<Vec<FactHit>> {
        let embedding = self
            .embedder
            .embed_query(question)
            .await
            .map_err(|e| anyhow::anyhow!("embed query: {e}"))?;
        let query_stemmed = cue_rag::hybrid::stem_for_bm25(question);

        // Query-entity boosts (mem0 _compute_entity_boosts: cap 8, dedupe,
        // match ≥0.5, boost = sim * 0.5 * count-weight, max per fact).
        let mut boosts: std::collections::HashMap<i64, f32> = std::collections::HashMap::new();
        let mut seen = std::collections::HashSet::new();
        for (_, entity_text) in cue_rag::hybrid::extract_entities(question)
            .into_iter()
            .take(cue_rag::hybrid::MAX_QUERY_ENTITIES)
        {
            let key = cue_rag::hybrid::normalize_entity_text(&entity_text);
            if key.is_empty() || !seen.insert(key) {
                continue;
            }
            // Passage mode on BOTH sides: entity↔entity matching is symmetric
            // (the bge query prefix is tuned for question→passage, and mixing
            // modes systematically lowers similarities — review finding).
            let entity_embedding = match self.embedder.embed(&entity_text).await {
                Ok(e) => e,
                Err(error) => {
                    debug!("entity query embed failed for {entity_text:?}: {error}");
                    continue;
                }
            };
            for (similarity, stored_text, linked) in self.store.entity_matches(&entity_embedding)? {
                // Short/acronym entities need the stricter acceptance rule —
                // bge-small clusters acronyms ("SLA"≈"SSO") past mem0's floor.
                if !cue_rag::hybrid::entity_match_accepted(&entity_text, &stored_text, similarity) {
                    continue;
                }
                let boost = cue_rag::hybrid::entity_boost(similarity, linked.len());
                for fact_id in linked {
                    boosts
                        .entry(fact_id)
                        .and_modify(|b| *b = b.max(boost))
                        .or_insert(boost);
                }
            }
        }

        self.store.hybrid_query(
            &query_stemmed,
            &embedding,
            &boosts,
            k,
            exclude_meeting,
            RELEVANCE_FLOOR,
        )
    }
}

/// Relevance floor for cross-meeting recall: gates the SEMANTIC score before
/// fusion (mem0's threshold contract; 0.45 verified against real bge-small
/// scores on extracted facts).
pub const RELEVANCE_FLOOR: f32 = 0.45;

/// Paper s: similar existing memories retrieved per candidate fact.
const SIMILAR_PER_FACT: usize = 10;
/// mem0 presents at most the top-10 existing memories to the decision LLM.
const MAX_PRESENTED: usize = 10;

/// One new fact awaiting consolidation (text + its passage embedding).
struct Candidate {
    text: String,
    embedding: Vec<f32>,
}

/// The prepared update phase: candidates, the existing facts presented to the
/// agent (display index = position), and the decision prompt (None → nothing
/// to decide, apply the heuristic directly).
pub struct UpdatePlan {
    candidates: Vec<Candidate>,
    existing: Vec<FactRow>,
    prompt: Option<String>,
}

impl UpdatePlan {
    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }
}

/// What a consolidation pass did (counts for the debug log + tests).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UpdateReport {
    pub added: usize,
    pub updated: usize,
    pub deleted: usize,
    pub none: usize,
    pub skipped: usize,
}

/// One parsed decision row from the agent.
struct UpdateOp {
    event: String,
    /// Display index into the presented existing list ("0", "1", … or bare
    /// integers — both accepted).
    display_id: Option<usize>,
    text: Option<String>,
}

/// Near-verbatim port of mem0's `DEFAULT_UPDATE_MEMORY_PROMPT` +
/// `get_update_memory_messages` assembly (configs/prompts.py:176,406 —
/// verified against source 2026-07), with the personal-assistant examples
/// swapped for engineering-meeting ones. Existing facts are shown with small
/// integer display ids, never store ids (mem0's anti-hallucination trick).
fn build_update_prompt(existing: &[FactRow], candidates: &[Candidate]) -> String {
    let mut current = String::new();
    for (idx, row) in existing.iter().enumerate() {
        current.push_str(&format!(
            "{}\n",
            serde_json::json!({"id": idx.to_string(), "text": row.text})
        ));
    }
    let facts = serde_json::json!(candidates
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>());
    format!(
        r#"You are a smart memory manager which controls the engineering memory of a meeting copilot. You can perform four operations: (1) add into the memory, (2) update the memory, (3) delete from the memory, and (4) no change.

Compare the newly retrieved facts with the existing memory. For each new fact, decide whether to:
- ADD: Add it to the memory as a new element
- UPDATE: Update an existing memory element
- DELETE: Delete an existing memory element
- NONE: Make no change (if the fact is already present or irrelevant)

Guidelines:
1. ADD: If the retrieved facts contain new information not present in the memory, add it. New rows continue the integer id sequence.
2. UPDATE: If the retrieved facts contain information already present but materially different or richer, update that element. Example: memory has "Checkout SLA is 200ms" and the new fact is "Checkout SLA moved to 300ms after the load test" -> UPDATE. If a fact conveys the SAME thing as an existing element in different words, use NONE, not UPDATE. While updating you must keep the same id — return ids from the current memory only; do not invent new ids for updates.
3. DELETE: If a retrieved fact contradicts an existing memory element (a decision was reversed, an owner changed away, a number is now wrong) and the new fact itself carries the replacement, DELETE the stale element (the replacement arrives via ADD/UPDATE). Use the existing id.
4. NONE: If the fact is already present or carries nothing worth remembering.

Below is the current content of my memory:
```
{current}```

The new retrieved facts are mentioned in the triple backticks:
```
{facts}
```

Return the decisions in JSON only, one row per decision, in this exact format:
{{"memory":[{{"id":"<id>","text":"<content>","event":"<ADD|UPDATE|DELETE|NONE>","old_memory":"<old content, only for UPDATE>"}}]}}

- For UPDATE and DELETE, ids MUST come from the current memory shown above.
- Include a row for every new fact (event ADD/UPDATE/DELETE/NONE).
- Do not return anything except the JSON format."#
    )
}

/// Parse the agent's decision JSON. Tolerates one enclosing code fence,
/// `<think>` spans, and prose around the JSON object (mem0's
/// `remove_code_blocks` + `extract_json` fallbacks).
fn parse_update_ops(raw: &str) -> Result<Vec<UpdateOp>> {
    #[derive(serde::Deserialize)]
    struct RawOp {
        event: String,
        id: Option<serde_json::Value>,
        text: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct RawOps {
        memory: Vec<RawOp>,
    }

    let mut cleaned = raw.trim().to_string();
    while let (Some(start), Some(end)) = (cleaned.find("<think>"), cleaned.find("</think>")) {
        if end < start {
            break;
        }
        cleaned.replace_range(start..end + "</think>".len(), "");
    }
    let cleaned = cleaned.trim();
    let body = match (cleaned.find('{'), cleaned.rfind('}')) {
        (Some(start), Some(end)) if end > start => &cleaned[start..=end],
        _ => anyhow::bail!("no JSON object in agent response"),
    };
    let parsed: RawOps = serde_json::from_str(body).context("agent memory ops not valid JSON")?;
    Ok(parsed
        .memory
        .into_iter()
        .map(|op| UpdateOp {
            event: op.event,
            display_id: op.id.and_then(|v| match v {
                serde_json::Value::String(s) => s.trim().parse::<usize>().ok(),
                serde_json::Value::Number(n) => n.as_u64().map(|n| n as usize),
                _ => None,
            }),
            text: op.text,
        })
        .collect())
}

/// Case/whitespace-insensitive text equality (embedding-reuse check).
fn normalized_eq(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    norm(a) == norm(b)
}

/// Resolve the embedding-model dir: env override, else
/// `<data_dir>/models/bge-small-en`.
fn resolve_model_dir(paths: &AppPaths) -> PathBuf {
    if let Ok(dir) = std::env::var("BLUEY_EMBED_MODEL_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    paths.data_dir.join("models").join("bge-small-en")
}

/// Stream one file to `dest` (`.part` + rename so an interrupted download never
/// leaves a half-written model file). Mirrors the STT model_setup pattern.
async fn download_file(url: &str, dest: &Path) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    debug!(%url, "downloading embedding model file");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .context("build embed-model HTTP client")?;
    let mut resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("download {url} returned HTTP {}", resp.status());
    }
    let tmp = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .with_context(|| format!("create {}", tmp.display()))?;
    let mut written: u64 = 0;
    while let Some(chunk) = resp
        .chunk()
        .await
        .with_context(|| format!("stream error during {url}"))?
    {
        file.write_all(&chunk)
            .await
            .with_context(|| format!("write error to {}", tmp.display()))?;
        written += chunk.len() as u64;
    }
    file.flush().await.ok();
    drop(file);
    if written < 1_024 {
        let _ = tokio::fs::remove_file(&tmp).await;
        anyhow::bail!("download {url} produced only {written} bytes (likely an error page)");
    }
    tokio::fs::rename(&tmp, dest)
        .await
        .with_context(|| format!("finalize {}", dest.display()))?;
    info!(dest = %dest.display(), bytes = written, "embedding model file ready");
    Ok(())
}

/// Render cross-meeting hits as one bounded context block, best-ranked first.
/// The floor applies to the SEMANTIC score (hybrid-combined `score` values
/// are normalized by the active-signal divisor and not comparable to a cosine
/// floor); ordering follows the hits' hybrid ranking. Empty string when no
/// hit clears the floor — noise must not pollute the answer context.
pub fn render_hits(hits: &[FactHit], min_semantic: f32, max_chars: usize) -> String {
    let mut block = String::new();
    for hit in hits.iter().filter(|h| h.semantic >= min_semantic) {
        let line = format!("- {}\n", hit.text.trim());
        if block.len() + line.len() > max_chars {
            break;
        }
        block.push_str(&line);
    }
    block.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, text: &str) -> FactRow {
        FactRow {
            id,
            text: text.to_string(),
            score: 0.8,
        }
    }

    #[test]
    fn update_prompt_carries_display_ids_facts_and_contract() {
        let existing = vec![
            row(41, "checkout sla is 200ms"),
            row(7, "auth owner is dana"),
        ];
        let candidates = vec![Candidate {
            text: "checkout sla moved to 300ms".to_string(),
            embedding: vec![0.0; 3],
        }];
        let prompt = build_update_prompt(&existing, &candidates);
        // Display ids are positions, never store row ids.
        assert!(prompt.contains(r#""id":"0""#) && prompt.contains(r#""id":"1""#));
        assert!(
            !prompt.contains("41"),
            "store ids must never reach the agent"
        );
        assert!(prompt.contains("checkout sla moved to 300ms"));
        assert!(prompt.contains("Do not return anything except the JSON format."));
    }

    #[test]
    fn parse_ops_tolerates_fences_think_spans_and_prose() {
        let raw = r#"<think>the sla changed, so update row 0</think>
Sure — here are the decisions:
```json
{"memory":[
  {"id":"0","text":"checkout sla moved to 300ms","event":"UPDATE","old_memory":"checkout sla is 200ms"},
  {"id":"2","text":"retro moved to fridays","event":"ADD"},
  {"id":1,"event":"NONE"}
]}
```"#;
        let ops = parse_update_ops(raw).expect("parse");
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[0].event, "UPDATE");
        assert_eq!(ops[0].display_id, Some(0));
        assert_eq!(ops[1].event, "ADD");
        assert_eq!(ops[2].display_id, Some(1), "bare-integer ids accepted");
        assert!(parse_update_ops("no json here at all").is_err());
    }

    fn hit(text: &str, score: f32) -> FactHit {
        FactHit {
            text: text.to_string(),
            meeting_id: "m".to_string(),
            score,
            semantic: score,
            created_at_ms: 0,
        }
    }

    #[test]
    fn render_hits_filters_by_score_and_bounds_size() {
        let hits = vec![
            hit("shard by tenant id", 0.82),
            hit("checkout sla 200ms", 0.71),
            hit("irrelevant noise", 0.20),
        ];
        let block = render_hits(&hits, 0.5, 200);
        assert!(block.contains("shard by tenant"));
        assert!(block.contains("checkout sla"));
        assert!(!block.contains("noise"), "low-score hits must be dropped");

        let bounded = render_hits(&hits, 0.0, 25);
        assert!(bounded.len() <= 25);
    }
}
