mod gmail;
mod google_calendar;
mod microsoft_calendar;
mod outlook;

use reqwest::{header::HeaderMap, StatusCode};
use serde_json::Value;

use crate::jobs_provider_auth::bounded_provider_json as bounded_json;

use super::contracts::{
    CommunicationProviderRequest, ProviderDispatchResult, ProviderFailureKind, ProviderLookupResult,
};

#[derive(Debug, Clone)]
pub(crate) struct ProviderEndpoints {
    gmail: String,
    google_calendar: String,
    microsoft_graph: String,
}

impl ProviderEndpoints {
    pub(crate) fn production() -> Self {
        Self {
            gmail: "https://gmail.googleapis.com".to_string(),
            google_calendar: "https://www.googleapis.com/calendar/v3".to_string(),
            microsoft_graph: "https://graph.microsoft.com/v1.0".to_string(),
        }
    }

    #[cfg(test)]
    pub(crate) fn fixture(base_url: &str) -> Self {
        let base = base_url.trim_end_matches('/').to_string();
        Self {
            gmail: base.clone(),
            google_calendar: base.clone(),
            microsoft_graph: base,
        }
    }
}

pub(crate) async fn dispatch(
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    access_token: &str,
    request: &CommunicationProviderRequest,
) -> ProviderDispatchResult {
    if !valid_operation_key(&request.provider_operation_key) || access_token.trim().is_empty() {
        return invalid_action();
    }
    match request.provider.as_str() {
        "gmail" => gmail::dispatch(client, endpoints, access_token, request).await,
        "outlook_email" => outlook::dispatch(client, endpoints, access_token, request).await,
        "google_calendar" => {
            google_calendar::dispatch(client, endpoints, access_token, request).await
        }
        "outlook_calendar" => {
            microsoft_calendar::dispatch(client, endpoints, access_token, request).await
        }
        _ => invalid_action(),
    }
}

pub(crate) async fn lookup(
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    access_token: &str,
    request: &CommunicationProviderRequest,
) -> ProviderLookupResult {
    if !valid_operation_key(&request.provider_operation_key) || access_token.trim().is_empty() {
        return ProviderLookupResult::Inconclusive {
            reason_code: "provider_lookup_invalid",
        };
    }
    match request.provider.as_str() {
        "gmail" => gmail::lookup(client, endpoints, access_token, request).await,
        "outlook_email" => outlook::lookup(client, endpoints, access_token, request).await,
        "google_calendar" => {
            google_calendar::lookup(client, endpoints, access_token, request).await
        }
        "outlook_calendar" => {
            microsoft_calendar::lookup(client, endpoints, access_token, request).await
        }
        _ => ProviderLookupResult::Inconclusive {
            reason_code: "provider_lookup_unsupported",
        },
    }
}

fn valid_operation_key(value: &str) -> bool {
    (16..=160).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn payload_string<'a>(payload: &'a Value, key: &str, max_chars: usize) -> Option<&'a str> {
    let value = payload.get(key)?.as_str()?;
    if value.is_empty()
        || value.chars().count() > max_chars
        || value.chars().any(|character| character == '\0')
    {
        return None;
    }
    Some(value)
}

fn header_text(value: &str, max_chars: usize) -> Option<String> {
    if value.is_empty()
        || value.chars().count() > max_chars
        || value
            .chars()
            .any(|character| character == '\r' || character == '\n' || character == '\0')
        || !crate::db::jobs::communication_review_text_is_safe(value)
    {
        return None;
    }
    Some(value.to_string())
}

fn normalized_email(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized != value
        || normalized.len() > 320
        || !crate::db::jobs::communication_review_text_is_safe(&normalized)
        || normalized
            .chars()
            .any(|character| matches!(character, '\r' | '\n' | '\0'))
        || normalized.chars().any(char::is_whitespace)
        || normalized.matches('@').count() != 1
        || normalized.starts_with('@')
        || normalized.ends_with('@')
    {
        return None;
    }
    Some(normalized)
}

fn provider_url_with_segments(base: &str, segments: &[&str]) -> Option<reqwest::Url> {
    let mut url = reqwest::Url::parse(base).ok()?;
    if url.query().is_some() || url.fragment().is_some() {
        return None;
    }
    url.path_segments_mut()
        .ok()?
        .extend(segments.iter().copied());
    Some(url)
}

