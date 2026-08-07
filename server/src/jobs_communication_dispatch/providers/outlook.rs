use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{
    bounded_json, classify_no_side_effect_status, classify_write_status, header_text,
    invalid_action, lookup_http_failure, normalized_email, payload_string, provider_identifier,
    provider_url_with_segments, CommunicationProviderRequest, ProviderDispatchResult,
    ProviderEndpoints, ProviderLookupResult,
};
use crate::jobs_communication_dispatch::contracts::{ProviderCommitEvidence, ProviderFailureKind};

#[derive(Deserialize)]
struct OutlookMessage {
    #[serde(default)]
    id: String,
    #[serde(rename = "internetMessageId", default)]
    internet_message_id: String,
    #[serde(rename = "conversationId", default)]
    conversation_id: String,
    #[serde(default)]
    from: Option<OutlookRecipient>,
    #[serde(rename = "replyTo", default)]
    reply_to: Vec<OutlookRecipient>,
}

#[derive(Deserialize)]
struct OutlookMessageList {
    #[serde(default)]
    value: Vec<OutlookMessage>,
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

pub(super) async fn dispatch(
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    access_token: &str,
    request: &CommunicationProviderRequest,
) -> ProviderDispatchResult {
    let Some(source) = request.source_message.as_ref() else {
        return invalid_action();
    };
    if source.provider != "outlook"
        || provider_identifier(&source.provider_id).is_none()
        || provider_identifier(&source.conversation_id).is_none()
    {
        return invalid_action();
    }
    let Some(to) = payload_string(&request.payload, "to", 320).and_then(normalized_email) else {
        return invalid_action();
    };
    if !source.reply_target.is_empty()
        && normalized_email(&source.reply_target).as_deref() != Some(to.as_str())
    {
        return invalid_action();
    }
    let Some(subject) =
        payload_string(&request.payload, "subject", 998).and_then(|value| header_text(value, 998))
    else {
        return invalid_action();
    };
    let Some(parent_message_id) = header_text(&source.rfc_message_id, 998) else {
        return invalid_action();
    };
    let Some(body) = payload_string(&request.payload, "body_text", 32_000)
        .filter(|value| crate::db::jobs::communication_body_text_is_safe(value))
    else {
        return invalid_action();
    };
    let immutable_source_id = match resolve_immutable_source(
        client,
        endpoints,
        access_token,
        source,
        &parent_message_id,
        &to,
    )
    .await
    {
        Ok(id) => id,
        Err(result) => return result,
    };
    let operation_message_id = operation_message_id(&request.provider_operation_key);
    let body = body
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n");
    let mime = format!(
        "To: {to}\r\nSubject: {subject}\r\nMessage-ID: {operation_message_id}\r\n\
         In-Reply-To: {parent_message_id}\r\nReferences: {parent_message_id}\r\n\
         MIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\n\
         Content-Transfer-Encoding: 8bit\r\n\r\n{body}"
    );
    let encoded = base64::engine::general_purpose::STANDARD.encode(mime.as_bytes());
    let Some(reply_url) = provider_url_with_segments(
        &endpoints.microsoft_graph,
        &["me", "messages", &immutable_source_id, "reply"],
    ) else {
        return invalid_action();
    };
    let response = match client
        .post(reply_url)
        .header(reqwest::header::CONTENT_TYPE, "text/plain")
        .header("Prefer", "IdType=\"ImmutableId\"")
        .bearer_auth(access_token)
        .body(encoded)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return ProviderDispatchResult::Ambiguous {
                reason_code: "outlook_reply_transport_unknown",
            }
        }
    };
    if !response.status().is_success() {
        return classify_write_status(response.status(), response.headers());
    }
    match lookup(client, endpoints, access_token, request).await {
        ProviderLookupResult::Found(evidence) => ProviderDispatchResult::Committed(evidence),
        _ => ProviderDispatchResult::Ambiguous {
            reason_code: "outlook_reply_send_proof_unavailable",
        },
    }
}

async fn resolve_immutable_source(
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    access_token: &str,
    source: &crate::jobs_communication_dispatch::contracts::CommunicationSourceMessage,
    internet_message_id: &str,
    approved_to: &str,
) -> Result<String, ProviderDispatchResult> {
    let escaped_message_id = internet_message_id.replace('\'', "''");
    let filter = format!("internetMessageId eq '{escaped_message_id}'");
    let mut url = reqwest::Url::parse(&format!("{}/me/messages", endpoints.microsoft_graph))
        .map_err(|_| invalid_action())?;
    url.query_pairs_mut()
        .append_pair("$filter", &filter)
        .append_pair(
            "$select",
            "id,internetMessageId,conversationId,from,replyTo",
        )
        .append_pair("$top", "2");
    let response = client
        .get(url)
        .header("Prefer", "IdType=\"ImmutableId\"")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|_| retryable_no_side_effect())?;
    if !response.status().is_success() {
        return Err(classify_no_side_effect_status(
            response.status(),
            response.headers(),
        ));
    }
    let list = bounded_json::<OutlookMessageList>(response)
        .await
        .map_err(|_| retryable_no_side_effect())?;
    let mut exact = list
        .value
        .into_iter()
        .filter(|message| {
            message.internet_message_id == internet_message_id
                && message.conversation_id == source.conversation_id
        })
        .collect::<Vec<_>>();
    if exact.len() != 1 {
        return Err(invalid_action());
    }
    let message = exact.pop().expect("one exact Outlook source message");
    let immutable_id = provider_identifier(&message.id)
        .ok_or_else(retryable_no_side_effect)?
        .to_string();
    let reply_target = provider_reply_target(&message).ok_or_else(invalid_action)?;
    if reply_target != approved_to {
        return Err(invalid_action());
    }
    Ok(immutable_id)
}

