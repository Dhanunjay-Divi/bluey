use chrono::{TimeZone, Utc};
use chrono_tz::Tz;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{
    bounded_json, classify_write_status, invalid_action, lookup_http_failure, normalized_email,
    payload_string, CommunicationProviderRequest, ProviderDispatchResult, ProviderEndpoints,
    ProviderLookupResult,
};
use crate::jobs_communication_dispatch::contracts::ProviderCommitEvidence;

#[derive(Deserialize)]
struct GoogleEventResponse {
    #[serde(default)]
    id: String,
    #[serde(rename = "extendedProperties", default)]
    extended_properties: GoogleExtendedProperties,
}

#[derive(Default, Deserialize)]
struct GoogleExtendedProperties {
    #[serde(default)]
    private: std::collections::HashMap<String, String>,
}

pub(super) async fn dispatch(
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    access_token: &str,
    request: &CommunicationProviderRequest,
) -> ProviderDispatchResult {
    let Some(title) = payload_string(&request.payload, "title", 512)
        .filter(|value| crate::db::jobs::communication_review_text_is_safe(value))
    else {
        return invalid_action();
    };
    let Some(starts_at_ms) = request.payload.get("starts_at_ms").and_then(Value::as_i64) else {
        return invalid_action();
    };
    let Some(ends_at_ms) = request.payload.get("ends_at_ms").and_then(Value::as_i64) else {
        return invalid_action();
    };
    if starts_at_ms <= 0
        || ends_at_ms <= starts_at_ms
        || ends_at_ms.saturating_sub(starts_at_ms) > 24 * 60 * 60 * 1_000
    {
        return invalid_action();
    }
    let Some(start) = Utc.timestamp_millis_opt(starts_at_ms).single() else {
        return invalid_action();
    };
    let Some(end) = Utc.timestamp_millis_opt(ends_at_ms).single() else {
        return invalid_action();
    };
    let Some(time_zone) = payload_string(&request.payload, "time_zone", 64) else {
        return invalid_action();
    };
    let Ok(time_zone_value) = time_zone.parse::<Tz>() else {
        return invalid_action();
    };
    let Some(attendees) = calendar_attendees(&request.payload) else {
        return invalid_action();
    };
    let event_id = operation_event_id(&request.provider_operation_key);
    let mut url = match reqwest::Url::parse(&format!(
        "{}/calendars/primary/events",
        endpoints.google_calendar
    )) {
        Ok(url) => url,
        Err(_) => return invalid_action(),
    };
    let attendee_updates = if attendees.is_empty() { "none" } else { "all" };
    url.query_pairs_mut()
        .append_pair("sendUpdates", attendee_updates);
    let response = match client
        .post(url)
        .bearer_auth(access_token)
        .json(&json!({
            "id": event_id,
            "summary": title,
            "start": {
                "dateTime": start.with_timezone(&time_zone_value).to_rfc3339(),
                "timeZone": time_zone,
            },
            "end": {
                "dateTime": end.with_timezone(&time_zone_value).to_rfc3339(),
                "timeZone": time_zone,
            },
            "attendees": attendees,
            "extendedProperties": {
                "private": { "blueyActionKey": request.provider_operation_key }
            }
        }))
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return ProviderDispatchResult::Ambiguous {
                reason_code: "google_calendar_insert_transport_unknown",
            }
        }
    };
    if response.status() == reqwest::StatusCode::CONFLICT {
        return match lookup(client, endpoints, access_token, request).await {
            ProviderLookupResult::Found(evidence) => ProviderDispatchResult::Committed(evidence),
            _ => ProviderDispatchResult::Ambiguous {
                reason_code: "google_calendar_conflict_unresolved",
            },
        };
    }
    if !response.status().is_success() {
        return classify_write_status(response.status(), response.headers());
    }
    let event = match bounded_json::<GoogleEventResponse>(response).await {
        Ok(event) => event,
        Err(_) => {
            return ProviderDispatchResult::Ambiguous {
                reason_code: "google_calendar_insert_response_unknown",
            }
        }
    };
    if event.id != event_id
        || event
            .extended_properties
            .private
            .get("blueyActionKey")
            .map(String::as_str)
            != Some(request.provider_operation_key.as_str())
    {
        return ProviderDispatchResult::Ambiguous {
            reason_code: "google_calendar_insert_proof_mismatch",
        };
    }
    ProviderDispatchResult::Committed(commit_evidence(
        &event_id,
        attendee_updates,
        time_zone,
        &request.provider_operation_key,
    ))
}

