# Skill: RAG with sqlite-vec

## Overview

Cue uses SQLite + sqlite-vec for local vector search. No external DB needed.

## Schema

```sql
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    content TEXT NOT NULL,
    token_count INTEGER NOT NULL,
    start_time_ms INTEGER,
    end_time_ms INTEGER,
    speaker TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

-- sqlite-vec virtual table for embeddings
CREATE VIRTUAL TABLE chunk_embeddings USING vec0(
    chunk_id INTEGER PRIMARY KEY,
    embedding FLOAT[384]  -- all-MiniLM-L6-v2 dimension
);
```

## Semantic Chunking

```rust
pub struct SemanticChunker {
    max_tokens: usize,      // 256
    overlap_tokens: usize,  // 32
    sentence_splitter: fn(&str) -> Vec<&str>,
}

impl SemanticChunker {
    pub fn chunk(&self, text: &str) -> Vec<Chunk> {
        let sentences = (self.sentence_splitter)(text);
        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut current_tokens = 0;

        for sentence in sentences {
            let tokens = count_tokens(sentence);
            if current_tokens + tokens > self.max_tokens && !current.is_empty() {
                chunks.push(Chunk { text: current.clone(), tokens: current_tokens });
                // Keep overlap
                current = self.last_n_tokens(&current, self.overlap_tokens);
                current_tokens = self.overlap_tokens;
            }
            current.push_str(sentence);
            current_tokens += tokens;
        }
        if !current.is_empty() {
            chunks.push(Chunk { text: current, tokens: current_tokens });
        }
        chunks
    }
}
```

## Embedding Providers

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
}

// Local: use candle or ort for on-device inference
// Remote: OpenAI text-embedding-3-small, Voyage, etc.
```

## Vector Search

```rust
pub async fn search_similar(
    db: &Connection,
    query_embedding: &[f32],
    meeting_id: &str,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let results = db.prepare(
        "SELECT c.id, c.content, c.speaker, c.start_time_ms,
                distance
         FROM chunk_embeddings e
         INNER JOIN chunks c ON c.id = e.chunk_id
         WHERE c.meeting_id = ?1
         ORDER BY e.embedding <-> ?2
         LIMIT ?3"
    )?
    .query_map(params![meeting_id, query_embedding, limit], |row| {
        Ok(SearchResult {
            chunk_id: row.get(0)?,
            content: row.get(1)?,
            speaker: row.get(2)?,
            start_time_ms: row.get(3)?,
            distance: row.get(4)?,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}
```

## Indexing Pipeline

```
Audio → STT → Transcript → Chunker → Embedder → sqlite-vec INSERT
                                                       ↓
User query → Embed query → sqlite-vec KNN search → Top-K chunks → LLM context
```
