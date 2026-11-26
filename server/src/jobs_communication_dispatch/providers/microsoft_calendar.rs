use chrono::{TimeZone, Utc};
use chrono_tz::Tz;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{
    bounded_json, classify_write_status, invalid_action, lookup_http_failure, normalized_email,
    payload_string, provider_identifier, provider_url_with_segments, retry_after_ms,
    CommunicationProviderRequest, ProviderDispatchResult, ProviderEndpoints, ProviderLookupResult,
};
use crate::jobs_communication_dispatch::contracts::{ProviderCommitEvidence, ProviderFailureKind};

pub(super) const ACTION_PROPERTY_ID: &str =
    "String {8d8c9f64-7f3b-4cf7-a65b-2f8f8b9fd542} Name BlueyActionKey";

#[derive(Deserialize)]
struct MicrosoftEvent {
    #[serde(default)]
    id: String,
    #[serde(rename = "transactionId", default)]
    transaction_id: String,
    #[serde(rename = "singleValueExtendedProperties", default)]
    properties: Vec<MicrosoftProperty>,
}

#[derive(Deserialize)]
struct MicrosoftProperty {
    id: String,
    value: String,
}

#[derive(Deserialize)]
struct MicrosoftEventList {
    #[serde(default)]
    value: Vec<MicrosoftEvent>,
}

#[derive(Deserialize)]
struct MicrosoftTimeZoneList {
    #[serde(default)]
    value: Vec<MicrosoftTimeZone>,
}

#[derive(Deserialize)]
struct MicrosoftTimeZone {
    alias: String,
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
    if let Err(result) =
        require_supported_time_zone(client, endpoints, access_token, time_zone).await
    {
        return result;
    }
    let transaction_id = operation_transaction_id(&request.provider_operation_key);
    let response = match client
        .post(format!("{}/me/events", endpoints.microsoft_graph))
        .header("Prefer", "IdType=\"ImmutableId\"")
        .bearer_auth(access_token)
        .json(&json!({
            "subject": title,
            "start": {
                "dateTime": start
                    .with_timezone(&time_zone_value)
                    .format("%Y-%m-%dT%H:%M:%S%.3f")
                    .to_string(),
                "timeZone": time_zone,
            },
            "end": {
                "dateTime": end
                    .with_timezone(&time_zone_value)
                    .format("%Y-%m-%dT%H:%M:%S%.3f")
                    .to_string(),
                "timeZone": time_zone,
            },
            "attendees": attendees,
            "transactionId": transaction_id,
            "singleValueExtendedProperties": [{
                "id": ACTION_PROPERTY_ID,
                "value": request.provider_operation_key,
            }]
        }))
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return ProviderDispatchResult::Ambiguous {
                reason_code: "microsoft_calendar_insert_transport_unknown",
            }
        }
    };
    if !response.status().is_success() {
        return classify_write_status(response.status(), response.headers());
    }
    let event = match bounded_json::<MicrosoftEvent>(response).await {
        Ok(event) => event,
        Err(_) => {
            return ProviderDispatchResult::Ambiguous {
                reason_code: "microsoft_calendar_insert_response_unknown",
            }
        }
    };
    if provider_identifier(&event.id).is_none() {
        return ProviderDispatchResult::Ambiguous {
            reason_code: "microsoft_calendar_insert_proof_mismatch",
        };
    }
    match verify_event(
        client,
        endpoints,
        access_token,
        &event.id,
        &transaction_id,
        &request.provider_operation_key,
        request,
    )
    .await
    {
        ProviderLookupResult::Found(evidence) => ProviderDispatchResult::Committed(evidence),
        _ => ProviderDispatchResult::Ambiguous {
            reason_code: "microsoft_calendar_insert_proof_unavailable",
        },
    }
}

pub(super) async fn lookup(
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    access_token: &str,
    request: &CommunicationProviderRequest,
) -> ProviderLookupResult {
    let transaction_id = operation_transaction_id(&request.provider_operation_key);
    let filter = format!(
        "singleValueExtendedProperties/Any(ep: ep/id eq '{}' and ep/value eq '{}')",
        ACTION_PROPERTY_ID, request.provider_operation_key
    );
    let mut url = match reqwest::Url::parse(&format!("{}/me/events", endpoints.microsoft_graph)) {
        Ok(url) => url,
        Err(_) => {
            return ProviderLookupResult::Inconclusive {
                reason_code: "microsoft_calendar_lookup_configuration_invalid",
            }
        }
    };
    url.query_pairs_mut()
        .append_pair("$filter", &filter)
        .append_pair("$expand", "singleValueExtendedProperties")
        .append_pair("$select", "id,transactionId")
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
                reason_code: "microsoft_calendar_lookup_transport_failed",
            }
        }
    };
    if !response.status().is_success() {
        return lookup_http_failure(response.status());
    }
    let list = match bounded_json::<MicrosoftEventList>(response).await {
        Ok(list) => list,
        Err(_) => {
            return ProviderLookupResult::Inconclusive {
                reason_code: "microsoft_calendar_lookup_response_invalid",
            }
        }
    };
    let mut exact = list
        .value
        .into_iter()
        .filter(|event| {
            event.transaction_id == transaction_id
                && event.properties.iter().any(|property| {
                    property.id == ACTION_PROPERTY_ID
                        && property.value == request.provider_operation_key
                })
        })
        .collect::<Vec<_>>();
    match exact.len() {
        0 => ProviderLookupResult::Absent,
        1 => {
            let event = exact.pop().expect("one exact Microsoft event");
            if provider_identifier(&event.id).is_none() {
                ProviderLookupResult::Inconclusive {
                    reason_code: "microsoft_calendar_lookup_missing_provider_id",
                }
            } else {
                ProviderLookupResult::Found(commit_evidence(&event.id, &transaction_id, request))
            }
        }
        _ => ProviderLookupResult::Conflict,
    }
}

