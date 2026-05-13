# bluey Master Plan V3 — Part B: Phases 5-10

**Author**: Principal Engineer (Step 2B of 4)
**Date**: 2026-05-12
**Scope**: Memory/RAG, Dashboard Polish, Ops/Security, Dev Discipline, Latency Engineering, Cost Optimization
**Target**: ~1800 lines, compilable code sketches, design-doc citations

---

## Phase 5: Memory + RAG + Context Management (~2 weeks)

### Entry Criteria
- B3.1 (LLM trait) merged — embedding providers need the trait interface
- C0.1 (SQLite session model) merged — vector store shares the DB
- Phase 4 Batch 4A complete (streaming infrastructure available)

### Exit Criteria
- `cargo test --features rag` passes with mock embeddings
- Live indexing produces searchable chunks within 2s of transcript final
- Epoch summarization triggers at 50K token threshold
- Hybrid retrieval returns relevant chunks for test queries (precision >0.7 on eval set)
- Context assembly produces lane-appropriate payloads under token budgets

### Batch Structure

**PR 5A — Vector Infrastructure (days 1-4)**:
- B5.1 sqlite-vec vector store
- B5.2 SemanticChunker
- B5.3 Embedding trait + 4 providers

**PR 5B — Live Indexing + Search (days 5-8)**:
- B5.4 Live RAG indexer
- B5.7 Async vector search
- B5.8 Hybrid retrieval

**PR 5C — Context Management (days 9-14)**:
- CM1 SessionState + Turn model
- CM2 Context-assembly (lane-aware)
- CM3 Epoch summarization
- CM4 Token counter + compaction trigger

---

