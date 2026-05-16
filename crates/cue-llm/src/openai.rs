use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{LlmError, LlmProvider, LlmRequest, LlmResponse};

const DEFAULT_BASE_URL: &str = "https://api.openai.com";
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
            base_url: DEFAULT_BASE_URL.to_string(),
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

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
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
        };

        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
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

        Ok(LlmResponse { text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
