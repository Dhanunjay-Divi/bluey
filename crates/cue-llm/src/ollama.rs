use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{LlmError, LlmProvider, LlmRequest, LlmResponse};

const DEFAULT_BASE_URL: &str = "http://localhost:11434";
const DEFAULT_MODEL: &str = "llama3.2";

pub struct OllamaProvider {
    client: Client,
    model: String,
    base_url: String,
}

impl OllamaProvider {
    pub fn new() -> Self {
        let base_url =
            std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
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

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &'static str {
        "ollama"
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
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
}