pub(super) async fn lookup(
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    access_token: &str,
    request: &CommunicationProviderRequest,
) -> ProviderLookupResult {
    let event_id = operation_event_id(&request.provider_operation_key);
    let response = match client
        .get(format!(
            "{}/calendars/primary/events/{event_id}",
            endpoints.google_calendar
        ))
        .bearer_auth(access_token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return ProviderLookupResult::Inconclusive {
                reason_code: "google_calendar_lookup_transport_failed",
            }
        }
    };
    if matches!(
        response.status(),
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::GONE
    ) {
        return ProviderLookupResult::Absent;
    }
    if !response.status().is_success() {
        return lookup_http_failure(response.status());
    }
    let event = match bounded_json::<GoogleEventResponse>(response).await {
        Ok(event) => event,
        Err(_) => {
            return ProviderLookupResult::Inconclusive {
                reason_code: "google_calendar_lookup_response_invalid",
            }
        }
    };
    if event.id != event_id
        || event
            .extended_properties
            .private
            .get("blueyActionKey")
            .map(String::as_str)
            != Some(request.provider_operation_key.as_str())
    {
        return ProviderLookupResult::Conflict;
    }
    let attendee_updates = if request
        .payload
        .get("attendees")
        .and_then(Value::as_array)
        .is_some_and(|attendees| !attendees.is_empty())
    {
        "all"
    } else {
        "none"
    };
    let time_zone = request
        .payload
        .get("time_zone")
        .and_then(Value::as_str)
        .unwrap_or_default();
    ProviderLookupResult::Found(commit_evidence(
        &event_id,
        attendee_updates,
        time_zone,
        &request.provider_operation_key,
    ))
}

fn calendar_attendees(payload: &Value) -> Option<Vec<Value>> {
    let mut attendees = Vec::new();
    for attendee in payload
        .get("attendees")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let email = normalized_email(attendee.as_str()?)?;
        attendees.push(json!({ "email": email }));
    }
    if attendees.len() > 25 {
        return None;
    }
    Some(attendees)
}

pub(super) fn operation_event_id(operation_key: &str) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789abcdefghijklmnopqrstuv";
    let digest = Sha256::digest(operation_key.as_bytes());
    let mut output = String::with_capacity(32);
    let mut accumulator = 0u32;
    let mut bits = 0u8;
    for byte in digest {
        accumulator = (accumulator << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 && output.len() < 32 {
            bits -= 5;
            output.push(ALPHABET[((accumulator >> bits) & 31) as usize] as char);
        }
        if output.len() == 32 {
            break;
        }
    }
    output
}

fn commit_evidence(
    event_id: &str,
    attendee_updates: &str,
    time_zone: &str,
    operation_key: &str,
) -> ProviderCommitEvidence {
    ProviderCommitEvidence {
        provider_object_id: event_id.to_string(),
        evidence: json!({
            "schema_version": 1,
            "provider": "google_calendar",
            "provider_object_id": event_id,
            "deterministic_event_id": event_id,
            "private_marker_sha256": hex::encode(Sha256::digest(operation_key.as_bytes())),
            "attendee_updates": attendee_updates,
            "time_zone": time_zone,
            "time_normalization": "reviewed_epoch_to_local_zone",
        }),
    }
}
