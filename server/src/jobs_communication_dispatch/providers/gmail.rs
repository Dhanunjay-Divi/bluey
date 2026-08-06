use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{
    bounded_json, classify_write_status, header_text, invalid_action, lookup_http_failure,
    normalized_email, payload_string, provider_identifier, provider_url_with_segments,
    CommunicationProviderRequest, ProviderDispatchResult, ProviderEndpoints, ProviderLookupResult,
};
use crate::jobs_communication_dispatch::contracts::ProviderCommitEvidence;

#[derive(Deserialize)]
struct GmailSendResponse {
    #[serde(default)]
    id: String,
    #[serde(rename = "threadId", default)]
    thread_id: String,
}

#[derive(Deserialize)]
struct GmailListResponse {
    #[serde(default)]
    messages: Vec<GmailMessageRef>,
}

#[derive(Deserialize)]
struct GmailMessageRef {
    id: String,
    #[serde(rename = "threadId", default)]
    thread_id: String,
}

#[derive(Deserialize)]
struct GmailMessageProof {
    id: String,
    #[serde(rename = "threadId", default)]
    thread_id: String,
    #[serde(default)]
    payload: GmailProofPayload,
}

#[derive(Default, Deserialize)]
struct GmailProofPayload {
    #[serde(default)]
    headers: Vec<GmailProofHeader>,
}

#[derive(Deserialize)]
struct GmailProofHeader {
    name: String,
    value: String,
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
    if source.provider != "gmail" || provider_identifier(&source.thread_id).is_none() {
        return invalid_action();
    }
    let Some(to) = payload_string(&request.payload, "to", 320).and_then(normalized_email) else {
        return invalid_action();
    };
    if normalized_email(&source.reply_target).as_deref() != Some(to.as_str()) {
        return invalid_action();
    }
    let Some(subject) =
        payload_string(&request.payload, "subject", 998).and_then(|value| header_text(value, 998))
    else {
        return invalid_action();
    };
    let Some(body) = payload_string(&request.payload, "body_text", 32_000)
        .filter(|value| crate::db::jobs::communication_body_text_is_safe(value))
    else {
        return invalid_action();
    };
    let operation_message_id = operation_message_id(&request.provider_operation_key);
    let mut headers = vec![
        format!("To: {to}"),
        format!("Subject: {subject}"),
        format!("Message-ID: {operation_message_id}"),
        "MIME-Version: 1.0".to_string(),
        "Content-Type: text/plain; charset=UTF-8".to_string(),
        "Content-Transfer-Encoding: 8bit".to_string(),
    ];
    if let Some(parent) = header_text(&source.rfc_message_id, 998) {
        headers.insert(2, format!("In-Reply-To: {parent}"));
        headers.insert(3, format!("References: {parent}"));
    }
    let body = body
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n");
    let raw = format!("{}\r\n\r\n{}", headers.join("\r\n"), body);
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw.as_bytes());
    let response = match client
        .post(format!(
            "{}/gmail/v1/users/me/messages/send",
            endpoints.gmail
        ))
        .bearer_auth(access_token)
        .json(&json!({ "raw": encoded, "threadId": source.thread_id }))
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return ProviderDispatchResult::Ambiguous {
                reason_code: "gmail_send_transport_unknown",
            }
        }
    };
    let status = response.status();
    if !status.is_success() {
        return classify_write_status(status, response.headers());
    }
    let sent = match bounded_json::<GmailSendResponse>(response).await {
        Ok(sent) => sent,
        Err(_) => {
            return ProviderDispatchResult::Ambiguous {
                reason_code: "gmail_send_response_unknown",
            }
        }
    };
    if provider_identifier(&sent.id).is_none()
        || provider_identifier(&sent.thread_id).is_none()
        || sent.thread_id != source.thread_id
    {
        return ProviderDispatchResult::Ambiguous {
            reason_code: "gmail_send_proof_mismatch",
        };
    }
    let Some(mut proof_url) = provider_url_with_segments(
        &endpoints.gmail,
        &["gmail", "v1", "users", "me", "messages", &sent.id],
    ) else {
        return ProviderDispatchResult::Ambiguous {
            reason_code: "gmail_send_proof_configuration_invalid",
        };
    };
    proof_url
        .query_pairs_mut()
        .append_pair("format", "metadata")
        .append_pair("metadataHeaders", "Message-ID");
    let proof_response = match client.get(proof_url).bearer_auth(access_token).send().await {
        Ok(response) if response.status().is_success() => response,
        _ => {
            return ProviderDispatchResult::Ambiguous {
                reason_code: "gmail_send_readback_unknown",
            }
        }
    };
    let proof = match bounded_json::<GmailMessageProof>(proof_response).await {
        Ok(proof) => proof,
        Err(_) => {
            return ProviderDispatchResult::Ambiguous {
                reason_code: "gmail_send_readback_unknown",
            }
        }
    };
    let message_ids = proof
        .payload
        .headers
        .iter()
        .filter(|header| header.name.eq_ignore_ascii_case("message-id"))
        .map(|header| header.value.as_str())
        .collect::<Vec<_>>();
    if proof.id != sent.id
        || proof.thread_id != sent.thread_id
        || message_ids.as_slice() != [operation_message_id.as_str()]
    {
        return ProviderDispatchResult::Ambiguous {
            reason_code: "gmail_send_proof_mismatch",
        };
    }
    ProviderDispatchResult::Committed(commit_evidence(
        &sent.id,
        &sent.thread_id,
        &operation_message_id,
    ))
}