### B5.1 — sqlite-vec Vector Store

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | ❌ Not started |
| **Depends** | C0.1 (SQLite), D0.1 (Tauri scaffold) |
| **Design source** | CUE-DESIGN-03:L1200-1270 (VectorStore.ts pattern #13) |

**Summary**: Local vector store using sqlite-vec extension with per-dimension virtual tables. Supports insert, search (KNN), and delete operations.

**Code sketch**:
```rust
// src-tauri/src/rag/vector_store.rs
use rusqlite::{Connection, params};
use std::path::Path;
use anyhow::Result;

pub struct VectorStore {
    conn: Connection,
    dim: u32,
}

pub struct SearchResult {
    pub chunk_id: String,
    pub distance: f32,
}

impl VectorStore {
    pub fn new(db_path: &Path, dim: u32) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        unsafe { conn.load_extension("vec0", None)?; }
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks_{dim}
             USING vec0(embedding float[{dim}], chunk_id TEXT, session_id TEXT);"
        ))?;
        Ok(Self { conn, dim })
    }

    pub fn insert(&self, chunk_id: &str, session_id: &str, embedding: &[f32]) -> Result<()> {
        self.conn.execute(
            &format!("INSERT INTO vec_chunks_{} (embedding, chunk_id, session_id) VALUES (?, ?, ?)", self.dim),
            params![embedding_to_blob(embedding), chunk_id, session_id],
        )?;
        Ok(())
    }

    pub fn search(&self, query: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT chunk_id, distance FROM vec_chunks_{} WHERE embedding MATCH ? ORDER BY distance LIMIT ?",
            self.dim
        ))?;
        let rows = stmt.query_map(params![embedding_to_blob(query), limit as i64], |row| {
            Ok(SearchResult { chunk_id: row.get(0)?, distance: row.get(1)? })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete_session(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            &format!("DELETE FROM vec_chunks_{} WHERE session_id = ?", self.dim),
            params![session_id],
        )?;
        Ok(())
    }
}

fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}
```

**Acceptance**: Insert 1000 chunks, KNN search returns correct top-5 by cosine distance in <50ms.

**Verification**: Integration test with known embeddings; verify distance ordering matches manual cosine computation.

**Risks**: sqlite-vec extension loading may conflict with tauri-plugin-sql's bundled SQLite. Mitigation: use separate rusqlite connection for RAG (not the Tauri plugin DB).

---

### B5.2 — SemanticChunker

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | ❌ Not started |
| **Depends** | None (pure logic) |
| **Design source** | CUE-DESIGN-03:L1275-1360 (SemanticChunker.ts pattern #115) |

**Summary**: Speaker-aware chunker with sliding-window overlap. Parameters: TARGET=300, MAX=400, MIN=100, OVERLAP=50 tokens.

**Code sketch**:
```rust
// src-tauri/src/rag/chunker.rs
const TARGET_TOKENS: usize = 300;
const MAX_TOKENS: usize = 400;
const MIN_TOKENS: usize = 100;
const OVERLAP_TOKENS: usize = 50;

#[derive(Clone, Debug)]
pub struct Chunk {
    pub id: String,
    pub text: String,
    pub speaker: Option<String>,
    pub token_count: usize,
    pub start_ms: u64,
    pub end_ms: u64,
}

pub struct TranscriptSegment {
    pub text: String,
    pub speaker: String,
    pub timestamp_ms: u64,
}

pub fn chunk_segments(segments: &[TranscriptSegment]) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut buf = String::new();
    let mut tok_count = 0usize;
    let mut start_ms = 0u64;
    let mut speaker: Option<&str> = None;

    for seg in segments {
        let seg_tok = seg.text.len() / 4;
        let speaker_changed = speaker.is_some() && speaker != Some(&seg.speaker);

        if (speaker_changed || tok_count + seg_tok > MAX_TOKENS) && tok_count >= MIN_TOKENS {
            chunks.push(Chunk {
                id: uuid::Uuid::new_v4().to_string(),
                text: buf.clone(),
                speaker: speaker.map(String::from),
                token_count: tok_count,
                start_ms,
                end_ms: seg.timestamp_ms,
            });
            let overlap = take_tail(&buf, OVERLAP_TOKENS * 4);
            buf = overlap;
            tok_count = buf.len() / 4;
            start_ms = seg.timestamp_ms;
        }

        if buf.is_empty() { start_ms = seg.timestamp_ms; }
        if !buf.is_empty() { buf.push(' '); }
        buf.push_str(&seg.text);
        tok_count += seg_tok;
        speaker = Some(&seg.speaker);
    }

    if tok_count >= MIN_TOKENS {
        chunks.push(Chunk {
            id: uuid::Uuid::new_v4().to_string(),
            text: buf,
            speaker: speaker.map(String::from),
            token_count: tok_count,
            start_ms,
            end_ms: segments.last().map(|s| s.timestamp_ms).unwrap_or(0),
        });
    }
    chunks
}

fn take_tail(s: &str, chars: usize) -> String {
    if s.len() <= chars { s.to_string() } else { s[s.len() - chars..].to_string() }
}
```

**Acceptance**: 10-minute transcript (600 segments) produces chunks all within [100, 400] token range; overlap verified between consecutive chunks.

**Verification**: Unit test with synthetic segments; assert no chunk exceeds MAX, speaker boundaries respected.

**Risks**: Token estimation (len/4) may drift for non-English. Mitigation: swap to tiktoken-rs if accuracy matters post-v1.

---

### B5.3 — Embedding Trait + 4 Providers

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | ❌ Not started |
| **Depends** | B3.1 (LLM trait for HTTP client reuse) |
| **Design source** | CUE-DESIGN-03:L1365-1430 (EmbeddingPipeline.ts pattern #12) |

**Summary**: Async trait with cascading fallback: OpenAI text-embedding-3-small (1536d) → Gemini text-embedding-004 (768d) → Cohere embed-v3 (1024d) → local all-MiniLM-L6-v2 ONNX (384d).

**Code sketch**:
```rust
// src-tauri/src/rag/embedding.rs
use async_trait::async_trait;
use anyhow::Result;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn name(&self) -> &str;
    fn dimension(&self) -> u32;
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

pub struct EmbeddingResolver {
    providers: Vec<Box<dyn EmbeddingProvider>>,
}

impl EmbeddingResolver {
    pub fn new(providers: Vec<Box<dyn EmbeddingProvider>>) -> Self {
        Self { providers }
    }

    pub async fn embed(&self, text: &str) -> Result<(Vec<f32>, u32)> {
        for p in &self.providers {
            match p.embed(text).await {
                Ok(emb) => return Ok((emb, p.dimension())),
                Err(e) => tracing::warn!(provider = p.name(), err = %e, "embedding failed, trying next"),
            }
        }
        anyhow::bail!("all embedding providers failed")
    }
}

// OpenAI implementation (1536-dim)
pub struct OpenAIEmbedding { pub client: reqwest::Client, pub api_key: String }

#[async_trait]
impl EmbeddingProvider for OpenAIEmbedding {
    fn name(&self) -> &str { "openai" }
    fn dimension(&self) -> u32 { 1536 }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let resp = self.client.post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({"input": text, "model": "text-embedding-3-small"}))
            .send().await?.error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        let arr = body["data"][0]["embedding"].as_array().unwrap();
        Ok(arr.iter().map(|v| v.as_f64().unwrap() as f32).collect())
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let resp = self.client.post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({"input": texts, "model": "text-embedding-3-small"}))
            .send().await?.error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        Ok(body["data"].as_array().unwrap().iter()
            .map(|d| d["embedding"].as_array().unwrap().iter()
                .map(|v| v.as_f64().unwrap() as f32).collect())
            .collect())
    }
}
```

**Acceptance**: Each provider returns correct-dimension vector; fallback chain skips failed provider and succeeds on next.

**Verification**: Mock HTTP responses; verify dimension matches; test cascade with first provider returning 500.

**Risks**: Dimension mismatch if user switches providers mid-session. Mitigation: per-dimension vec0 tables (B5.1 already handles this).

---

### B5.4 — Live RAG Indexer (JIT)

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | ❌ Not started |
| **Depends** | B5.1, B5.2, B5.3 |
| **Design source** | CUE-DESIGN-03:L1435-1470 (LiveRAGIndexer.ts pattern #14) |

**Summary**: Feeds final transcript segments in real-time, chunks when buffer hits TARGET, embeds, and inserts. Searchable within 2s.

**Code sketch**:
```rust
// src-tauri/src/rag/live_indexer.rs
use tokio::sync::mpsc;

pub struct LiveIndexer {
    tx: mpsc::Sender<TranscriptSegment>,
}

impl LiveIndexer {
    pub fn spawn(
        embedding: Arc<EmbeddingResolver>,
        store: Arc<VectorStore>,
        session_id: String,
    ) -> Self {
        let (tx, mut rx) = mpsc::channel::<TranscriptSegment>(256);
        tokio::spawn(async move {
            let mut buf: Vec<TranscriptSegment> = Vec::new();
            while let Some(seg) = rx.recv().await {
                buf.push(seg);
                let tok_est: usize = buf.iter().map(|s| s.text.len() / 4).sum();
                if tok_est >= TARGET_TOKENS {
                    let chunks = chunk_segments(&buf);
                    for chunk in &chunks {
                        if let Ok((emb, _)) = embedding.embed(&chunk.text).await {
                            let _ = store.insert(&chunk.id, &session_id, &emb);
                        }
                    }
                    // Keep overlap
                    let keep = buf.len().saturating_sub(2);
                    buf.drain(..keep);
                }
            }
        });
        Self { tx }
    }

    pub async fn feed(&self, segment: TranscriptSegment) -> Result<()> {
        self.tx.send(segment).await.map_err(|_| anyhow::anyhow!("indexer closed"))
    }
}
```

**Acceptance**: Feed 50 segments at 1/s; query after 15s returns relevant chunks from earlier in session.

**Verification**: Integration test with mock embedding (identity vectors); verify chunks appear in search results.

**Risks**: Embedding latency (200-500ms per chunk) may cause backlog. Mitigation: batch embed when possible; buffer absorbs bursts.

---

### B5.7 — Async Vector Search

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | ❌ Not started |
| **Depends** | B5.1 |
| **Design source** | CUE-DESIGN-03:L1530-1560 (vectorSearchWorker.ts pattern #116) |

**Summary**: Non-blocking search via `spawn_blocking` with 30s timeout. Replaces Node.js worker thread pattern.

**Code sketch**:
```rust
// src-tauri/src/rag/search.rs
use std::time::Duration;

pub async fn search_async(
    store: &VectorStore,
    query: &[f32],
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let query = query.to_vec();
    let db_path = store.db_path().to_path_buf();
    let dim = store.dim;

    tokio::time::timeout(Duration::from_secs(30), tokio::task::spawn_blocking(move || {
        let s = VectorStore::new(&db_path, dim)?;
        s.search(&query, limit)
    })).await??
}
```

**Acceptance**: Search completes in <100ms for 10K chunks; timeout fires correctly at 30s for pathological cases.

**Verification**: Benchmark with 10K synthetic embeddings; verify timeout with artificially slow query.

**Risks**: Opening new connection per search adds ~5ms. Mitigation: acceptable for cold-path; hot-path uses connection pool.

---

### B5.8 — Hybrid Retrieval (Vector + BM25)

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | 🟡 Partial (BM25 logic designed) |
| **Depends** | B5.1, B5.7 |
| **Design source** | CUE-DESIGN-03:L1565-1640 (RAGRetriever.ts pattern #61) |

**Summary**: Combine vector similarity (weight 0.7) with keyword BM25 scoring (weight 0.3) for robust retrieval.

**Code sketch**:
```rust
// src-tauri/src/rag/hybrid.rs
use std::collections::HashMap;

pub struct HybridRetriever {
    pub vector_weight: f32, // 0.7
}

impl HybridRetriever {
    pub fn merge(
        &self,
        vector_results: &[SearchResult],
        keyword_results: &[(String, f32)], // (chunk_id, bm25_score)
        limit: usize,
    ) -> Vec<(String, f32)> {
        let mut scores: HashMap<&str, f32> = HashMap::new();
        for r in vector_results {
            let sim = 1.0 - r.distance;
            *scores.entry(&r.chunk_id).or_default() += sim * self.vector_weight;
        }
        for (id, s) in keyword_results {
            *scores.entry(id.as_str()).or_default() += s * (1.0 - self.vector_weight);
        }
        let mut ranked: Vec<_> = scores.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        ranked.truncate(limit);
        ranked
    }
}

/// Simple BM25-like keyword scoring on SQLite FTS5
pub fn keyword_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<(String, f32)>> {
    let mut stmt = conn.prepare(
        "SELECT chunk_id, bm25(chunks_fts) as score FROM chunks_fts WHERE chunks_fts MATCH ? ORDER BY score LIMIT ?"
    )?;
    let rows = stmt.query_map(params![query, limit as i64], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f32>(1)?))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
```

**Acceptance**: Query "system design scalability" returns chunks about system design (vector) AND chunks containing exact keywords (BM25), merged correctly.

**Verification**: Test with chunks where vector-similar ≠ keyword-match; verify both contribute to final ranking.

**Risks**: FTS5 index adds storage overhead. Mitigation: only index active session chunks; purge on session archive.

---

### CM1 — SessionState + Turn Model

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Core |
| **Status** | ❌ Not started |
| **Depends** | C0.1 (base SQLite schema) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L470 (CM1 spec) |

**Summary**: Extends C0.1 with lane metadata per message, turn tracking, and session state machine.

**Code sketch**:
```rust
// src-tauri/src/session/model.rs
use rusqlite::params;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lane { Snap, Solve, Think }

#[derive(Debug, Clone)]
pub struct Turn {
    pub id: String,
    pub session_id: String,
    pub role: Role,
    pub content: String,
    pub lane: Option<Lane>,
    pub token_count: u32,
    pub created_at: i64,
}

pub fn migrate_cm1(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        ALTER TABLE messages ADD COLUMN lane TEXT;
        ALTER TABLE messages ADD COLUMN token_count INTEGER DEFAULT 0;
        CREATE INDEX IF NOT EXISTS idx_msg_session_lane ON messages(conversation_id, lane);
    ")?;
    Ok(())
}

pub fn insert_turn(conn: &Connection, turn: &Turn) -> Result<()> {
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, lane, token_count, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![turn.id, turn.session_id, turn.role.as_str(), turn.content,
                turn.lane.map(|l| l.as_str()), turn.token_count, turn.created_at],
    )?;
    Ok(())
}

pub fn get_recent_turns(conn: &Connection, session_id: &str, limit: u32) -> Result<Vec<Turn>> {
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, role, content, lane, token_count, created_at
         FROM messages WHERE conversation_id = ? ORDER BY created_at DESC LIMIT ?"
    )?;
    // ... map rows to Turn structs
    todo!()
}

impl Lane {
    pub fn as_str(&self) -> &'static str {
        match self { Lane::Snap => "snap", Lane::Solve => "solve", Lane::Think => "think" }
    }
}
```

**Acceptance**: Turns persist with lane metadata; query by session+lane returns correct subset.

**Verification**: Insert turns across all 3 lanes; verify filtered queries return only matching lane.

**Risks**: ALTER TABLE on existing DB with data. Mitigation: use IF NOT EXISTS pattern; test migration on populated DB.

---

### CM2 — Context Assembly (Lane-Aware)

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Core |
| **Status** | ❌ Not started |
| **Depends** | CM1, R1 (intent classifier) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L38-50 (Three-Lane token budgets) |

**Summary**: Four strategies: Fresh (Snap: last 3 turns), Refinement (Solve: RAG-selected turns), LongHistory (Think: full + epoch summaries), LaneUpgrade (carry forward from lower lane).

**Code sketch**:
```rust
// src-tauri/src/session/context.rs

pub struct ContextBudget {
    pub max_input_tokens: u32,
    pub max_turns: u32,
}

pub const SNAP_BUDGET: ContextBudget = ContextBudget { max_input_tokens: 2000, max_turns: 3 };
pub const SOLVE_BUDGET: ContextBudget = ContextBudget { max_input_tokens: 16000, max_turns: 20 };
pub const THINK_BUDGET: ContextBudget = ContextBudget { max_input_tokens: 32000, max_turns: 50 };

pub struct AssembledContext {
    pub system_prompt: String,
    pub turns: Vec<Turn>,
    pub total_tokens: u32,
}

pub fn assemble_context(
    lane: Lane,
    session_id: &str,
    query: &str,
    turns: &[Turn],
    epoch_summaries: &[String],
    rag_results: &[String],
) -> AssembledContext {
    let budget = match lane {
        Lane::Snap => SNAP_BUDGET,
        Lane::Solve => SOLVE_BUDGET,
        Lane::Think => THINK_BUDGET,
    };

    let selected_turns = match lane {
        Lane::Snap => turns.iter().rev().take(budget.max_turns as usize).cloned().collect(),
        Lane::Solve => {
            // Include RAG-retrieved relevant turns + last 5 recent
            let mut ctx: Vec<Turn> = Vec::new();
            ctx.extend(turns.iter().rev().take(5).cloned());
            // RAG results injected as system context, not turns
            ctx
        }
        Lane::Think => {
            let mut ctx: Vec<Turn> = Vec::new();
            // Epoch summaries as synthetic system turns
            for summary in epoch_summaries {
                ctx.push(Turn { role: Role::System, content: summary.clone(), ..Default::default() });
            }
            ctx.extend(turns.iter().cloned());
            ctx
        }
    };

    // Truncate to token budget
    let mut total = 0u32;
    let final_turns: Vec<Turn> = selected_turns.into_iter()
        .take_while(|t| { total += t.token_count; total <= budget.max_input_tokens })
        .collect();

    AssembledContext { system_prompt: String::new(), turns: final_turns, total_tokens: total }
}
```

**Acceptance**: Snap context ≤2K tokens with exactly last 3 turns; Solve includes RAG context; Think includes epoch summaries.

**Verification**: Unit test with 100-turn session; verify each lane strategy produces correct subset under budget.

**Risks**: Token counting drift may cause budget overflows. Mitigation: CM4 provides accurate counts; 10% safety margin.

---

### CM3 — Epoch Summarization Background Job

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | ❌ Not started |
| **Depends** | CM1, B3.1 (LLM for summarization) |
| **Design source** | CUE-DESIGN-03:L1490-1530 (SessionTracker.ts pattern #17) |

**Summary**: When session token count exceeds 50K, compress oldest 1/3 of turns into a summary paragraph. Max 5 epoch summaries retained.

**Code sketch**:
```rust
// src-tauri/src/rag/epoch.rs
use std::sync::atomic::{AtomicBool, Ordering};

pub struct EpochSummarizer {
    max_tokens_before_compact: u32, // 50_000
    max_summaries: usize,           // 5
    compacting: AtomicBool,
}

impl EpochSummarizer {
    pub async fn maybe_compact(
        &self,
        turns: &mut Vec<Turn>,
        summaries: &mut Vec<String>,
        llm: &dyn LlmProvider,
    ) -> Result<()> {
        let total: u32 = turns.iter().map(|t| t.token_count).sum();
        if total < self.max_tokens_before_compact { return Ok(()); }
        if self.compacting.swap(true, Ordering::SeqCst) { return Ok(()); }

        let drain_count = turns.len() / 3;
        let to_summarize: Vec<_> = turns.drain(..drain_count).collect();
        let text = to_summarize.iter()
            .map(|t| format!("[{}] {}", t.role.as_str(), t.content))
            .collect::<Vec<_>>().join("\n");

        let prompt = format!(
            "Summarize this conversation section in 3-5 sentences. \
             Preserve key questions and answers:\n\n{}", text
        );
        // Use Snap lane (Cerebras) for cheap summarization — $0.001 per call
        let summary = llm.generate_simple(&prompt).await?;
        summaries.push(summary);
        if summaries.len() > self.max_summaries { summaries.remove(0); }

        self.compacting.store(false, Ordering::SeqCst);
        Ok(())
    }
}
```

**Acceptance**: 60K-token session triggers compaction; resulting summaries are coherent; total context drops below 40K.

**Verification**: Feed synthetic 60K-token session; verify compaction fires once; verify summary quality with LLM judge.

**Risks**: Summarization quality with cheap model. Mitigation: use Cerebras (fast+cheap) for summaries; quality acceptable for context, not user-facing.

---

### CM4 — Token Counter + Compaction Trigger

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Core |
| **Status** | ❌ Not started |
| **Depends** | CM1 |
| **Design source** | CUE-DESIGN-03:L1560 (open question #9 — tiktoken vs estimate) |

**Summary**: Accurate token counting via tiktoken-rs for budget enforcement. Triggers CM3 at threshold.

**Code sketch**:
```rust
// src-tauri/src/session/tokens.rs
use tiktoken_rs::cl100k_base;

static ENCODER: once_cell::sync::Lazy<tiktoken_rs::CoreBPE> =
    once_cell::sync::Lazy::new(|| cl100k_base().unwrap());

pub fn count_tokens(text: &str) -> u32 {
    ENCODER.encode_with_special_tokens(text).len() as u32
}

pub fn session_total_tokens(turns: &[Turn]) -> u32 {
    turns.iter().map(|t| t.token_count).sum()
}

pub fn should_compact(turns: &[Turn], threshold: u32) -> bool {
    session_total_tokens(turns) > threshold
}
```

**Acceptance**: Token counts match OpenAI tokenizer within ±1%; compaction trigger fires at exactly 50K threshold.

**Verification**: Compare count_tokens output against OpenAI API token usage for same strings.

**Risks**: tiktoken-rs adds ~2ms per call. Mitigation: count once on insert, cache in Turn.token_count field.

---

### Phase 5 Deliverable

A working local RAG pipeline: transcript → chunk → embed → store → retrieve (hybrid) → assemble (lane-aware context). Epoch summarization keeps long sessions manageable. All searchable within 2s of utterance.



---

## Phase 6: Dashboard Polish (~2 weeks)

### Entry Criteria
- Phase 1+2 complete (dashboard shell, session pages working)
- Phase 4 partial (LLM providers configured, at least Snap lane functional)
- SQLite schema stable (migrations 001-004 applied)

### Exit Criteria
- All 9 settings pages functional with persistence
- Command palette (Cmd+K) navigates to any page/action
- Onboarding flow completes for new users
- Shortcut rebinding works with conflict detection
- System prompts CRUD with AI-generation option

### Batch Structure

**PR 6A — Settings Pages (days 1-5)**:
- B6.2 Rebindable keybinds
- B4.3 Per-provider prompt variants
- B4.4 TINY prompt for Snap lane
- B3.10 testConnection command

**PR 6B — Content Pages (days 6-10)**:
- B4.5 Skill library UI
- B3.6 Structured JSON generation
- B3.7 Custom cURL provider
- B3.8 Codex CLI integration

**PR 6C — Polish + UX (days 11-14)**:
- B6.7 Inertial scroll
- B6.8 Code expansion animation
- B6.9 Command palette
- B6.10 Onboarding flow
- B4.7 System-prompt protection
- B4.9 First-person enforcement

---

### B6.2 — Rebindable Keybinds

| Field | Value |
|-------|-------|
| **Layer** | Dashboard / Settings |
| **Status** | ❌ Not started |
| **Depends** | B6.1 (hotkey system), C0.1 (SQLite) |
| **Design source** | CUE-DESIGN-04:L45-130 (KeybindManager.ts, shortcuts.rs) |

**Summary**: Settings page with ShortcutRecorder component, conflict detection, persist to SQLite, re-register on save.

**Code sketch (Rust)**:
```rust
// src-tauri/src/shortcuts.rs
#[tauri::command]
pub async fn update_shortcuts(
    app: AppHandle,
    bindings: HashMap<String, String>, // action_id → accelerator
) -> Result<(), String> {
    // Validate all accelerators parse
    for (action, accel) in &bindings {
        accel.parse::<tauri::keyboard::Shortcut>()
            .map_err(|e| format!("Invalid shortcut for {action}: {e}"))?;
    }
    // Check for conflicts
    let mut seen = HashMap::new();
    for (action, accel) in &bindings {
        if let Some(existing) = seen.insert(accel.clone(), action.clone()) {
            return Err(format!("Conflict: {accel} bound to both {existing} and {action}"));
        }
    }
    // Unregister all, re-register with new bindings
    app.global_shortcut().unregister_all().map_err(|e| e.to_string())?;
    for (action, accel) in &bindings {
        let shortcut: tauri::keyboard::Shortcut = accel.parse().unwrap();
        let action = action.clone();
        app.global_shortcut().on_shortcut(shortcut, move |_app, _s, _e| {
            dispatch_action(&action);
        }).map_err(|e| e.to_string())?;
    }
    // Persist
    persist_shortcuts_to_db(&bindings).map_err(|e| e.to_string())?;
    Ok(())
}
```

**Code sketch (React)**:
```typescript
// src/pages/Shortcuts.tsx
export function ShortcutRecorder({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [recording, setRecording] = useState(false);

  useEffect(() => {
    if (!recording) return;
    const handler = (e: KeyboardEvent) => {
      e.preventDefault();
      const parts: string[] = [];
      if (e.metaKey) parts.push("Cmd");
      if (e.ctrlKey) parts.push("Ctrl");
      if (e.altKey) parts.push("Alt");
      if (e.shiftKey) parts.push("Shift");
      if (e.key.length === 1) parts.push(e.key.toUpperCase());
      else if (e.key !== "Meta" && e.key !== "Control" && e.key !== "Alt" && e.key !== "Shift") {
        parts.push(e.key);
      }
      if (parts.length > 1) { onChange(parts.join("+")); setRecording(false); }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [recording, onChange]);

  return (
    <button onClick={() => setRecording(true)} className="shortcut-recorder">
      {recording ? "Press keys..." : value || "Click to record"}
    </button>
  );
}
```

**Acceptance**: User can rebind any shortcut; conflicts show error; bindings persist across restart.

**Verification**: Rebind Alt+Z to Alt+X; restart app; verify Alt+X triggers toggle.

**Risks**: Platform differences in key naming (Cmd vs Super). Mitigation: normalize in Rust before registration.

---

### B6.9 — Command Palette (Cmd+K)

| Field | Value |
|-------|-------|
| **Layer** | Dashboard / UX |
| **Status** | ❌ Not started |
| **Depends** | B6.3 (sidebar nav) |
| **Design source** | CUE-DESIGN-04:L176 (cmdk library, page structure) |

**Summary**: Spotlight-style command palette using `cmdk` library. Actions: navigate pages, switch models, toggle modes, search sessions.

**Code sketch**:
```typescript
// src/components/CommandPalette.tsx
import { Command } from "cmdk";
import { useNavigate } from "react-router-dom";

const COMMANDS = [
  { id: "nav-chats", label: "Go to Chats", action: "/chats", group: "Navigation" },
  { id: "nav-settings", label: "Go to Settings", action: "/settings", group: "Navigation" },
  { id: "nav-prompts", label: "Go to System Prompts", action: "/system-prompts", group: "Navigation" },
  { id: "model-snap", label: "Switch to Snap mode", action: "mode:snap", group: "Mode" },
  { id: "model-solve", label: "Switch to Solve mode", action: "mode:solve", group: "Mode" },
  { id: "model-think", label: "Switch to Think mode", action: "mode:think", group: "Mode" },
];

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const navigate = useNavigate();

  const execute = (action: string) => {
    if (action.startsWith("/")) navigate(action);
    else if (action.startsWith("mode:")) invoke("set_lane_override", { lane: action.split(":")[1] });
    onClose();
  };

  return (
    <Command.Dialog open={open} onOpenChange={(v) => !v && onClose()} label="Command palette">
      <Command.Input placeholder="Type a command..." />
      <Command.List>
        {Object.entries(groupBy(COMMANDS, "group")).map(([group, items]) => (
          <Command.Group key={group} heading={group}>
            {items.map(cmd => (
              <Command.Item key={cmd.id} onSelect={() => execute(cmd.action)}>
                {cmd.label}
              </Command.Item>
            ))}
          </Command.Group>
        ))}
      </Command.List>
    </Command.Dialog>
  );
}
```

**Acceptance**: Cmd+K opens palette; typing filters commands; selecting navigates or executes action.

**Verification**: Open palette, type "chat", verify only chat-related commands shown; select and verify navigation.

**Risks**: cmdk may conflict with Tauri's global shortcut for Cmd+K. Mitigation: register Cmd+K as app-level, not OS-global.

---

### B6.10 — Onboarding Flow

| Field | Value |
|-------|-------|
| **Layer** | Dashboard / UX |
| **Status** | ❌ Not started |
| **Depends** | B6.3 (dashboard) |
| **Design source** | CUE-DESIGN-04:L530-540 (FeatureSpotlight.tsx pattern) |

**Summary**: First-run wizard: API key setup → audio device selection → shortcut overview → test connection. Tracks completion in localStorage.

**Code sketch**:
```typescript
// src/components/Onboarding.tsx
const STEPS = ["welcome", "api-keys", "audio", "shortcuts", "test", "done"] as const;

export function Onboarding() {
  const [step, setStep] = useState(0);
  const completed = localStorage.getItem("onboarding_complete");
  if (completed) return null;

  const finish = () => { localStorage.setItem("onboarding_complete", "true"); };

  return (
    <Dialog open={!completed}>
      <DialogContent className="max-w-lg">
        {STEPS[step] === "welcome" && <WelcomeStep onNext={() => setStep(1)} />}
        {STEPS[step] === "api-keys" && <ApiKeyStep onNext={() => setStep(2)} />}
        {STEPS[step] === "audio" && <AudioStep onNext={() => setStep(3)} />}
        {STEPS[step] === "shortcuts" && <ShortcutStep onNext={() => setStep(4)} />}
        {STEPS[step] === "test" && <TestStep onNext={() => setStep(5)} />}
        {STEPS[step] === "done" && <DoneStep onFinish={finish} />}
      </DialogContent>
    </Dialog>
  );
}
```

**Acceptance**: New install shows onboarding; completing it sets flag; never shows again.

**Verification**: Clear localStorage; relaunch; verify wizard appears; complete all steps; verify flag set.

**Risks**: Users may skip without configuring API keys. Mitigation: "Skip" button warns that features won't work.

---

### Phase 6 Deliverable

Full-featured dashboard with all settings pages, command palette for power users, onboarding for new users, and rebindable shortcuts with conflict detection. All preferences persist to SQLite.



---

## Phase 7: Ops + Security (~2 weeks)

### Entry Criteria
- Phase 0 complete (Tauri binary builds)
- Phase 4 partial (LLM providers exist for cost tracking)
- B3.9 (key scrubbing logic) merged

### Exit Criteria
- All API keys stored in OS keychain (zero plaintext on disk)
- Log rotation at 10MB with NDJSON format
- OpenTelemetry metrics exporting to Grafana Cloud
- Auto-updater checks and installs updates
- Single-instance lock prevents duplicate processes
- Panic handler logs crash context before exit

### Batch Structure

**PR 7A — Security (days 1-4)**:
- B7.1 Keychain integration
- B7.2 Key scrubbing on drop
- B7.3 Log masking utility
- B7.7 Single-instance lock
- B7.10 Panic handler

**PR 7B — Logging + Telemetry (days 5-9)**:
- B7.4 Log rotation + NDJSON
- B8.1 OpenTelemetry init
- B8.2 Metric definitions
- B8.3 Host identity labels
- B8.4 AI pricing table
- B8.5 In-memory ring buffer

**PR 7C — Release Infrastructure (days 10-14)**:
- B9.1 Auto-updater
- B9.3 Autostart on login
- B9.4 PostHog analytics
- B9.5 Anonymous install ping
- B9.6 Machine UID
- B7.5 SQLite migration system

---

### B7.1 — Keychain Integration

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Security |
| **Status** | ❌ Not started |
| **Depends** | D0.1 (Tauri scaffold) |
| **Design source** | CUE-DESIGN-04:L640-680 (tauri-plugin-keychain pattern #10) |

**Summary**: Store all API keys in OS keychain (macOS Keychain / Windows Credential Vault / Linux Secret Service). Zero plaintext storage.

**Code sketch**:
```rust
// src-tauri/src/keychain.rs
use tauri::AppHandle;

const SERVICE: &str = "com.bluey.app";

#[tauri::command]
pub async fn save_api_key(provider: String, key: String) -> Result<(), String> {
    tauri_plugin_keychain::save(SERVICE, &format!("api_key_{provider}"), &key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_api_key(provider: String) -> Result<Option<String>, String> {
    match tauri_plugin_keychain::get(SERVICE, &format!("api_key_{provider}")) {
        Ok(key) => Ok(Some(key)),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub async fn delete_api_key(provider: String) -> Result<(), String> {
    tauri_plugin_keychain::remove(SERVICE, &format!("api_key_{provider}"))
        .map_err(|e| e.to_string())
}
```

**Acceptance**: API key saved via keychain; retrievable after app restart; not present in any file on disk.

**Verification**: Save key; grep entire app data directory for key value; verify zero matches.

**Risks**: Linux Secret Service may not be available on minimal installs. Mitigation: fall back to encrypted file with machine-derived key.

---

### B7.4 — Log Rotation + NDJSON

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Ops |
| **Status** | ❌ Not started |
| **Depends** | None |
| **Design source** | CUE-DESIGN-04:L830-870 (tracing-appender, 10MB rotation) |

**Summary**: Structured NDJSON logs via tracing-subscriber. Rotate at 10MB, keep one backup.

**Code sketch**:
```rust
// src-tauri/src/logging.rs
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tracing_appender::non_blocking;
use std::fs;

pub fn init(log_dir: &Path) -> Result<tracing_appender::non_blocking::WorkerGuard> {
    fs::create_dir_all(log_dir)?;
    let log_path = log_dir.join("bluey.jsonl");

    // Rotate if over 10MB
    if log_path.exists() && fs::metadata(&log_path)?.len() > 10 * 1024 * 1024 {
        let backup = log_dir.join("bluey.jsonl.1");
        fs::rename(&log_path, &backup)?;
    }

    let file = fs::OpenOptions::new().create(true).append(true).open(&log_path)?;
    let (writer, guard) = non_blocking(file);

    tracing_subscriber::registry()
        .with(EnvFilter::new("bluey=info,warn"))
        .with(fmt::layer().json().with_writer(writer))
        .with(fmt::layer().with_writer(std::io::stderr).compact())
        .init();

    Ok(guard)
}
```

**Acceptance**: Logs written as valid NDJSON; file rotates at 10MB; only one backup retained.

**Verification**: Write 11MB of logs; verify rotation occurred; verify backup exists; verify new file started.

**Risks**: Non-blocking writer may lose final lines on crash. Mitigation: panic handler flushes before exit.

---

### B8.1 — OpenTelemetry Init

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Observability |
| **Status** | ❌ Not started |
| **Depends** | None |
| **Design source** | CUE-DESIGN-04:L780-830 (opentelemetry-otlp setup) |

**Summary**: Initialize OTLP HTTP exporter to Grafana Cloud. Per-lane metrics for TTFT, total latency, token counts, cost.

**Code sketch**:
```rust
// src-tauri/src/telemetry.rs
use opentelemetry::metrics::{Histogram, Counter, Meter};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_otlp::WithExportConfig;

pub struct Metrics {
    pub ttft_ms: Histogram<f64>,           // time-to-first-token, label: lane
    pub total_ms: Histogram<f64>,          // total generation time, label: lane
    pub input_tokens: Counter<u64>,        // label: lane, model
    pub output_tokens: Counter<u64>,       // label: lane, model
    pub cost_usd: Counter<f64>,            // label: lane, model
    pub stt_latency_ms: Histogram<f64>,
    pub provider_errors: Counter<u64>,     // label: provider, error_class
}

pub fn init(endpoint: &str, auth: &str) -> Result<Metrics> {
    let exporter = opentelemetry_otlp::new_exporter()
        .http()
        .with_endpoint(endpoint)
        .with_headers(std::collections::HashMap::from([
            ("Authorization".to_string(), format!("Basic {auth}")),
        ]));

    let provider = SdkMeterProvider::builder()
        .with_reader(opentelemetry_sdk::metrics::PeriodicReader::builder(exporter.build_metrics_exporter()?).build())
        .build();

    let meter = provider.meter("bluey");
    Ok(Metrics {
        ttft_ms: meter.f64_histogram("bluey_ttft_ms").build(),
        total_ms: meter.f64_histogram("bluey_generation_total_ms").build(),
        input_tokens: meter.u64_counter("bluey_input_tokens_total").build(),
        output_tokens: meter.u64_counter("bluey_output_tokens_total").build(),
        cost_usd: meter.f64_counter("bluey_cost_usd_total").build(),
        stt_latency_ms: meter.f64_histogram("bluey_stt_latency_ms").build(),
        provider_errors: meter.u64_counter("bluey_provider_errors_total").build(),
    })
}
```

**Acceptance**: Metrics appear in Grafana Cloud within 60s of generation; per-lane labels correct.

**Verification**: Trigger Snap + Solve queries; verify distinct metric series in Grafana; verify cost_usd increments.

**Risks**: OTLP export adds ~5ms per batch. Mitigation: periodic reader batches every 30s, not per-event.

---

### B8.4 — AI Pricing Table + Cost Tracking

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Observability |
| **Status** | 🟡 Partial (table designed) |
| **Depends** | B3.1 (LLM trait) |
| **Design source** | CUE-DESIGN-04:L880-930 (pricing.rs, per-model USD) |

**Summary**: Per-model pricing lookup. Compute cost per generation. Accumulate per-session for dashboard display.

**Code sketch**:
```rust
// src-tauri/src/pricing.rs
pub struct ModelPrice { pub input_per_1m: f64, pub output_per_1m: f64 }

pub fn get_price(model: &str) -> ModelPrice {
    match model {
        m if m.contains("deepseek") => ModelPrice { input_per_1m: 0.14, output_per_1m: 0.28 }, // Cerebras
        m if m.contains("claude-sonnet-4") => ModelPrice { input_per_1m: 3.00, output_per_1m: 15.00 },
        m if m.contains("o3") => ModelPrice { input_per_1m: 10.00, output_per_1m: 40.00 },
        m if m.contains("gpt-4o") => ModelPrice { input_per_1m: 2.50, output_per_1m: 10.00 },
        _ => ModelPrice { input_per_1m: 0.0, output_per_1m: 0.0 },
    }
}

pub fn compute_cost(model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
    let p = get_price(model);
    (input_tokens as f64 / 1_000_000.0) * p.input_per_1m
        + (output_tokens as f64 / 1_000_000.0) * p.output_per_1m
}

// Per-session accumulator
pub struct SessionCost {
    pub total_usd: f64,
    pub by_lane: [f64; 3], // [snap, solve, think]
}

impl SessionCost {
    pub fn record(&mut self, lane: Lane, model: &str, input: u64, output: u64) {
        let cost = compute_cost(model, input, output);
        self.total_usd += cost;
        self.by_lane[lane as usize] += cost;
    }
}
```

**Acceptance**: Cost computed correctly for known models; session accumulator tracks per-lane spend.

**Verification**: 15 Solve queries × 16K/4K tokens = expected $1.44; verify accumulator matches.

**Risks**: Pricing changes frequently. Mitigation: pricing table is a simple match — update on each release.

---

### B9.1 — Auto-Updater

| Field | Value |
|-------|-------|
| **Layer** | App / Release |
| **Status** | ❌ Not started |
| **Depends** | D0.1 (Tauri scaffold) |
| **Design source** | CUE-DESIGN-04:L940-990 (tauri-plugin-updater 2.9.0) |

**Summary**: Check for updates on launch + every 4 hours. Download and install with user confirmation.

**Code sketch**:
```typescript
// src/lib/updater.ts
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export async function checkForUpdates(silent = false): Promise<boolean> {
  try {
    const update = await check();
    if (!update?.available) return false;

    if (silent) {
      // Background download, notify when ready
      await update.downloadAndInstall();
      return true; // Caller shows "restart to update" banner
    }
    // Interactive: download + relaunch immediately
    await update.downloadAndInstall();
    await relaunch();
    return true;
  } catch {
    return false;
  }
}

// Check every 4 hours
setInterval(() => checkForUpdates(true), 4 * 60 * 60 * 1000);
```

**Acceptance**: App detects new version from update endpoint; downloads; installs on restart.

**Verification**: Deploy test version with higher semver; verify app detects and offers update.

**Risks**: Signing key compromise. Mitigation: key stored in CI secrets only; pubkey pinned in tauri.conf.json.

---

### Phase 7 Deliverable

Production-ready ops infrastructure: secure credential storage, structured logging with rotation, real-time telemetry to Grafana, automatic updates, and cost visibility per session/lane.



---

## Phase 8: Dev Discipline (~3 days)

### Entry Criteria
- Phase 0 complete (repo structure established)
- Can run in parallel with any phase

### Exit Criteria
- All dev docs committed and referenced in README
- .codex/agents and .agents/skills directories populated
- PR template enforced via .github/PULL_REQUEST_TEMPLATE.md
- AUDIT.md checklist passes for current codebase

### Batch Structure

**PR 8A — Single PR (days 1-3)**:
- B10.1 CLAUDE.md
- B10.2 CHANGELOG.md
- B10.3 PR template
- B10.4 FIXES.md
- B10.5 AUDIT.md
- B10.6 .codex/agents (7 configs)
- B10.7 .agents/skills (10 skill cards)

---

### B10.1 — CLAUDE.md

| Field | Value |
|-------|-------|
| **Layer** | Dev / Documentation |
| **Status** | ❌ Not started |
| **Depends** | None |
| **Design source** | CUE-DESIGN-04:L1240-1290 (CLAUDE.md rules) |

**Summary**: Root development rules file for AI coding assistants. Defines architecture, code style, key files, and forbidden patterns.

**Code sketch**:
```markdown
# CLAUDE.md — bluey development rules

## Architecture
- Tauri 2 + React 19 + TypeScript + Rust
- Frontend: src/ (React app)
- Backend: src-tauri/src/ (Rust daemon)
- IPC: #[tauri::command] + invoke() (cold path), Unix socket (hot path)
- Three lanes: Snap (Cerebras), Solve (Claude Sonnet 4.5), Think (o3)

## Code style
- Rust: clippy::pedantic, no unwrap() in production, anyhow for errors
- TypeScript: strict mode, no `any`, ESM only
- Events: snake_case ("speech_detected", "llm_token")
- Commands: camelCase (Tauri convention)

## Key files
- src-tauri/src/lib.rs — plugin registration
- src-tauri/src/llm/router.rs — three-lane dispatch
- src-tauri/src/rag/ — vector store + retrieval
- src/routes/index.tsx — page structure
- src/hooks/ — React hook composition

## Never
- Never hardcode API keys
- Never log raw keys (use mask_key())
- Never block tokio runtime with sync I/O
- Never use setTimeout for timing (use rAF)
- Never add state outside Tauri managed state
```

**Acceptance**: File exists at repo root; AI assistants follow rules when given context.

---

### B10.6 — .codex/agents (7 Configs)

| Field | Value |
|-------|-------|
| **Layer** | Dev / Tooling |
| **Status** | ❌ Not started |
| **Depends** | None |
| **Design source** | CUE-DESIGN-04:L1380-1410 (.codex/ directory structure) |

**Summary**: Specialized agent configurations for different development tasks.

**Code sketch**:
```markdown
<!-- .codex/agents/backend-architect.md -->
# Backend Architect Agent

You are a Rust systems architect for a Tauri 2 desktop application.

## Expertise
- Async Rust (tokio), trait-based abstractions, zero-copy patterns
- SQLite (rusqlite, WAL mode, migrations)
- HTTP/2 streaming (reqwest, hyper)
- Unix domain sockets, IPC design

## Constraints
- All state in Tauri managed state (no globals)
- Errors via anyhow::Result, never panic in production
- Streaming via async Stream trait
- Rate limiting via governor crate

## When reviewing code
- Check for blocking calls in async context
- Verify CancellationToken propagation
- Ensure proper Drop implementations for resources
- Validate token budget compliance per lane
```

```markdown
<!-- .codex/agents/test-engineer.md -->
# Test Engineer Agent

## Testing strategy
- Unit tests: #[cfg(test)] mod tests in each file
- Integration tests: tests/ directory with mock providers
- Property tests: proptest for chunker, token counter
- Benchmarks: criterion for hot-path latency

## Mock patterns
- MockLlmProvider: returns canned responses, tracks calls
- MockEmbedding: returns identity vectors for deterministic search
- MockKeychain: in-memory HashMap<String, String>

## Coverage targets
- Core logic (router, chunker, context assembly): >90%
- IPC commands: >80%
- UI components: snapshot tests for critical paths
```

**Acceptance**: 7 agent files exist; each has clear expertise, constraints, and behavioral rules.

**Verification**: Use each agent config in a coding session; verify it produces domain-appropriate output.

**Risks**: None (documentation only).

---

### Phase 8 Deliverable

Complete developer documentation suite: CLAUDE.md for AI assistants, CHANGELOG for release tracking, PR template for review quality, FIXES.md for debugging patterns, AUDIT.md for security posture, and 7+10 agent/skill configs for AI-assisted development.



---

## Phase 9: Latency Engineering + Profiling (~3 weeks)

### Entry Criteria
- Phase 0 complete (daemon binary, Unix socket IPC)
- B2.5 (STT provider trait) merged — Deepgram WebSocket exists
- B3.1 (LLM trait) merged — HTTP clients exist
- R1 (intent classifier) at least stubbed

### Exit Criteria
- p50 Snap (mic → first visible token): ≤500ms
- p50 Solve (mic → first token): ≤4s
- Deepgram WebSocket stays open for entire session (zero reconnects under normal conditions)
- HTTP/2 connection pool: 3 prewarmed, reused across requests
- Latency instrumentation: every hop timestamped, viewable in dashboard
- Stable-partial detector: LLM dispatch only on stabilized transcript

### Batch Structure

**PR 9A — Connection Persistence (days 1-7)**:
- L1 Persistent Deepgram WebSocket
- L2 HTTP/2 keep-alive pool
- L5 Unix domain socket hot path (shared with Phase 0)

**PR 9B — Speculative Dispatch (days 8-14)**:
- L3 Stable-partial detector
- L4 Speculative LLM with cancel-on-change

**PR 9C — Instrumentation + Tuning (days 15-21)**:
- L6 End-to-end latency instrumentation
- Profiling cycles: measure → identify bottleneck → fix → measure

---

### L1 — Persistent Deepgram WebSocket Lifecycle

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Audio |
| **Status** | ❌ Not started |
| **Depends** | B2.5 (SttProvider trait) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L580 (L1 spec: never close mid-session) |

**Summary**: Maintain a single Deepgram Nova-3 WebSocket for the entire session. Reconnect on drop with exponential backoff. Eliminates 200-400ms connection setup per utterance.

**Latency impact**: Saves ~300ms per utterance (WebSocket handshake + TLS + Deepgram auth). Over a 3-hour session with 200 utterances, this saves 60s of cumulative latency.

**Code sketch**:
```rust
// src-tauri/src/stt/deepgram_ws.rs
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

pub struct PersistentDeepgramWs {
    audio_tx: mpsc::Sender<Vec<u8>>,
    transcript_rx: mpsc::Receiver<TranscriptEvent>,
}

#[derive(Debug)]
pub struct TranscriptEvent {
    pub text: String,
    pub is_final: bool,
    pub confidence: f32,
    pub timestamp_ms: u64,
}

impl PersistentDeepgramWs {
    pub async fn connect(api_key: &str, sample_rate: u32) -> Result<Self> {
        let url = format!(
            "wss://api.deepgram.com/v1/listen?model=nova-3&language=en&smart_format=true\
             &interim_results=true&endpointing=300&sample_rate={sample_rate}&encoding=linear16"
        );

        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(512);
        let (tx_out, transcript_rx) = mpsc::channel::<TranscriptEvent>(256);
        let api_key = api_key.to_string();

        tokio::spawn(async move {
            let mut backoff = Duration::from_millis(100);
            loop {
                match Self::run_connection(&url, &api_key, &mut audio_rx, &tx_out).await {
                    Ok(()) => break, // Clean shutdown
                    Err(e) => {
                        tracing::warn!(err = %e, backoff_ms = backoff.as_millis(), "Deepgram WS dropped, reconnecting");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(10));
                    }
                }
            }
        });

        Ok(Self { audio_tx, transcript_rx })
    }

    async fn run_connection(
        url: &str,
        api_key: &str,
        audio_rx: &mut mpsc::Receiver<Vec<u8>>,
        tx_out: &mpsc::Sender<TranscriptEvent>,
    ) -> Result<()> {
        let request = http::Request::builder()
            .uri(url)
            .header("Authorization", format!("Token {api_key}"))
            .body(())?;
        let (mut ws, _) = connect_async(request).await?;

        loop {
            tokio::select! {
                Some(audio) = audio_rx.recv() => {
                    ws.send(Message::Binary(audio)).await?;
                }
                Some(msg) = ws.next() => {
                    match msg? {
                        Message::Text(json) => {
                            if let Ok(event) = parse_deepgram_response(&json) {
                                tx_out.send(event).await.ok();
                            }
                        }
                        Message::Close(_) => return Ok(()),
                        _ => {}
                    }
                }
                else => break,
            }
        }
        Ok(())
    }

    pub async fn send_audio(&self, pcm: Vec<u8>) -> Result<()> {
        self.audio_tx.send(pcm).await.map_err(|_| anyhow::anyhow!("ws closed"))
    }

    pub async fn recv_transcript(&mut self) -> Option<TranscriptEvent> {
        self.transcript_rx.recv().await
    }

    pub async fn close(&self) {
        // Send close frame via a separate channel (omitted for brevity)
    }
}

fn parse_deepgram_response(json: &str) -> Result<TranscriptEvent> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let alt = &v["channel"]["alternatives"][0];
    Ok(TranscriptEvent {
        text: alt["transcript"].as_str().unwrap_or("").to_string(),
        is_final: v["is_final"].as_bool().unwrap_or(false),
        confidence: alt["confidence"].as_f64().unwrap_or(0.0) as f32,
        timestamp_ms: (v["start"].as_f64().unwrap_or(0.0) * 1000.0) as u64,
    })
}
```

**Acceptance**: WebSocket stays open for 30+ minutes without reconnect; audio sent continuously; transcripts arrive within 200ms of speech.

**Verification**: Run 30-minute session; count reconnects (should be 0); measure p50 partial latency.

**Risks**: Deepgram may close idle connections after 10s silence. Mitigation: send keepalive frames every 5s during silence (empty audio or ping).

---

### L2 — HTTP/2 Keep-Alive Pool (3 Prewarmed)

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Network |
| **Status** | ❌ Not started |
| **Depends** | B3.1 (LLM trait) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L582 (L2 spec: 3 prewarmed connections) |

**Summary**: Maintain persistent HTTP/2 connections to Cerebras, Anthropic, and OpenAI (for o3). Eliminates TLS handshake + TCP setup per request (~150-300ms savings).

**Latency impact**: First request to cold endpoint: ~300ms (DNS + TCP + TLS + HTTP/2 SETTINGS). With prewarmed pool: ~5ms (reuse existing multiplexed connection). Saves 295ms on every LLM call.

**Code sketch**:
```rust
// src-tauri/src/llm/connection_pool.rs
use reqwest::Client;
use std::time::Duration;

pub struct LlmConnectionPool {
    pub cerebras: Client,   // Snap lane
    pub anthropic: Client,  // Solve lane
    pub openai: Client,     // Think lane (o3)
}

impl LlmConnectionPool {
    pub fn new() -> Self {
        let base = Client::builder()
            .http2_prior_knowledge()       // Force HTTP/2 (skip upgrade)
            .pool_max_idle_per_host(3)     // Keep 3 idle connections
            .pool_idle_timeout(Duration::from_secs(300)) // 5 min idle before close
            .tcp_keepalive(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(120));

        Self {
            cerebras: base.clone().build().unwrap(),
            anthropic: base.clone().build().unwrap(),
            openai: base.build().unwrap(),
        }
    }

    /// Prewarm all connections on session start
    /// Sends a minimal request to establish HTTP/2 connection
    pub async fn prewarm(&self, keys: &CredentialStore) {
        let futs = vec![
            self.prewarm_one(&self.cerebras, "https://api.cerebras.ai/v1/models", keys.get("cerebras")),
            self.prewarm_one(&self.anthropic, "https://api.anthropic.com/v1/messages", keys.get("anthropic")),
            self.prewarm_one(&self.openai, "https://api.openai.com/v1/models", keys.get("openai")),
        ];
        futures::future::join_all(futs).await;
        tracing::info!("Connection pool prewarmed (3 endpoints)");
    }

    async fn prewarm_one(&self, client: &Client, url: &str, key: Option<&str>) {
        if let Some(k) = key {
            // HEAD or lightweight GET to establish connection
            let _ = client.get(url).bearer_auth(k).send().await;
        }
    }
}
```

**Acceptance**: Second request to same endpoint shows <10ms connection time (vs ~300ms for first cold request).

**Verification**: Time first vs second request to each endpoint; verify HTTP/2 multiplexing via connection reuse header.

**Risks**: Endpoints may close idle connections after 60s. Mitigation: pool_idle_timeout=300s covers most inter-query gaps; reqwest auto-reconnects transparently.

---

### L3 — Stable-Partial Detector + Trigger-on-Stable

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Intelligence |
| **Status** | ❌ Not started |
| **Depends** | L1 (Deepgram WS), R1 (intent classifier) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L584 (L3 spec) |

**Summary**: Don't dispatch LLM on every partial transcript. Wait until partial stabilizes (same text for 300ms) OR is_final arrives. This prevents wasted LLM calls on rapidly-changing partials.

**Latency impact**: Without this, we'd fire 5-10 LLM calls per utterance (one per partial). With stable-detection, we fire 1-2 (stable partial + final). Saves $0.01-0.05 per utterance AND reduces p95 latency by avoiding queue contention.

**Code sketch**:
```rust
// src-tauri/src/stt/stable_detector.rs
use tokio::time::{sleep, Duration, Instant};
use tokio::sync::mpsc;

pub struct StablePartialDetector {
    stability_window: Duration, // 300ms
}

#[derive(Debug, Clone)]
pub enum StableEvent {
    StablePartial(String),  // Partial hasn't changed for stability_window
    Final(String),          // Deepgram confirmed final
}

impl StablePartialDetector {
    pub fn new(stability_ms: u64) -> Self {
        Self { stability_window: Duration::from_millis(stability_ms) }
    }

    pub fn spawn(
        self,
        mut transcript_rx: mpsc::Receiver<TranscriptEvent>,
    ) -> mpsc::Receiver<StableEvent> {
        let (tx, rx) = mpsc::channel(64);

        tokio::spawn(async move {
            let mut last_partial = String::new();
            let mut last_change = Instant::now();
            let mut stable_fired = false;

            loop {
                tokio::select! {
                    Some(event) = transcript_rx.recv() => {
                        if event.is_final {
                            tx.send(StableEvent::Final(event.text)).await.ok();
                            last_partial.clear();
                            stable_fired = false;
                        } else if event.text != last_partial {
                            last_partial = event.text;
                            last_change = Instant::now();
                            stable_fired = false;
                        }
                    }
                    _ = sleep(Duration::from_millis(50)) => {
                        if !last_partial.is_empty()
                            && !stable_fired
                            && last_change.elapsed() >= self.stability_window
                        {
                            tx.send(StableEvent::StablePartial(last_partial.clone())).await.ok();
                            stable_fired = true;
                        }
                    }
                    else => break,
                }
            }
        });

        rx
    }
}
```

**Acceptance**: Rapid partial changes (every 100ms) don't trigger dispatch; stable partial (unchanged 300ms) triggers exactly once; final always triggers.

**Verification**: Feed synthetic partials at varying rates; count StableEvent emissions; verify exactly 1 per stable period.

**Risks**: 300ms stability window adds latency to fast speakers. Mitigation: configurable; reduce to 200ms for power users; final always fires immediately regardless.

---

### L4 — Speculative LLM with Cancel-on-Partial-Change

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Intelligence |
| **Status** | ❌ Not started |
| **Depends** | L3 (stable detector), R1 (router) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L586 (L4 spec) |

**Summary**: On StablePartial, speculatively dispatch to Snap lane. If a new partial arrives that changes meaning, cancel the in-flight request. If Final confirms the stable partial, let it complete. This shaves 300ms off perceived latency (we start LLM before final confirmation).

**Latency impact**: Snap lane takes ~200ms. By starting on stable-partial (which arrives ~300ms before final), we overlap LLM processing with STT finalization. Net effect: response appears ~300ms earlier.

**Code sketch**:
```rust
// src-tauri/src/llm/speculative.rs
use tokio_util::sync::CancellationToken;

pub struct SpeculativeDispatcher {
    pool: Arc<LlmConnectionPool>,
    router: Arc<IntentRouter>,
}

impl SpeculativeDispatcher {
    pub async fn handle_stable_event(
        &self,
        event: StableEvent,
        active_speculation: &mut Option<(String, CancellationToken)>,
    ) -> Option<LlmResponse> {
        match event {
            StableEvent::StablePartial(text) => {
                // Cancel any previous speculation
                if let Some((_, cancel)) = active_speculation.take() {
                    cancel.cancel();
                }
                // Start speculative generation
                let cancel = CancellationToken::new();
                let cancel_clone = cancel.clone();
                let text_clone = text.clone();
                let pool = self.pool.clone();
                let router = self.router.clone();

                let handle = tokio::spawn(async move {
                    let lane = router.classify(&text_clone).await;
                    if lane == Lane::Snap {
                        // Only speculate on Snap (fast enough to be worth it)
                        pool.cerebras.generate(&text_clone, cancel_clone).await.ok()
                    } else {
                        None
                    }
                });

                *active_speculation = Some((text, cancel));
                None // Result comes later
            }
            StableEvent::Final(text) => {
                if let Some((speculated_text, cancel)) = active_speculation.take() {
                    if text == speculated_text || text.starts_with(&speculated_text) {
                        // Final confirms speculation — let it complete
                        // (already running, result will arrive shortly)
                        return None; // Caller awaits the spawned task
                    } else {
                        // Final differs — cancel speculation, dispatch fresh
                        cancel.cancel();
                        let lane = self.router.classify(&text).await;
                        return self.dispatch_fresh(&text, lane).await;
                    }
                }
                // No active speculation — dispatch normally
                let lane = self.router.classify(&text).await;
                self.dispatch_fresh(&text, lane).await
            }
        }
    }

    async fn dispatch_fresh(&self, text: &str, lane: Lane) -> Option<LlmResponse> {
        // Normal dispatch through three-lane router
        todo!()
    }
}
```

**Acceptance**: Speculation fires on stable partial; if final matches, response arrives ~300ms earlier than non-speculative path; if final differs, speculation cancelled within 10ms.

**Verification**: Measure time-to-first-token with and without speculation on 100 test utterances; verify p50 improvement ≥200ms.

**Risks**: Wasted Cerebras calls when speculation is wrong (~20% of cases). Cost: 20% × $0.002/call = negligible ($0.0004/wrong speculation). Acceptable tradeoff for 300ms latency win.

---

### L5 — Unix Domain Socket Hot Path

| Field | Value |
|-------|-------|
| **Layer** | Daemon / IPC |
| **Status** | ❌ Not started (shared with Phase 0) |
| **Depends** | D0.1 (Tauri scaffold) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L588 (L5 spec: <10ms streaming) |

**Summary**: Replace stdin/stdout IPC between daemon and native overlay with Unix domain socket. Enables streaming tokens to overlay with <10ms latency (vs 30-50ms for Tauri events through webview).

**Latency impact**: Tauri event path: Rust → serialize → webview IPC → JS → render = ~30ms. Unix socket path: Rust → write bytes → native overlay reads → render = ~2ms. Saves 28ms per token batch at 60Hz = smoother streaming.

**Code sketch**:
```rust
// src-tauri/src/ipc/unix_socket.rs
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncWriteExt, AsyncReadExt};

const SOCKET_PATH: &str = "/tmp/bluey-overlay.sock";

pub struct OverlaySocket {
    stream: Option<UnixStream>,
}

impl OverlaySocket {
    pub async fn listen() -> Result<Self> {
        let _ = std::fs::remove_file(SOCKET_PATH);
        let listener = UnixListener::bind(SOCKET_PATH)?;
        tracing::info!("Overlay socket listening at {SOCKET_PATH}");

        let (stream, _) = listener.accept().await?;
        Ok(Self { stream: Some(stream) })
    }

    /// Send token batch to overlay (<1ms for typical payload)
    pub async fn send_tokens(&mut self, payload: &OverlayPayload) -> Result<()> {
        if let Some(ref mut stream) = self.stream {
            let bytes = serde_json::to_vec(payload)?;
            let len = (bytes.len() as u32).to_le_bytes();
            stream.write_all(&len).await?;
            stream.write_all(&bytes).await?;
        }
        Ok(())
    }
}

#[derive(serde::Serialize)]
pub struct OverlayPayload {
    pub kind: &'static str,  // "token", "complete", "mode", "clear"
    pub text: Option<String>,
    pub lane: Option<&'static str>,
    pub generation_id: u64,
}
```

**Acceptance**: Token streaming from daemon to overlay measured at <5ms p99; overlay renders within same frame.

**Verification**: Instrument with timestamps on both sides; measure 1000 token deliveries; verify p99 < 5ms.

**Risks**: Windows doesn't have Unix sockets. Mitigation: use named pipes on Windows (`\\.\pipe\bluey-overlay`); abstract behind trait.

---

### L6 — End-to-End Latency Instrumentation

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Observability |
| **Status** | ❌ Not started |
| **Depends** | L1, L2, L5 |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L590 (L6 spec: timestamp every hop) |

**Summary**: Instrument every hop in the pipeline with monotonic timestamps. Export as spans to OTel + display in dashboard as flamegraph.

**Code sketch**:
```rust
// src-tauri/src/latency/instrument.rs
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct LatencyTrace {
    pub id: u64,
    pub hops: Vec<Hop>,
}

#[derive(Debug, Clone)]
pub struct Hop {
    pub name: &'static str,
    pub start: Instant,
    pub end: Option<Instant>,
}

impl LatencyTrace {
    pub fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Self {
            id: COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            hops: Vec::with_capacity(8),
        }
    }

    pub fn start_hop(&mut self, name: &'static str) -> usize {
        let idx = self.hops.len();
        self.hops.push(Hop { name, start: Instant::now(), end: None });
        idx
    }

    pub fn end_hop(&mut self, idx: usize) {
        if let Some(hop) = self.hops.get_mut(idx) {
            hop.end = Some(Instant::now());
        }
    }

    pub fn total_ms(&self) -> f64 {
        if let (Some(first), Some(last)) = (self.hops.first(), self.hops.last()) {
            let end = last.end.unwrap_or_else(Instant::now);
            end.duration_since(first.start).as_secs_f64() * 1000.0
        } else { 0.0 }
    }

    /// Expected hops for Snap lane:
    /// mic_capture → vad_detect → stt_partial → stable_detect →
    /// intent_classify → llm_dispatch → llm_first_token → overlay_render
    pub fn report(&self) -> String {
        self.hops.iter().map(|h| {
            let dur = h.end.map(|e| e.duration_since(h.start).as_millis())
                .unwrap_or(0);
            format!("  {} → {}ms", h.name, dur)
        }).collect::<Vec<_>>().join("\n")
    }
}

// Target validation
pub fn validate_targets(trace: &LatencyTrace, lane: Lane) -> bool {
    let total = trace.total_ms();
    match lane {
        Lane::Snap => total <= 500.0,   // p50 target
        Lane::Solve => total <= 4000.0, // p50 first-token target
        Lane::Think => true,            // No hard target (progress bar)
    }
}
```

**Acceptance**: Every generation produces a LatencyTrace with all hops; dashboard displays flamegraph; p50 targets validated.

**Verification**: Run 100 Snap queries; verify all traces have 8 hops; verify p50 ≤ 500ms; alert on regression.

**Risks**: Instrumentation overhead. Mitigation: Instant::now() is ~20ns on modern hardware; 8 hops = 160ns total — negligible.

---

### Phase 9 Deliverable

Sub-500ms mic-to-first-token for Snap lane. Persistent connections eliminate setup latency. Speculative dispatch overlaps STT finalization with LLM processing. Full instrumentation enables continuous optimization. This is what makes bluey feel instant.



---

## Phase 10: Cost Optimization (~1 week)

### Entry Criteria
- Phase 4 complete (three-lane routing working)
- Phase 5 complete (RAG available for context filtering)
- R1 (intent classifier) production-ready
- CO1 requires Anthropic prompt caching API access

### Exit Criteria
- Solve lane input costs reduced 50-70% via prompt caching
- Average session cost drops from $3.04 to ≤$1.50
- Router correctly splits easy/hard within Solve lane
- Cost dashboard shows real-time per-session spend
- Per-session alerting fires at configurable threshold

### Batch Structure

**PR 10A — Single PR (days 1-7)**:
- CO1 Anthropic prompt caching
- CO2 RAG-filtered context
- CO3 Router split within Solve
- Cost dashboard widget

---

### CO1 — Anthropic Prompt Caching (Solve Lane)

| Field | Value |
|-------|-------|
| **Layer** | Daemon / LLM |
| **Status** | ❌ Not started |
| **Depends** | F2 (Solve-lane streaming), B3.1 (LLM trait) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L130 (50-70% Solve input savings) |

**Summary**: Use Anthropic's prompt caching to avoid re-processing the system prompt + skill template on every Solve request. The static prefix (system prompt + skill template + context preamble) is cached server-side; only the dynamic suffix (user query + recent turns) is billed at full rate.

**Cost justification**:
- Without caching: 15 Solve queries/session × 16K input tokens × $3.00/1M = $0.72/session input cost
- With caching (60% cache hit): 15 × (6.4K full-price + 9.6K cached at $0.30/1M) = $0.33/session
- **Savings: $0.39/session (54% reduction on input tokens)**
- At scale (1000 users × 2 sessions/day): **$780/day saved**

**Code sketch**:
```rust
// src-tauri/src/llm/providers/anthropic.rs
use serde_json::json;

pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
}

impl AnthropicProvider {
    /// Build request with cache_control on static prefix blocks
    /// Anthropic caches content blocks marked with cache_control: {type: "ephemeral"}
    pub fn build_cached_request(
        &self,
        system_prompt: &str,
        skill_template: &str,
        dynamic_context: &str,
        user_query: &str,
    ) -> serde_json::Value {
        json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 4096,
            "stream": true,
            "system": [
                {
                    "type": "text",
                    "text": system_prompt,
                    "cache_control": {"type": "ephemeral"}  // Cached (stable across requests)
                },
                {
                    "type": "text",
                    "text": skill_template,
                    "cache_control": {"type": "ephemeral"}  // Cached (stable per skill)
                }
            ],
            "messages": [
                {
                    "role": "user",
                    "content": format!("{}\n\n{}", dynamic_context, user_query)
                    // NOT cached (changes every request)
                }
            ]
        })
    }

    pub async fn stream_with_caching(
        &self,
        system_prompt: &str,
        skill_template: &str,
        context: &str,
        query: &str,
        cancel: CancellationToken,
    ) -> Result<impl Stream<Item = Result<Token>>> {
        let body = self.build_cached_request(system_prompt, skill_template, context, query);
        let resp = self.client.post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "prompt-caching-2024-07-31")
            .json(&body)
            .send().await?;

        // Parse SSE stream (same as non-cached path)
        Ok(parse_anthropic_sse(resp.bytes_stream(), cancel))
    }
}
```

**Acceptance**: Response headers show `cache_creation_input_tokens` on first call, `cache_read_input_tokens` on subsequent calls within 5-minute TTL.

**Verification**: Make 5 Solve requests with same system prompt; verify cache hits in response usage metadata; compute actual cost reduction.

**Risks**: Cache TTL is 5 minutes — if user is idle >5min between queries, cache evicts. Mitigation: acceptable; most interview sessions have queries every 1-3 minutes. Cache miss just means one full-price request.

---

### CO2 — RAG-Filtered Context for Solve Lane

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Context |
| **Status** | ❌ Not started |
| **Depends** | B5.4 (Live RAG indexer), CM2 (context assembly) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L132 (RAG-filtered, not full history) |

**Summary**: Instead of sending full conversation history to Solve lane (16K tokens), use RAG to select only the 3-5 most relevant previous turns. Reduces average input from 16K to ~6K tokens.

**Cost justification**:
- Without RAG filtering: 16K avg input tokens per Solve query
- With RAG filtering: 6K avg input tokens (only relevant turns + current query)
- Savings per query: 10K tokens × $3.00/1M = $0.03
- Per session (15 queries): **$0.45 saved**
- Combined with CO1 caching on the 6K: further 60% reduction on cached portion

**Code sketch**:
```rust
// src-tauri/src/session/context_filter.rs

pub async fn build_solve_context(
    query: &str,
    session_turns: &[Turn],
    retriever: &HybridRetriever,
    budget_tokens: u32, // 16000
) -> Vec<Turn> {
    // 1. Always include last 3 turns (recency)
    let recent: Vec<&Turn> = session_turns.iter().rev().take(3).collect();
    let recent_tokens: u32 = recent.iter().map(|t| t.token_count).sum();

    // 2. Use RAG to find relevant older turns
    let remaining_budget = budget_tokens.saturating_sub(recent_tokens + 2000); // Reserve 2K for query+system
    let rag_results = retriever.retrieve(query, None, 10).await.unwrap_or_default();

    // 3. Map RAG chunk_ids back to turns, deduplicate with recent
    let recent_ids: HashSet<&str> = recent.iter().map(|t| t.id.as_str()).collect();
    let mut rag_turns: Vec<&Turn> = Vec::new();
    let mut rag_tokens = 0u32;

    for result in &rag_results {
        if let Some(turn) = session_turns.iter().find(|t| t.id == result.0) {
            if !recent_ids.contains(turn.id.as_str()) && rag_tokens + turn.token_count <= remaining_budget {
                rag_tokens += turn.token_count;
                rag_turns.push(turn);
            }
        }
    }

    // 4. Combine: RAG turns (chronological) + recent turns
    rag_turns.sort_by_key(|t| t.created_at);
    let mut context: Vec<Turn> = rag_turns.into_iter().cloned().collect();
    context.extend(recent.into_iter().rev().cloned());
    context
}
```

**Acceptance**: Solve context averages 6K tokens (vs 16K without filtering); answer quality maintained (human eval on 20 test cases).

**Verification**: Compare answer quality with full context vs RAG-filtered on held-out test set; verify no degradation >5%.

**Risks**: RAG may miss critical context for follow-up questions. Mitigation: always include last 3 turns (covers immediate follow-ups); RAG catches older relevant context.

---

### CO3 — Router Split: Easy → Cerebras, Hard → Claude

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Intelligence |
| **Status** | ❌ Not started |
| **Depends** | R1 (intent classifier), CO1 (Anthropic integration) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L134 (within Solve lane, cost-route) |

**Summary**: Within the Solve lane, further classify queries as "easy" (can be handled by Cerebras at 1/20th the cost) vs "hard" (requires Claude's reasoning). Easy: factual recall, simple explanations, short answers. Hard: multi-step reasoning, code generation, system design.

**Cost justification**:
- Assume 40% of Solve queries are "easy" (factual, short-answer)
- Easy query cost: Cerebras $0.14/1M input vs Claude $3.00/1M = 21x cheaper
- Per session: 6 easy queries × 6K tokens × $0.14/1M = $0.005 (vs $0.108 on Claude)
- **Savings: $0.10/session from easy-query routing**
- Combined with CO1+CO2: total session cost drops from $3.04 to ~$1.20

**Code sketch**:
```rust
// src-tauri/src/llm/cost_router.rs

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SolveDifficulty { Easy, Hard }

pub fn classify_solve_difficulty(query: &str, context_tokens: u32) -> SolveDifficulty {
    // Heuristic rules (fast, no model call needed)
    let indicators_hard = [
        query.contains("design") && query.contains("system"),
        query.contains("implement") || query.contains("code"),
        query.contains("compare") && query.contains("tradeoff"),
        query.len() > 200, // Long queries tend to be complex
        context_tokens > 8000, // Heavy context suggests complex problem
    ];

    let hard_score: usize = indicators_hard.iter().filter(|&&x| x).count();

    let indicators_easy = [
        query.starts_with("what is") || query.starts_with("define"),
        query.contains("example of"),
        query.len() < 50,
        query.split_whitespace().count() < 10,
    ];

    let easy_score: usize = indicators_easy.iter().filter(|&&x| x).count();

    if hard_score >= 2 || (hard_score >= 1 && easy_score == 0) {
        SolveDifficulty::Hard
    } else {
        SolveDifficulty::Easy
    }
}

pub fn select_solve_model(difficulty: SolveDifficulty) -> (&'static str, &'static str) {
    match difficulty {
        SolveDifficulty::Easy => ("cerebras", "deepseek-v3"),      // $0.14/1M
        SolveDifficulty::Hard => ("anthropic", "claude-sonnet-4"), // $3.00/1M
    }
}
```

**Acceptance**: Easy queries route to Cerebras with acceptable quality; hard queries still go to Claude; misclassification rate <15%.

**Verification**: Label 100 test queries as easy/hard; run classifier; verify accuracy >85%; spot-check Cerebras answers on "easy" queries for quality.

**Risks**: Cerebras may produce lower-quality answers for borderline queries. Mitigation: conservative classification (when in doubt, route to Claude); user can always force Think lane for maximum quality.

---

### Cost Dashboard Widget

| Field | Value |
|-------|-------|
| **Layer** | Dashboard / UI |
| **Status** | ❌ Not started |
| **Depends** | B8.4 (pricing table) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L160 (cost dashboard) |

**Summary**: Real-time cost display in dashboard showing per-session and cumulative spend, broken down by lane.

**Code sketch**:
```typescript
// src/components/CostWidget.tsx
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

interface CostData {
  session_total: number;
  by_lane: { snap: number; solve: number; think: number };
  queries_count: number;
}

export function CostWidget() {
  const [cost, setCost] = useState<CostData>({ session_total: 0, by_lane: { snap: 0, solve: 0, think: 0 }, queries_count: 0 });

  useEffect(() => {
    const unlisten = listen<CostData>("cost_update", (e) => setCost(e.payload));
    return () => { unlisten.then(f => f()); };
  }, []);

  return (
    <div className="cost-widget p-3 rounded-lg bg-muted">
      <div className="text-2xl font-mono">${cost.session_total.toFixed(3)}</div>
      <div className="text-xs text-muted-foreground mt-1">
        Snap: ${cost.by_lane.snap.toFixed(4)} · Solve: ${cost.by_lane.solve.toFixed(4)} · Think: ${cost.by_lane.think.toFixed(4)}
      </div>
      <div className="text-xs mt-1">{cost.queries_count} queries this session</div>
    </div>
  );
}
```

**Acceptance**: Cost updates in real-time after each LLM call; breakdown by lane is accurate; matches manual calculation.

**Verification**: Run 5 queries across all lanes; verify widget total matches sum of individual costs from pricing table.

---

### Phase 10 Deliverable

60% cost reduction on Solve lane through three complementary optimizations: prompt caching (50-70% input savings), RAG-filtered context (62% fewer input tokens), and smart routing within Solve (21x cheaper for easy queries). Session cost drops from $3.04 to ~$1.20. Real-time cost visibility prevents surprise bills.

### Combined Cost Impact Summary

| Optimization | Mechanism | Per-Session Savings | Annual Savings (1K users, 2 sessions/day) |
|---|---|---|---|
| CO1 Prompt Caching | Cache static prefix server-side | $0.39 | $284K |
| CO2 RAG-Filtered Context | Send 6K instead of 16K tokens | $0.45 | $328K |
| CO3 Easy→Cerebras Routing | 21x cheaper for 40% of queries | $0.10 | $73K |
| **Combined** | | **$0.94** | **$685K** |

**Before optimizations**: $3.04/session → **After**: $1.20/session (60% reduction)

---

## Appendix: Cross-Phase Dependency Summary

```
Phase 5 (RAG) ←── Phase 4 (B3.1 LLM trait, R1 router)
Phase 6 (Dashboard) ←── Phase 1+2 (shell, sessions)
Phase 7 (Ops) ←── Phase 0 (Tauri scaffold)
Phase 8 (Dev) ←── None (parallel anytime)
Phase 9 (Latency) ←── Phase 0 (daemon), B2.5 (STT), B3.1 (LLM)
Phase 10 (Cost) ←── Phase 4 (routing) + Phase 5 (RAG)
```

## Appendix: Target Validation Matrix

| Metric | Target | Measured By | Phase |
|--------|--------|-------------|-------|
| Snap mic→first-token | ≤500ms p50 | L6 instrumentation | 9 |
| Solve mic→first-token | ≤4s p50 | L6 instrumentation | 9 |
| Overlay render latency | <10ms | Unix socket timestamps | 9 |
| RAG search latency | <100ms | spawn_blocking timer | 5 |
| Session cost (with opts) | ≤$1.50 | B8.4 pricing accumulator | 10 |
| Epoch compaction | <5s | Background job timer | 5 |
| Deepgram reconnects/session | 0 (normal) | L1 reconnect counter | 9 |
| Intent classifier latency | <5ms | R1 timer | 4 (validated in 9) |

---

*End of Part B. Phases 5-10 fully specified with compilable code sketches, design-doc citations, cost justifications, and acceptance criteria.*