fn provider_identifier(value: &str) -> Option<&str> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 2_048
        || value.chars().any(char::is_control)
    {
        return None;
    }
    Some(value)
}

fn classify_write_status(status: StatusCode, headers: &HeaderMap) -> ProviderDispatchResult {
    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return ProviderDispatchResult::DefinitiveNoSideEffect {
            kind: ProviderFailureKind::Authorization,
            retry_after_ms: None,
        };
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return ProviderDispatchResult::DefinitiveNoSideEffect {
            kind: ProviderFailureKind::RetryableNoSideEffect,
            retry_after_ms: retry_after_ms(headers),
        };
    }
    if status.is_client_error()
        && !matches!(
            status,
            StatusCode::REQUEST_TIMEOUT | StatusCode::CONFLICT | StatusCode::TOO_EARLY
        )
    {
        return ProviderDispatchResult::DefinitiveNoSideEffect {
            kind: ProviderFailureKind::ProviderRejected,
            retry_after_ms: None,
        };
    }
    ProviderDispatchResult::Ambiguous {
        reason_code: "provider_write_outcome_unknown",
    }
}

fn classify_no_side_effect_status(
    status: StatusCode,
    headers: &HeaderMap,
) -> ProviderDispatchResult {
    let kind = if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        ProviderFailureKind::Authorization
    } else if status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
        || matches!(status, StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_EARLY)
    {
        ProviderFailureKind::RetryableNoSideEffect
    } else {
        ProviderFailureKind::ProviderRejected
    };
    ProviderDispatchResult::DefinitiveNoSideEffect {
        kind,
        retry_after_ms: retry_after_ms(headers),
    }
}

fn retry_after_ms(headers: &HeaderMap) -> Option<i64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .map(|seconds| seconds.clamp(1, 3_600).saturating_mul(1_000))
}

fn invalid_action() -> ProviderDispatchResult {
    ProviderDispatchResult::DefinitiveNoSideEffect {
        kind: ProviderFailureKind::InvalidAction,
        retry_after_ms: None,
    }
}