pub(super) async fn lookup(
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    access_token: &str,
    request: &CommunicationProviderRequest,
) -> ProviderLookupResult {
    let operation_message_id = operation_message_id(&request.provider_operation_key);
    let mut url =
        match reqwest::Url::parse(&format!("{}/gmail/v1/users/me/messages", endpoints.gmail)) {
            Ok(url) => url,
            Err(_) => {
                return ProviderLookupResult::Inconclusive {
                    reason_code: "gmail_lookup_configuration_invalid",
                }
            }
        };
    url.query_pairs_mut()
        .append_pair("q", &format!("in:sent rfc822msgid:{operation_message_id}"))
        .append_pair("maxResults", "10");
    let response = match client.get(url).bearer_auth(access_token).send().await {
        Ok(response) => response,
        Err(_) => {
            return ProviderLookupResult::Inconclusive {
                reason_code: "gmail_lookup_transport_failed",
            }
        }
    };
    if !response.status().is_success() {
        return lookup_http_failure(response.status());
    }
    let list = match bounded_json::<GmailListResponse>(response).await {
        Ok(list) => list,
        Err(_) => {
            return ProviderLookupResult::Inconclusive {
                reason_code: "gmail_lookup_response_invalid",
            }
        }
    };
    match list.messages.as_slice() {
        [] => ProviderLookupResult::Absent,
        [message]
            if provider_identifier(&message.id).is_some()
                && provider_identifier(&message.thread_id).is_some() =>
        {
            if let Some(source) = request.source_message.as_ref() {
                if !source.thread_id.trim().is_empty()
                    && !message.thread_id.trim().is_empty()
                    && source.thread_id != message.thread_id
                {
                    return ProviderLookupResult::Conflict;
                }
            }
            ProviderLookupResult::Found(commit_evidence(
                &message.id,
                &message.thread_id,
                &operation_message_id,
            ))
        }
        [_] => ProviderLookupResult::Inconclusive {
            reason_code: "gmail_lookup_missing_provider_id",
        },
        _ => ProviderLookupResult::Conflict,
    }
}

pub(super) fn operation_message_id(operation_key: &str) -> String {
    let digest = hex::encode(Sha256::digest(operation_key.as_bytes()));
    format!("<{digest}@actions.bluey.sh>")
}

fn commit_evidence(
    provider_object_id: &str,
    thread_id: &str,
    operation_message_id: &str,
) -> ProviderCommitEvidence {
    ProviderCommitEvidence {
        provider_object_id: provider_object_id.to_string(),
        evidence: json!({
            "schema_version": 1,
            "provider": "gmail",
            "provider_object_id": provider_object_id,
            "thread_sha256": hex::encode(Sha256::digest(thread_id.as_bytes())),
            "operation_message_id": operation_message_id,
        }),
    }
}
