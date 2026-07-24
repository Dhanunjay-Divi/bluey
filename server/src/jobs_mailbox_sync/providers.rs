use crate::{
    api::jobs_mailbox_oauth::ProviderConfig,
    db::jobs::{JobsProviderCredential, JobsProviderMessage},
};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_MESSAGES_PER_SYNC: usize = 200;
const MAX_BODY_CHARS: usize = 24_000;

#[derive(Debug)]
pub(crate) struct RefreshResult {
    pub access_token: String,
    pub refresh_token: String,
    pub scopes: Vec<String>,
    pub expires_in_seconds: i64,
}

#[derive(Debug)]
pub(crate) struct FetchedMessages {
    pub messages: Vec<ProviderMessage>,
    pub cursor: Value,
}

#[derive(Debug)]
pub(crate) struct ProviderMessage {
    pub external_id: String,
    pub sender: String,
    pub recipients: Vec<String>,
    pub subject: String,
    pub body_text: String,
    pub received_at_ms: i64,
    pub metadata: Value,
}

impl ProviderMessage {
    pub fn into_stored(self, connection_id: &str, provider: &str) -> JobsProviderMessage {
        JobsProviderMessage {
            id: String::new(),
            connection_id: connection_id.to_string(),
            provider: provider.to_string(),
            external_id: self.external_id,
            sender: self.sender,
            recipients: self.recipients,
            subject: self.subject,
            body_text: self.body_text,
            received_at_ms: self.received_at_ms,
            application_id: None,
            processing_status: "received".to_string(),
            classification: String::new(),
            confidence: 0.0,
            metadata: self.metadata,
            processed_at_ms: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_in: i64,
    #[serde(default)]
    scope: String,
}

pub(crate) async fn refresh_access_token(
    client: &reqwest::Client,
    config: &ProviderConfig,
    refresh_token: &str,
) -> anyhow::Result<RefreshResult> {
    if refresh_token.trim().is_empty() {
        anyhow::bail!("mailbox refresh token is missing")
    }
    let response = client
        .post(&config.token_url)
        .form(&[
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
            ("scope", config.scopes.join(" ").as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<TokenResponse>()
        .await?;
    if response.access_token.trim().is_empty() {
        anyhow::bail!("provider refresh returned no access token")
    }
    Ok(RefreshResult {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        scopes: response
            .scope
            .split_whitespace()
            .map(str::to_string)
            .collect(),
        expires_in_seconds: response.expires_in,
    })
}

pub(crate) async fn fetch_messages(
    client: &reqwest::Client,
    config: &ProviderConfig,
    credential: &JobsProviderCredential,
    last_synced_at_ms: Option<i64>,
    cursor: &Value,
) -> anyhow::Result<FetchedMessages> {
    match config.provider {
        "gmail" => fetch_gmail_messages(client, credential, last_synced_at_ms).await,
        "outlook" => fetch_outlook_messages(client, credential, cursor).await,
        _ => anyhow::bail!("unsupported mailbox provider"),
    }
}

async fn fetch_gmail_messages(
    client: &reqwest::Client,
    credential: &JobsProviderCredential,
    last_synced_at_ms: Option<i64>,
) -> anyhow::Result<FetchedMessages> {
    let after_seconds = last_synced_at_ms
        .map(|value| value.saturating_sub(5 * 60 * 1_000) / 1_000)
        .unwrap_or_else(|| chrono::Utc::now().timestamp() - 90 * 24 * 60 * 60);
    let query = format!("in:inbox after:{}", after_seconds.max(0));
    let mut page_token: Option<String> = None;
    let mut ids = Vec::new();
    while ids.len() < MAX_MESSAGES_PER_SYNC {
        let mut url =
            reqwest::Url::parse("https://gmail.googleapis.com/gmail/v1/users/me/messages")?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs
                .append_pair("q", &query)
                .append_pair("maxResults", "50");
            if let Some(token) = page_token.as_deref() {
                pairs.append_pair("pageToken", token);
            }
        }
        let page = client
            .get(url)
            .bearer_auth(&credential.access_token)
            .send()
            .await?
            .error_for_status()?
            .json::<GmailListResponse>()
            .await?;
        ids.extend(page.messages.into_iter().map(|item| item.id));
        page_token = page.next_page_token;
        if page_token.is_none() {
            break;
        }
    }
    ids.truncate(MAX_MESSAGES_PER_SYNC);

    let mut messages = Vec::with_capacity(ids.len());
    for id in ids {
        let url =
            format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{id}?format=full");
        let message = client
            .get(url)
            .bearer_auth(&credential.access_token)
            .send()
            .await?
            .error_for_status()?
            .json::<GmailMessage>()
            .await?;
        messages.push(parse_gmail_message(message)?);
    }
    Ok(FetchedMessages {
        messages,
        cursor: json!({ "after_ms": chrono::Utc::now().timestamp_millis() }),
    })
}

#[derive(Deserialize)]
struct GmailListResponse {
    #[serde(default)]
    messages: Vec<GmailListItem>,
    #[serde(rename = "nextPageToken", default)]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct GmailListItem {
    id: String,
}

#[derive(Deserialize)]
struct GmailMessage {
    id: String,
    #[serde(rename = "threadId", default)]
    thread_id: String,
    #[serde(rename = "internalDate")]
    internal_date: String,
    payload: GmailPart,
}

#[derive(Deserialize)]
struct GmailPart {
    #[serde(rename = "mimeType", default)]
    mime_type: String,
    #[serde(default)]
    headers: Vec<GmailHeader>,
    #[serde(default)]
    body: GmailBody,
    #[serde(default)]
    parts: Vec<GmailPart>,
}

#[derive(Deserialize)]
struct GmailHeader {
    name: String,
    value: String,
}

#[derive(Default, Deserialize)]
struct GmailBody {
    #[serde(default)]
    data: String,
}

fn parse_gmail_message(message: GmailMessage) -> anyhow::Result<ProviderMessage> {
    let sender = header_value(&message.payload.headers, "from");
    let recipients = split_addresses(&header_value(&message.payload.headers, "to"));
    let subject = header_value(&message.payload.headers, "subject");
    let mut plain = String::new();
    let mut html = String::new();
    collect_gmail_body(&message.payload, &mut plain, &mut html);
    let body_text = if plain.trim().is_empty() {
        strip_html(&html)
    } else {
        plain
    };
    Ok(ProviderMessage {
        external_id: message.id,
        sender: normalize_address(&sender),
        recipients,
        subject: trim_text(&subject, 500),
        body_text: trim_text(&body_text, MAX_BODY_CHARS),
        received_at_ms: message.internal_date.parse::<i64>()?,
        metadata: json!({ "thread_id": message.thread_id }),
    })
}

fn collect_gmail_body(part: &GmailPart, plain: &mut String, html: &mut String) {
    if !part.body.data.is_empty() {
        if let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&part.body.data)
        {
            let text = String::from_utf8_lossy(&bytes);
            if part.mime_type.eq_ignore_ascii_case("text/plain") {
                plain.push_str(&text);
                plain.push('\n');
            } else if part.mime_type.eq_ignore_ascii_case("text/html") {
                html.push_str(&text);
                html.push('\n');
            }
        }
    }
    for child in &part.parts {
        collect_gmail_body(child, plain, html);
    }
}

fn header_value(headers: &[GmailHeader], name: &str) -> String {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.clone())
        .unwrap_or_default()
}

async fn fetch_outlook_messages(
    client: &reqwest::Client,
    credential: &JobsProviderCredential,
    cursor: &Value,
) -> anyhow::Result<FetchedMessages> {
    let mut next = cursor
        .get("delta_link")
        .and_then(Value::as_str)
        .filter(|value| valid_outlook_delta_url(value))
        .map(str::to_string)
        .unwrap_or_else(|| {
            "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages/delta\
             ?$select=id,internetMessageId,conversationId,from,toRecipients,subject,bodyPreview,receivedDateTime\
             &$top=50"
                .replace(' ', "")
        });
    let mut messages = Vec::new();
    let mut final_cursor = cursor.clone();
    for _ in 0..4 {
        if !valid_outlook_delta_url(&next) {
            anyhow::bail!("invalid Outlook continuation URL")
        }
        let page = client
            .get(&next)
            .bearer_auth(&credential.access_token)
            .send()
            .await?
            .error_for_status()?
            .json::<OutlookDeltaResponse>()
            .await?;
        for value in page.value {
            if messages.len() >= MAX_MESSAGES_PER_SYNC {
                break;
            }
            messages.push(parse_outlook_message(value)?);
        }
        if let Some(delta) = page.delta_link {
            if !valid_outlook_delta_url(&delta) {
                anyhow::bail!("invalid Outlook delta URL")
            }
            final_cursor = json!({ "delta_link": delta });
            break;
        }
        let Some(next_link) = page.next_link else {
            final_cursor = json!({});
            break;
        };
        next = next_link;
        final_cursor = json!({ "delta_link": next });
        if messages.len() >= MAX_MESSAGES_PER_SYNC {
            break;
        }
    }
    Ok(FetchedMessages {
        messages,
        cursor: final_cursor,
    })
}

#[derive(Deserialize)]
struct OutlookDeltaResponse {
    #[serde(default)]
    value: Vec<OutlookMessage>,
    #[serde(rename = "@odata.nextLink", default)]
    next_link: Option<String>,
    #[serde(rename = "@odata.deltaLink", default)]
    delta_link: Option<String>,
}

#[derive(Deserialize)]
struct OutlookMessage {
    id: String,
    #[serde(rename = "internetMessageId", default)]
    internet_message_id: String,
    #[serde(rename = "conversationId", default)]
    conversation_id: String,
    #[serde(default)]
    from: Option<OutlookRecipient>,
    #[serde(rename = "toRecipients", default)]
    to_recipients: Vec<OutlookRecipient>,
    #[serde(default)]
    subject: String,
    #[serde(rename = "bodyPreview", default)]
    body_preview: String,
    #[serde(rename = "receivedDateTime")]
    received_date_time: String,
}

#[derive(Deserialize)]
struct OutlookRecipient {
    #[serde(rename = "emailAddress")]
    email_address: OutlookEmailAddress,
}

#[derive(Deserialize)]
struct OutlookEmailAddress {
    address: String,
}

fn parse_outlook_message(message: OutlookMessage) -> anyhow::Result<ProviderMessage> {
    let external_id = if message.internet_message_id.trim().is_empty() {
        message.id.clone()
    } else {
        message.internet_message_id
    };
    let sender = message
        .from
        .map(|value| value.email_address.address)
        .unwrap_or_default();
    let recipients = message
        .to_recipients
        .into_iter()
        .map(|value| value.email_address.address)
        .collect();
    let received_at_ms =
        chrono::DateTime::parse_from_rfc3339(&message.received_date_time)?.timestamp_millis();
    Ok(ProviderMessage {
        external_id,
        sender: normalize_address(&sender),
        recipients,
        subject: trim_text(&message.subject, 500),
        body_text: trim_text(&message.body_preview, MAX_BODY_CHARS),
        received_at_ms,
        metadata: json!({
            "provider_id": message.id,
            "conversation_id": message.conversation_id
        }),
    })
}

pub(crate) fn valid_outlook_delta_url(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("graph.microsoft.com")
            && url
                .path()
                .starts_with("/v1.0/me/mailFolders/inbox/messages/delta")
    })
}