fn provider_reply_target(message: &OutlookMessage) -> Option<String> {
    match message.reply_to.as_slice() {
        [] => message
            .from
            .as_ref()
            .and_then(|recipient| canonical_provider_email(&recipient.email_address.address)),
        [recipient] => canonical_provider_email(&recipient.email_address.address),
        _ => None,
    }
}

fn canonical_provider_email(value: &str) -> Option<String> {
    if value.chars().any(char::is_control) {
        return None;
    }
    let normalized = value.trim().to_ascii_lowercase();
    normalized_email(&normalized)
}

fn retryable_no_side_effect() -> ProviderDispatchResult {
    ProviderDispatchResult::DefinitiveNoSideEffect {
        kind: ProviderFailureKind::RetryableNoSideEffect,
        retry_after_ms: None,
    }
}

pub(super) async fn lookup(
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    access_token: &str,
    request: &CommunicationProviderRequest,
) -> ProviderLookupResult {
    let Some(source) = request.source_message.as_ref() else {
        return ProviderLookupResult::Inconclusive {
            reason_code: "outlook_source_conversation_missing",
        };
    };
    if provider_identifier(&source.conversation_id).is_none() {
        return ProviderLookupResult::Inconclusive {
            reason_code: "outlook_source_conversation_missing",
        };
    }
    let operation_message_id = operation_message_id(&request.provider_operation_key);
    let escaped_message_id = operation_message_id.replace('\'', "''");
    let filter = format!("internetMessageId eq '{escaped_message_id}'");
    let mut url = match reqwest::Url::parse(&format!(
        "{}/me/mailFolders/sentitems/messages",
        endpoints.microsoft_graph
    )) {
        Ok(url) => url,
        Err(_) => {
            return ProviderLookupResult::Inconclusive {
                reason_code: "outlook_lookup_configuration_invalid",
            }
        }
    };
    url.query_pairs_mut()
        .append_pair("$filter", &filter)
        .append_pair("$select", "id,internetMessageId,conversationId")
        .append_pair("$top", "2");
    let response = match client
        .get(url)
        .header("Prefer", "IdType=\"ImmutableId\"")
        .bearer_auth(access_token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return ProviderLookupResult::Inconclusive {
                reason_code: "outlook_lookup_transport_failed",
            }
        }
    };
    if !response.status().is_success() {
        return lookup_http_failure(response.status());
    }
    let list = match bounded_json::<OutlookMessageList>(response).await {
        Ok(list) => list,
        Err(_) => {
            return ProviderLookupResult::Inconclusive {
                reason_code: "outlook_lookup_response_invalid",
            }
        }
    };
    let mut exact = list
        .value
        .into_iter()
        .filter(|message| message.internet_message_id == operation_message_id)
        .collect::<Vec<_>>();
    match exact.len() {
        0 => ProviderLookupResult::Absent,
        1 => {
            let message = exact.pop().expect("one exact Outlook message");
            if provider_identifier(&message.id).is_none()
                || provider_identifier(&message.conversation_id).is_none()
            {
                return ProviderLookupResult::Inconclusive {
                    reason_code: "outlook_lookup_missing_provider_id",
                };
            }
            if source.conversation_id != message.conversation_id {
                return ProviderLookupResult::Conflict;
            }
            ProviderLookupResult::Found(commit_evidence(
                &message.id,
                &message.conversation_id,
                &operation_message_id,
            ))
        }
        _ => ProviderLookupResult::Conflict,
    }
}

pub(super) fn operation_message_id(operation_key: &str) -> String {
    let digest = hex::encode(Sha256::digest(operation_key.as_bytes()));
    format!("<{digest}@actions.bluey.sh>")
}

fn commit_evidence(
    provider_object_id: &str,
    conversation_id: &str,
    operation_message_id: &str,
) -> ProviderCommitEvidence {
    ProviderCommitEvidence {
        provider_object_id: provider_object_id.to_string(),
        evidence: json!({
            "schema_version": 1,
            "provider": "outlook_email",
            "provider_object_id": provider_object_id,
            "conversation_sha256": hex::encode(Sha256::digest(conversation_id.as_bytes())),
            "operation_message_id": operation_message_id,
            "id_type": "immutable",
        }),
    }
}
