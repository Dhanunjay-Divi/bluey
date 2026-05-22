use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{LlmChunk, LlmChunkStream, LlmError, LlmProvider, LlmRequest, LlmResponse};

fn default_base_url() -> String {
    obfstr::obfstr!("http://localhost:11434").to_string()
}
const DEFAULT_MODEL: &str = "llama3.2";

pub struct OllamaProvider {
    client: Client,
    model: String,
    base_url: String,
}

impl OllamaProvider {
    pub fn new() -> Self {
        let base_url = std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| default_base_url());
        Self {
            client: Client::new(),
            model: DEFAULT_MODEL.to_string(),
            base_url,
        }
    }

    #[cfg(test)]
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMsg>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<Options>,
}

#[derive(Serialize)]
struct ChatMsg {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct Options {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ResponseMsg,
}

#[derive(Deserialize)]
struct ResponseMsg {
    content: String,
}

#[derive(Deserialize)]
struct StreamLine {
    message: Option<StreamMsg>,
    #[serde(default)]
    done: bool,
}

#[derive(Deserialize)]
struct StreamMsg {
    #[serde(default)]
    content: String,
}

fn parse_ndjson_chunks(buffer: &mut String) -> Vec<Result<LlmChunk, LlmError>> {
    let mut chunks = Vec::new();
    while let Some(pos) = buffer.find('\n') {
        let line = buffer[..pos].trim().to_string();
        *buffer = buffer[pos + 1..].to_string();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<StreamLine>(&line) {
            Ok(parsed) => {
                let text = parsed.message.map(|m| m.content).unwrap_or_default();
                chunks.push(Ok(LlmChunk {
                    text,
                    finished: parsed.done,
                    cost: None,
                    cost_label: None,
                    artifact: None,
                }));
            }
            Err(e) => {
                chunks.push(Err(LlmError::Provider(format!("parse error: {e}"))));
            }
        }
    }
    chunks
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &'static str {
        "ollama"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let options = if req.temperature.is_some() || req.max_tokens.is_some() {
            Some(Options {
                temperature: req.temperature,
                num_predict: req.max_tokens,
            })
        } else {
            None
        };

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
            stream: false,
            options,
        };

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        match resp.status().as_u16() {
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

        Ok(LlmResponse {
            text: chat.message.content,
            cost: None,
            cost_label: None,
            artifact: None,
        })
    }

    async fn complete_stream(&self, req: &LlmRequest) -> Result<LlmChunkStream, LlmError> {
        let options = if req.temperature.is_some() || req.max_tokens.is_some() {
            Some(Options {
                temperature: req.temperature,
                num_predict: req.max_tokens,
            })
        } else {
            None
        };

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
            stream: true,
            options,
        };

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        match resp.status().as_u16() {
            401 => return Err(LlmError::Auth),
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
                            let mut chunks = parse_ndjson_chunks(&mut state.buffer);
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
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_req() -> LlmRequest {
        LlmRequest {
            system: "You are helpful.".into(),
            user: "Hello".into(),
            max_tokens: Some(100),
            temperature: None,
            request_id: None,
        }
    }

    #[tokio::test]
    async fn test_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {"role": "assistant", "content": "Hey!"}
            })))
            .mount(&server)
            .await;

        let p = OllamaProvider::new().with_base_url(server.uri());
        let resp = p.complete(&test_req()).await.unwrap();
        assert_eq!(resp.text, "Hey!");
    }

    #[tokio::test]
    async fn test_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let p = OllamaProvider::new().with_base_url(server.uri());
        let err = p.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Provider(_)));
    }

    #[tokio::test]
    async fn test_bad_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(400).set_body_string("model not found"))
            .mount(&server)
            .await;

        let p = OllamaProvider::new().with_base_url(server.uri());
        let err = p.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Provider(_)));
    }

    #[tokio::test]
    async fn test_connection_error() {
        let p = OllamaProvider::new().with_base_url("http://127.0.0.1:1");
        let err = p.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Network(_)));
    }

    #[tokio::test]
    async fn test_default_model() {
        let p = OllamaProvider::new();
        assert_eq!(p.model, "llama3.2");
    }

    #[tokio::test]
    async fn streaming_yields_multiple_chunks() {
        let server = MockServer::start().await;
        let ndjson = "{\"message\":{\"content\":\"Hello\"},\"done\":false}\n{\"message\":{\"content\":\" world\"},\"done\":false}\n{\"message\":{\"content\":\"!\"},\"done\":false}\n{\"message\":{\"content\":\"\"},\"done\":true}\n";
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(ndjson, "application/x-ndjson"))
            .mount(&server)
            .await;

        let p = OllamaProvider::new().with_base_url(server.uri());
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
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let p = OllamaProvider::new().with_base_url(server.uri());
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
        let ndjson = "{\"message\":{\"content\":\"done\"},\"done\":false}\n{\"message\":{\"content\":\"\"},\"done\":true}\n";
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(ndjson, "application/x-ndjson"))
            .mount(&server)
            .await;

        let p = OllamaProvider::new().with_base_url(server.uri());
        let mut stream = p.complete_stream(&test_req()).await.unwrap();
        let c1 = stream.next().await.unwrap().unwrap();
        assert_eq!(c1.text, "done");
        assert!(!c1.finished);
        let c2 = stream.next().await.unwrap().unwrap();
        assert!(c2.finished);
        assert!(stream.next().await.is_none());
    }
}