fn lookup_http_failure(status: StatusCode) -> ProviderLookupResult {
    ProviderLookupResult::Inconclusive {
        reason_code: if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            "provider_lookup_authorization_required"
        } else {
            "provider_lookup_unavailable"
        },
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use serde_json::json;
    use wiremock::{
        matchers::{method, path, path_regex, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    use super::*;
    use crate::jobs_communication_dispatch::contracts::CommunicationSourceMessage;

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap()
    }

    fn reply_request(provider: &str) -> CommunicationProviderRequest {
        CommunicationProviderRequest {
            action_id: "action-fixture-1".to_string(),
            provider: provider.to_string(),
            provider_operation_key: "bluey-operation-fixture-1234".to_string(),
            payload_sha256: "a".repeat(64),
            payload: json!({
                "to": "recruiter@example.com",
                "subject": "Exact reviewed subject",
                "body_text": " leading body byte\ntrailing body byte ",
            }),
            source_message: Some(CommunicationSourceMessage {
                provider: if provider == "gmail" {
                    "gmail"
                } else {
                    "outlook"
                }
                .to_string(),
                provider_id: if provider == "gmail" {
                    "gmail-source-1"
                } else {
                    "opaque/id?value"
                }
                .to_string(),
                external_id: "<source-message@example.com>".to_string(),
                rfc_message_id: "<source-message@example.com>".to_string(),
                thread_id: "thread-1".to_string(),
                conversation_id: "conversation-1".to_string(),
                sender: "recruiter@example.com".to_string(),
                reply_target: "recruiter@example.com".to_string(),
                subject: "Original subject".to_string(),
            }),
        }
    }

    fn calendar_request(provider: &str, time_zone: &str) -> CommunicationProviderRequest {
        CommunicationProviderRequest {
            action_id: "action-fixture-calendar".to_string(),
            provider: provider.to_string(),
            provider_operation_key: "bluey-calendar-fixture-1234".to_string(),
            payload_sha256: "b".repeat(64),
            payload: json!({
                "title": "Exact reviewed interview",
                "starts_at_ms": 1_700_000_000_000_i64,
                "ends_at_ms": 1_700_003_600_000_i64,
                "attendees": ["candidate@example.com"],
                "time_zone": time_zone,
            }),
            source_message: None,
        }
    }

    #[tokio::test]
    async fn gmail_reply_preserves_reviewed_text_and_requires_exact_thread_proof() {
        let server = MockServer::start().await;
        let request = reply_request("gmail");
        let operation_message_id = gmail::operation_message_id(&request.provider_operation_key);
        Mock::given(method("POST"))
            .and(path("/gmail/v1/users/me/messages/send"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "sent-message-1",
                "threadId": "thread-1",
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/gmail/v1/users/me/messages/sent-message-1"))
            .and(query_param("format", "metadata"))
            .and(query_param("metadataHeaders", "Message-ID"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "sent-message-1",
                "threadId": "thread-1",
                "payload": {"headers": [{
                    "name": "Message-ID",
                    "value": operation_message_id,
                }]},
            })))
            .expect(1)
            .mount(&server)
            .await;
        let result = dispatch(
            &client(),
            &ProviderEndpoints::fixture(&server.uri()),
            "fixture-token",
            &request,
        )
        .await;
        assert!(matches!(result, ProviderDispatchResult::Committed(_)));

        let requests = server.received_requests().await.unwrap();
        let post = requests
            .iter()
            .find(|request| request.method.as_str() == "POST")
            .unwrap();
        let payload: Value = serde_json::from_slice(&post.body).unwrap();
        assert_eq!(payload["threadId"], "thread-1");
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload["raw"].as_str().unwrap())
            .unwrap();
        let mime = String::from_utf8(raw).unwrap();
        assert!(mime.contains("Subject: Exact reviewed subject\r\n"));
        assert!(mime.ends_with(" leading body byte\r\ntrailing body byte "));

        let mut invalid = request;
        invalid.payload["subject"] = Value::String("unsafe\r\nBcc: other@example.com".to_string());
        let rejected = dispatch(
            &client(),
            &ProviderEndpoints::fixture(&server.uri()),
            "fixture-token",
            &invalid,
        )
        .await;
        assert!(matches!(
            rejected,
            ProviderDispatchResult::DefinitiveNoSideEffect {
                kind: ProviderFailureKind::InvalidAction,
                ..
            }
        ));
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn gmail_reply_requires_readback_of_its_exact_operation_message_id() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/gmail/v1/users/me/messages/send"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "sent-message-tampered",
                "threadId": "thread-1",
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/gmail/v1/users/me/messages/sent-message-tampered"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "sent-message-tampered",
                "threadId": "thread-1",
                "payload": {"headers": [{
                    "name": "Message-ID",
                    "value": "<wrong-operation@actions.bluey.sh>",
                }]},
            })))
            .mount(&server)
            .await;
        let result = dispatch(
            &client(),
            &ProviderEndpoints::fixture(&server.uri()),
            "fixture-token",
            &reply_request("gmail"),
        )
        .await;
        assert!(matches!(
            result,
            ProviderDispatchResult::Ambiguous {
                reason_code: "gmail_send_proof_mismatch"
            }
        ));
    }

    #[tokio::test]
    async fn outlook_mime_reply_encodes_opaque_path_and_requires_sent_item_proof() {
        let server = MockServer::start().await;
        let request = reply_request("outlook_email");
        let operation_message_id = outlook::operation_message_id(&request.provider_operation_key);
        Mock::given(method("GET"))
            .and(path("/me/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "value": [{
                    "id": "immutable/moved?id",
                    "internetMessageId": "<source-message@example.com>",
                    "conversationId": "conversation-1",
                    "from": {"emailAddress": {"address": "recruiter@example.com"}},
                    "replyTo": [{"emailAddress": {"address": "recruiter@example.com"}}],
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"^/me/messages/[^/]+/reply$"))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/me/mailFolders/sentitems/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "value": [{
                    "id": "sent-immutable-1",
                    "internetMessageId": operation_message_id,
                    "conversationId": "conversation-1",
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let result = dispatch(
            &client(),
            &ProviderEndpoints::fixture(&server.uri()),
            "fixture-token",
            &request,
        )
        .await;
        assert!(matches!(result, ProviderDispatchResult::Committed(_)));

        let encoded_url = provider_url_with_segments(
            "https://graph.microsoft.com/v1.0",
            &["me", "messages", "opaque/id?value", "reply"],
        )
        .unwrap();
        assert_eq!(
            encoded_url.path(),
            "/v1.0/me/messages/opaque%2Fid%3Fvalue/reply"
        );
        let requests = server.received_requests().await.unwrap();
        let post = requests
            .iter()
            .find(|request| request.method.as_str() == "POST")
            .unwrap();
        assert!(post.url.path().contains("immutable%2Fmoved%3Fid"));
        let mime = base64::engine::general_purpose::STANDARD
            .decode(&post.body)
            .unwrap();
        let mime = String::from_utf8(mime).unwrap();
        assert!(mime.contains("To: recruiter@example.com\r\n"));
        assert!(mime.contains("Subject: Exact reviewed subject\r\n"));
        assert!(mime.contains("In-Reply-To: <source-message@example.com>\r\n"));
        assert!(mime.ends_with(" leading body byte\r\ntrailing body byte "));
    }

    #[tokio::test]
    async fn outlook_accepted_without_proof_is_unknown_and_exact_duplicates_conflict() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/me/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "value": [{
                    "id": "immutable-source-1",
                    "internetMessageId": "<source-message@example.com>",
                    "conversationId": "conversation-1",
                    "from": {"emailAddress": {"address": "recruiter@example.com"}},
                    "replyTo": [],
                }]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"^/me/messages/[^/]+/reply$"))
            .respond_with(ResponseTemplate::new(202))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/me/mailFolders/sentitems/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "value": [] })))
            .mount(&server)
            .await;
        let request = reply_request("outlook_email");
        let result = dispatch(
            &client(),
            &ProviderEndpoints::fixture(&server.uri()),
            "fixture-token",
            &request,
        )
        .await;
        assert!(matches!(result, ProviderDispatchResult::Ambiguous { .. }));

        let conflict_server = MockServer::start().await;
        let marker = outlook::operation_message_id(&request.provider_operation_key);
        Mock::given(method("GET"))
            .and(path("/me/mailFolders/sentitems/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "value": [
                    {"id":"one","internetMessageId":marker,"conversationId":"conversation-1"},
                    {"id":"two","internetMessageId":marker,"conversationId":"conversation-1"}
                ]
            })))
            .mount(&conflict_server)
            .await;
        let lookup_result = lookup(
            &client(),
            &ProviderEndpoints::fixture(&conflict_server.uri()),
            "fixture-token",
            &request,
        )
        .await;
        assert_eq!(lookup_result, ProviderLookupResult::Conflict);

        let transport_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/me/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "value": [{
                    "id": "immutable-source-1",
                    "internetMessageId": "<source-message@example.com>",
                    "conversationId": "conversation-1",
                    "from": {"emailAddress": {"address": "recruiter@example.com"}},
                    "replyTo": [],
                }]
            })))
            .mount(&transport_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/me/messages/immutable-source-1/reply"))
            .respond_with(ResponseTemplate::new(202).set_delay(std::time::Duration::from_secs(3)))
            .mount(&transport_server)
            .await;
        let transport_loss = dispatch(
            &client(),
            &ProviderEndpoints::fixture(&transport_server.uri()),
            "fixture-token",
            &request,
        )
        .await;
        assert!(matches!(
            transport_loss,
            ProviderDispatchResult::Ambiguous {
                reason_code: "outlook_reply_transport_unknown"
            }
        ));

        let reply_to_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/me/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "value": [{
                    "id": "immutable-source-1",
                    "internetMessageId": "<source-message@example.com>",
                    "conversationId": "conversation-1",
                    "from": {"emailAddress": {"address": "recruiter@example.com"}},
                    "replyTo": [{"emailAddress": {"address": "ats@example.net"}}],
                }]
            })))
            .mount(&reply_to_server)
            .await;
        let blocked_reply_to = dispatch(
            &client(),
            &ProviderEndpoints::fixture(&reply_to_server.uri()),
            "fixture-token",
            &request,
        )
        .await;
        assert!(matches!(
            blocked_reply_to,
            ProviderDispatchResult::DefinitiveNoSideEffect {
                kind: ProviderFailureKind::InvalidAction,
                ..
            }
        ));
        assert_eq!(reply_to_server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn google_calendar_notifies_attendees_preserves_zone_and_resolves_exact_conflict() {
        let server = MockServer::start().await;
        let request = calendar_request("google_calendar", "America/New_York");
        let event_id = google_calendar::operation_event_id(&request.provider_operation_key);
        Mock::given(method("POST"))
            .and(path("/calendars/primary/events"))
            .and(query_param("sendUpdates", "all"))
            .respond_with(ResponseTemplate::new(409))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/calendars/primary/events/{event_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": event_id,
                "extendedProperties": {
                    "private": {"blueyActionKey": request.provider_operation_key}
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        let result = dispatch(
            &client(),
            &ProviderEndpoints::fixture(&server.uri()),
            "fixture-token",
            &request,
        )
        .await;
        assert!(matches!(result, ProviderDispatchResult::Committed(_)));
        let requests = server.received_requests().await.unwrap();
        let post = requests
            .iter()
            .find(|request| request.method.as_str() == "POST")
            .unwrap();
        let body: Value = serde_json::from_slice(&post.body).unwrap();
        assert_eq!(body["summary"], "Exact reviewed interview");
        assert_eq!(body["start"]["timeZone"], "America/New_York");
        assert!(body["start"]["dateTime"]
            .as_str()
            .unwrap()
            .ends_with("-05:00"));
        assert_eq!(body["attendees"][0]["email"], "candidate@example.com");
    }

    #[tokio::test]
    async fn google_calendar_without_attendees_suppresses_notifications() {
        let server = MockServer::start().await;
        let mut request = calendar_request("google_calendar", "UTC");
        request.payload["attendees"] = json!([]);
        let event_id = google_calendar::operation_event_id(&request.provider_operation_key);
        Mock::given(method("POST"))
            .and(path("/calendars/primary/events"))
            .and(query_param("sendUpdates", "none"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": event_id,
                "extendedProperties": {
                    "private": {"blueyActionKey": request.provider_operation_key}
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        let result = dispatch(
            &client(),
            &ProviderEndpoints::fixture(&server.uri()),
            "fixture-token",
            &request,
        )
        .await;
        assert!(matches!(result, ProviderDispatchResult::Committed(_)));
    }

    #[tokio::test]
    async fn microsoft_calendar_preflights_iana_zone_before_exact_write_proof() {
        let server = MockServer::start().await;
        let request = calendar_request("outlook_calendar", "America/New_York");
        let transaction_id =
            microsoft_calendar::operation_transaction_id(&request.provider_operation_key);
        Mock::given(method("GET"))
            .and(path_regex(r"^/me/outlook/supportedTimeZones.*$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "value": [{"alias": "America/New_York"}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/me/events"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id":"event-1"})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/me/events/event-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "event-1",
                "transactionId": transaction_id,
                "singleValueExtendedProperties": [{
                    "id": microsoft_calendar::ACTION_PROPERTY_ID,
                    "value": request.provider_operation_key,
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let result = dispatch(
            &client(),
            &ProviderEndpoints::fixture(&server.uri()),
            "fixture-token",
            &request,
        )
        .await;
        assert!(matches!(result, ProviderDispatchResult::Committed(_)));
        let requests = server.received_requests().await.unwrap();
        let post = requests
            .iter()
            .find(|request| request.method.as_str() == "POST")
            .unwrap();
        let body: Value = serde_json::from_slice(&post.body).unwrap();
        assert_eq!(body["start"]["timeZone"], "America/New_York");
        assert_eq!(body["transactionId"], transaction_id);
        assert_eq!(
            body["attendees"][0]["emailAddress"]["address"],
            "candidate@example.com"
        );

        let unsupported = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(r"^/me/outlook/supportedTimeZones.*$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "value": [{"alias": "UTC"}]
            })))
            .mount(&unsupported)
            .await;
        let rejected = dispatch(
            &client(),
            &ProviderEndpoints::fixture(&unsupported.uri()),
            "fixture-token",
            &request,
        )
        .await;
        assert!(matches!(
            rejected,
            ProviderDispatchResult::DefinitiveNoSideEffect {
                kind: ProviderFailureKind::InvalidAction,
                ..
            }
        ));
        assert_eq!(unsupported.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn oversized_provider_success_body_is_never_committed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/gmail/v1/users/me/messages/send"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 256 * 1024 + 1]))
            .mount(&server)
            .await;
        let result = dispatch(
            &client(),
            &ProviderEndpoints::fixture(&server.uri()),
            "fixture-token",
            &reply_request("gmail"),
        )
        .await;
        assert!(matches!(result, ProviderDispatchResult::Ambiguous { .. }));
    }
}