fn calendar_attendees(payload: &Value) -> Option<Vec<Value>> {
    let mut attendees = Vec::new();
    for attendee in payload
        .get("attendees")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let address = normalized_email(attendee.as_str()?)?;
        attendees.push(json!({
            "emailAddress": { "address": address },
            "type": "required",
        }));
    }
    if attendees.len() > 25 {
        return None;
    }
    Some(attendees)
}

pub(super) fn operation_transaction_id(operation_key: &str) -> String {
    let digest = Sha256::digest(operation_key.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

async fn verify_event(
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    access_token: &str,
    event_id: &str,
    transaction_id: &str,
    operation_key: &str,
    request: &CommunicationProviderRequest,
) -> ProviderLookupResult {
    let mut url =
        match provider_url_with_segments(&endpoints.microsoft_graph, &["me", "events", event_id]) {
            Some(url) => url,
            None => {
                return ProviderLookupResult::Inconclusive {
                    reason_code: "microsoft_calendar_verify_configuration_invalid",
                }
            }
        };
    url.query_pairs_mut()
        .append_pair("$expand", "singleValueExtendedProperties")
        .append_pair("$select", "id,transactionId");
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
                reason_code: "microsoft_calendar_verify_transport_failed",
            }
        }
    };
    if !response.status().is_success() {
        return lookup_http_failure(response.status());
    }
    let event = match bounded_json::<MicrosoftEvent>(response).await {
        Ok(event) => event,
        Err(_) => {
            return ProviderLookupResult::Inconclusive {
                reason_code: "microsoft_calendar_verify_response_invalid",
            }
        }
    };
    if provider_identifier(&event.id).is_none()
        || provider_identifier(&event.transaction_id).is_none()
        || event.id != event_id
        || event.transaction_id != transaction_id
        || !event
            .properties
            .iter()
            .any(|property| property.id == ACTION_PROPERTY_ID && property.value == operation_key)
    {
        return ProviderLookupResult::Conflict;
    }
    ProviderLookupResult::Found(commit_evidence(event_id, transaction_id, request))
}

async fn require_supported_time_zone(
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    access_token: &str,
    time_zone: &str,
) -> Result<(), ProviderDispatchResult> {
    let response = client
        .get(
            format!(
                "{}/me/outlook/supportedTimeZones(\
             TimeZoneStandard=microsoft.graph.timeZoneStandard'Iana')",
                endpoints.microsoft_graph
            )
            .replace(' ', ""),
        )
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|_| ProviderDispatchResult::DefinitiveNoSideEffect {
            kind: ProviderFailureKind::RetryableNoSideEffect,
            retry_after_ms: None,
        })?;
    if !response.status().is_success() {
        let kind = if matches!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
        ) {
            ProviderFailureKind::Authorization
        } else if response.status().is_client_error() {
            ProviderFailureKind::ProviderRejected
        } else {
            ProviderFailureKind::RetryableNoSideEffect
        };
        return Err(ProviderDispatchResult::DefinitiveNoSideEffect {
            kind,
            retry_after_ms: retry_after_ms(response.headers()),
        });
    }
    let supported = bounded_json::<MicrosoftTimeZoneList>(response)
        .await
        .map_err(|_| ProviderDispatchResult::DefinitiveNoSideEffect {
            kind: ProviderFailureKind::RetryableNoSideEffect,
            retry_after_ms: None,
        })?;
    if !supported.value.iter().any(|zone| zone.alias == time_zone) {
        return Err(ProviderDispatchResult::DefinitiveNoSideEffect {
            kind: ProviderFailureKind::InvalidAction,
            retry_after_ms: None,
        });
    }
    Ok(())
}

fn commit_evidence(
    event_id: &str,
    transaction_id: &str,
    request: &CommunicationProviderRequest,
) -> ProviderCommitEvidence {
    let time_zone = request
        .payload
        .get("time_zone")
        .and_then(Value::as_str)
        .unwrap_or_default();
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
    ProviderCommitEvidence {
        provider_object_id: event_id.to_string(),
        evidence: json!({
            "schema_version": 1,
            "provider": "outlook_calendar",
            "provider_object_id": event_id,
            "transaction_id": transaction_id,
            "extended_property_sha256": hex::encode(Sha256::digest(
                request.provider_operation_key.as_bytes()
            )),
            "id_type": "immutable",
            "attendee_updates": attendee_updates,
            "time_zone": time_zone,
            "time_normalization": "reviewed_epoch_to_local_zone",
        }),
    }
}
