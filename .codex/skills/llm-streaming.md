# Skill: LLM Streaming Patterns

## Anthropic Streaming (SSE)

```rust
use reqwest::Client;
use futures::StreamExt;

let response = client
    .post("https://api.anthropic.com/v1/messages")
    .header("x-api-key", &api_key)
    .header("anthropic-version", "2023-06-01")
    .json(&serde_json::json!({
        "model": "claude-sonnet-4-20250514",
        "max_tokens": 4096,
        "stream": true,
        "messages": messages,
    }))
    .send()
    .await?;

let mut stream = response.bytes_stream();
let mut buffer = String::new();

while let Some(chunk) = stream.next().await {
    let bytes = chunk?;
    buffer.push_str(&String::from_utf8_lossy(&bytes));

    // Parse SSE events from buffer
    while let Some(pos) = buffer.find("\n\n") {
        let event = &buffer[..pos];
        if let Some(data) = event.strip_prefix("data: ") {
            let parsed: StreamEvent = serde_json::from_str(data)?;
            match parsed {
                StreamEvent::ContentBlockDelta { delta } => {
                    tx.send(delta.text).await?;
                }
                StreamEvent::MessageStop => break,
                _ => {}
            }
        }
        buffer = buffer[pos + 2..].to_string();
    }
}
```

## OpenAI Streaming

```rust
let response = client
    .post("https://api.openai.com/v1/chat/completions")
    .bearer_auth(&api_key)
    .json(&serde_json::json!({
        "model": "gpt-4o",
        "stream": true,
        "messages": messages,
    }))
    .send()
    .await?;

// Same SSE parsing, different event shape:
// data: {"choices":[{"delta":{"content":"token"}}]}
```

## Multi-Provider Fallback

```rust
pub struct LlmRouter {
    providers: Vec<Box<dyn LlmProvider>>,
}

impl LlmRouter {
    pub async fn stream(&self, req: &LlmRequest) -> Result<impl Stream<Item = String>> {
        for provider in &self.providers {
            match provider.stream(req).await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    tracing::warn!(provider = provider.name(), error = %e, "fallback");
                    continue;
                }
            }
        }
        anyhow::bail!("all providers failed")
    }
}
```

## Cancellation

```rust
// Cancel in-flight LLM request
let token = CancellationToken::new();
let cancel = token.clone();

tokio::select! {
    result = stream_llm(&client, &request) => handle(result),
    _ = cancel.cancelled() => {
        tracing::info!("LLM request cancelled by user");
    }
}
```