fn split_addresses(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(normalize_address)
        .filter(|value| !value.is_empty())
        .collect()
}

fn normalize_address(value: &str) -> String {
    let candidate = value
        .rsplit_once('<')
        .and_then(|(_, tail)| tail.split_once('>').map(|(email, _)| email))
        .unwrap_or(value)
        .trim()
        .to_ascii_lowercase();
    if candidate.contains('@') {
        candidate
    } else {
        String::new()
    }
}

fn trim_text(value: &str, limit: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

fn strip_html(value: &str) -> String {
    let mut text = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outlook_continuation_is_host_and_path_pinned() {
        assert!(valid_outlook_delta_url(
            "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages/delta?$skiptoken=a"
        ));
        assert!(!valid_outlook_delta_url(
            "https://example.com/v1.0/me/mailFolders/inbox/messages/delta"
        ));
        assert!(!valid_outlook_delta_url(
            "http://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages/delta"
        ));
        assert!(!valid_outlook_delta_url(
            "https://graph.microsoft.com/v1.0/users/other/messages/delta"
        ));
    }

    #[test]
    fn html_fallback_is_reduced_to_readable_text() {
        assert_eq!(
            trim_text(&strip_html("<p>Hello <b>candidate</b></p>"), 100),
            "Hello candidate"
        );
    }
}
