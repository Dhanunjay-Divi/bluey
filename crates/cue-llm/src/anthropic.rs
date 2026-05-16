use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{LlmChunk, LlmChunkStream, LlmError, LlmProvider, LlmRequest, LlmResponse};

fn default_base_url() -> String {
    obfstr::obfstr!("https://api.anthropic.com").to_string()
}
const DEFAULT_MODEL: &str = "claude-3-5-sonnet-20241022";

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model: DEFAULT_MODEL.to_string(),
            base_url: default_base_url(),
        }
    }

    #[cfg(test)]
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Msg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize)]
struct Msg {
    role: &'static str,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

fn parse_sse_chunks(buffer: &mut String) -> Vec<Result<LlmChunk, LlmError>> {
    let mut chunks = Vec::new();
    while let Some(pos) = buffer.find("\n\n") {
        let frame = buffer[..pos].to_string();
        *buffer = buffer[pos + 2..].to_string();
        for line in frame.lines() {
            let line = line.trim();
            if let Some(data) = line.strip_prefix("data: ") {
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                    match event.get("type").and_then(|t| t.as_str()) {
                        Some("content_block_delta") => {
                            if let Some(text) = event
                                .get("delta")
                                .and_then(|d| d.get("text"))
                                .and_then(|t| t.as_str())
                            {
                                chunks.push(Ok(LlmChunk {
                                    text: text.to_string(),
                                    finished: false,
                                }));
                            }
                        }
                        Some("message_stop") => {
                            chunks.push(Ok(LlmChunk {
                                text: String::new(),
                                finished: true,
                            }));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    chunks
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let body = ApiRequest {
            model: self.model.clone(),
            max_tokens: req.max_tokens.unwrap_or(1024),
            system: req.system.clone(),
            messages: vec![Msg {
                role: "user",
                content: req.user.clone(),
            }],
            temperature: req.temperature,
            stream: false,
        };

        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header(obfstr::obfstr!("x-api-key"), &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        match resp.status().as_u16() {
            401 => return Err(LlmError::Auth),
            429 => return Err(LlmError::Quota("rate limited".into())),
            s if s >= 500 => return Err(LlmError::Provider(format!("server error: {s}"))),
            s if s >= 400 => {
                let text = resp.text().await.unwrap_or_default();
                return Err(LlmError::Provider(format!("{s}: {text}")));
            }
            _ => {}
        }

        let api_resp: ApiResponse = resp
            .json()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        let text = api_resp
            .content
            .into_iter()
            .map(|b| b.text)
            .collect::<Vec<_>>()
            .join("");

        Ok(LlmResponse { text })
    }

    async fn complete_stream(&self, req: &LlmRequest) -> Result<LlmChunkStream, LlmError> {
        let body = ApiRequest {
            model: self.model.clone(),
            max_tokens: req.max_tokens.unwrap_or(1024),
            system: req.system.clone(),
            messages: vec![Msg {
                role: "user",
                content: req.user.clone(),
            }],
            temperature: req.temperature,
            stream: true,
        };

        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        match resp.status().as_u16() {
            401 => return Err(LlmError::Auth),
            429 => return Err(LlmError::Quota("rate limited".into())),
            s if s >= 500 => return Err(LlmError::Provider(format!("server error: {s}"))),
            s if s >= 400 => {
                let text = resp.text().await.unwrap_or_default();
                return Err(LlmError::Provider(format!("{s}: {text}")));
            }
            _ => {}
        }

        struct State {
            response: reqwest::Response,
            buffer: String,
            pending: Vec<Result<LlmChunk, LlmError>>,
        }

        let state = State {
            response: resp,
            buffer: String::new(),
            pending: Vec::new(),
        };

        Ok(Box::pin(futures_util::stream::unfold(
            state,
            |mut state| async move {
                loop {
                    if let Some(chunk) = state.pending.pop() {
                        let done = chunk.as_ref().map(|c| c.finished).unwrap_or(false);
                        if done {
                            return Some((chunk, state));
                        }
                        return Some((chunk, state));
                    }

                    match state.response.chunk().await {
                        Ok(Some(bytes)) => {
                            state.buffer.push_str(&String::from_utf8_lossy(&bytes));
                            let mut chunks = parse_sse_chunks(&mut state.buffer);
                            chunks.reverse(); // so we can pop from the end
                            state.pending = chunks;
                        }
                        Ok(None) => return None,
                        Err(e) => {
                            return Some((Err(LlmError::Network(e.to_string())), state));
                        }
                    }
                }
            },
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_req() -> LlmRequest {
        LlmRequest {
            system: "You are helpful.".into(),
            user: "Hello".into(),
            max_tokens: Some(100),
            temperature: None,
        }
    }

    #[tokio::test]
    async fn test_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{"type": "text", "text": "Hello there!"}]
            })))
            .mount(&server)
            .await;

        let p = AnthropicProvider::new("sk-test".into()).with_base_url(server.uri());
        let resp = p.complete(&test_req()).await.unwrap();
        assert_eq!(resp.text, "Hello there!");
    }

    #[tokio::test]
    async fn test_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let p = AnthropicProvider::new("bad".into()).with_base_url(server.uri());
        let err = p.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Auth));
        assert!(err.should_failover());
    }

    #[tokio::test]
    async fn test_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let p = AnthropicProvider::new("sk".into()).with_base_url(server.uri());
        let err = p.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Quota(_)));
    }

    #[tokio::test]
    async fn test_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let p = AnthropicProvider::new("sk".into()).with_base_url(server.uri());
        let err = p.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Provider(_)));
    }

    #[tokio::test]
    async fn test_connection_error() {
        let p = AnthropicProvider::new("sk".into()).with_base_url("http://127.0.0.1:1");
        let err = p.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Network(_)));
        assert!(err.is_retryable());
    }

    #[tokio::test]
    async fn streaming_yields_multiple_chunks() {
        let server = MockServer::start().await;
        let sse_body = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"!\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(sse_body, "text/event-stream"))
            .mount(&server)
            .await;

        let p = AnthropicProvider::new("sk-test".into()).with_base_url(server.uri());
        let mut stream = p.complete_stream(&test_req()).await.unwrap();
        let mut text = String::new();
        let mut count = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            text.push_str(&chunk.text);
            count += 1;
            if chunk.finished {
                break;
            }
        }
        assert_eq!(text, "Hello world!");
        assert!(count >= 3);
    }

    #[tokio::test]
    async fn streaming_propagates_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let p = AnthropicProvider::new("bad".into()).with_base_url(server.uri());
        let err = p
            .complete_stream(&test_req())
            .await
            .err()
            .expect("expected error");
        assert!(matches!(err, LlmError::Auth));
    }

    #[tokio::test]
    async fn streaming_finished_chunk_terminates_stream() {
        let server = MockServer::start().await;
        let sse_body = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"done\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(sse_body, "text/event-stream"))
            .mount(&server)
            .await;

        let p = AnthropicProvider::new("sk-test".into()).with_base_url(server.uri());
        let mut stream = p.complete_stream(&test_req()).await.unwrap();
        let c1 = stream.next().await.unwrap().unwrap();
        assert_eq!(c1.text, "done");
        assert!(!c1.finished);
        let c2 = stream.next().await.unwrap().unwrap();
        assert!(c2.finished);
        // After finished, stream should end
        assert!(stream.next().await.is_none());
    }
}
