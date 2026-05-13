# Skill: Testing Patterns

## Async Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_stream_completes_within_timeout() {
        let result = timeout(Duration::from_secs(5), async {
            let provider = MockLlmProvider::new(vec!["hello", " world"]);
            provider.stream(&request).await
        }).await;

        assert!(result.is_ok(), "stream timed out");
    }
}
```

## Mock HTTP (wiremock)

```rust
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[tokio::test]
async fn test_anthropic_streaming() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "test-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("data: {\"type\":\"content_block_delta\",...}\n\n")
        )
        .mount(&mock_server)
        .await;

    let client = AnthropicClient::new("test-key", &mock_server.uri());
    let chunks: Vec<_> = client.stream(&req).collect().await;
    assert_eq!(chunks.len(), 1);
}
```

## In-Memory SQLite

```rust
#[cfg(test)]
fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../migrations/001_init.sql")).unwrap();
    conn
}

#[test]
fn test_insert_and_query_chunk() {
    let db = test_db();
    insert_chunk(&db, &chunk).unwrap();
    let results = search(&db, "test query", 5).unwrap();
    assert_eq!(results.len(), 1);
}
```

## Snapshot Tests (insta)

```rust
use insta::assert_snapshot;

#[test]
fn test_prompt_assembly() {
    let prompt = build_prompt(&transcript, &rag_chunks, "system");
    assert_snapshot!(prompt);
}

// Run `cargo insta review` to approve/reject changes
```

## Property Tests (proptest)

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn chunker_never_exceeds_max_tokens(text in "\\PC{1,10000}") {
        let chunker = SemanticChunker::new(256, 32);
        let chunks = chunker.chunk(&text);
        for chunk in &chunks {
            prop_assert!(chunk.tokens <= 256 + 32); // max + overlap tolerance
        }
    }

    #[test]
    fn chunker_preserves_all_content(text in "[a-z ]{1,1000}") {
        let chunker = SemanticChunker::new(64, 0); // no overlap for this test
        let chunks = chunker.chunk(&text);
        let reassembled: String = chunks.iter().map(|c| c.text.as_str()).collect();
        prop_assert_eq!(reassembled.trim(), text.trim());
    }
}
```

## Test Organization

```
crates/cue-core/
├── src/
│   ├── chunker.rs          # has #[cfg(test)] mod tests
│   └── router.rs           # has #[cfg(test)] mod tests
└── tests/
    ├── integration_llm.rs  # full provider mock tests
    └── integration_rag.rs  # chunker + embed + search
```
