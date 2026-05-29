use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{LlmChunk, LlmChunkStream, LlmError, LlmProvider, LlmRequest, LlmResponse};

fn default_base_url() -> String {
    obfstr::obfstr!("https://api.openai.com").to_string()
}
const DEFAULT_MODEL: &str = "gpt-4o-mini";

pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenAiProvider {
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
struct ChatRequest {
    model: String,
    messages: Vec<ChatMsg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize)]
struct ChatMsg {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMsg,
}

#[derive(Deserialize)]
struct ChoiceMsg {
    content: Option<String>,
}

fn parse_openai_sse_chunks(buffer: &mut String) -> Vec<Result<LlmChunk, LlmError>> {
    let mut chunks = Vec::new();
    while let Some(pos) = buffer.find("\n\n") {
        let frame = buffer[..pos].to_string();
        *buffer = buffer[pos + 2..].to_string();
        for line in frame.lines() {
            let line = line.trim();
            if let Some(data) = line.strip_prefix("data: ") {
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }
                if data == "[DONE]" {
                    chunks.push(Ok(LlmChunk {
                        text: String::new(),
                        finished: true,
                        cost: None,
                        cost_label: None,
                        artifact: None,
                    }));
                    continue;
                }
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(delta) = parsed
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        if !delta.is_empty() {
                            chunks.push(Ok(LlmChunk {
                                text: delta.to_string(),
                                finished: false,
                                cost: None,
                                cost_label: None,
                                artifact: None,
                            }));
                        }
                    }
                }
            }
        }
    }
    chunks
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let body = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMsg {
                    role: "system".into(),
                    content: req.system.clone(),
                },
                ChatMsg {
                    role: "user".into(),
                    content: req.user.clone(),
                },
            ],
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: false,
        };

        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header(
                obfstr::obfstr!("Authorization"),
                format!("{} {}", obfstr::obfstr!("Bearer"), self.api_key),
            )
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

        let chat: ChatResponse = resp
            .json()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        let text = chat
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();

        Ok(LlmResponse {
            text,
            cost: None,
            cost_label: None,
            artifact: None,
        })
    }

    async fn complete_stream(&self, req: &LlmRequest) -> Result<LlmChunkStream, LlmError> {
        let body = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMsg {
                    role: "system".into(),
                    content: req.system.clone(),
                },
                ChatMsg {
                    role: "user".into(),
                    content: req.user.clone(),
                },
            ],
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: true,
        };

        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header(
                obfstr::obfstr!("Authorization"),
                format!("{} {}", obfstr::obfstr!("Bearer"), self.api_key),
            )
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
                        return Some((chunk, state));
                    }
                    match state.response.chunk().await {
                        Ok(Some(bytes)) => {
                            state.buffer.push_str(&String::from_utf8_lossy(&bytes));
                            let mut chunks = parse_openai_sse_chunks(&mut state.buffer);
                            chunks.reverse();
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
            reasoning_effort: None,
            thinking_budget_tokens: None,
            request_id: None,
        }
    }

    #[tokio::test]
    async fn test_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("Authorization", "Bearer sk-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"role": "assistant", "content": "Hi!"}}]
            })))
            .mount(&server)
            .await;

        let p = OpenAiProvider::new("sk-test".into()).with_base_url(server.uri());
        let resp = p.complete(&test_req()).await.unwrap();
        assert_eq!(resp.text, "Hi!");
    }

    #[tokio::test]
    async fn test_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let p = OpenAiProvider::new("bad".into()).with_base_url(server.uri());
        let err = p.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Auth));
    }

    #[tokio::test]
    async fn test_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let p = OpenAiProvider::new("sk".into()).with_base_url(server.uri());
        let err = p.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Quota(_)));
    }

    #[tokio::test]
    async fn test_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let p = OpenAiProvider::new("sk".into()).with_base_url(server.uri());
        let err = p.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Provider(_)));
    }

    #[tokio::test]
    async fn test_connection_error() {
        let p = OpenAiProvider::new("sk".into()).with_base_url("http://127.0.0.1:1");
        let err = p.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Network(_)));
    }

    #[tokio::test]
    async fn streaming_yields_multiple_chunks() {
        let server = MockServer::start().await;
        let sse_body = "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"!\"}}]}\n\ndata: [DONE]\n\n";
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(sse_body, "text/event-stream"))
            .mount(&server)
            .await;

        let p = OpenAiProvider::new("sk-test".into()).with_base_url(server.uri());
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
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let p = OpenAiProvider::new("bad".into()).with_base_url(server.uri());
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
        let sse_body =
            "data: {\"choices\":[{\"delta\":{\"content\":\"done\"}}]}\n\ndata: [DONE]\n\n";
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(sse_body, "text/event-stream"))
            .mount(&server)
            .await;

        let p = OpenAiProvider::new("sk-test".into()).with_base_url(server.uri());
        let mut stream = p.complete_stream(&test_req()).await.unwrap();
        let c1 = stream.next().await.unwrap().unwrap();
        assert_eq!(c1.text, "done");
        assert!(!c1.finished);
        let c2 = stream.next().await.unwrap().unwrap();
        assert!(c2.finished);
        assert!(stream.next().await.is_none());
    }
}
