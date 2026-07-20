//! Authenticated, bounded enrollment for public ATS career pages.
//!
//! The external catalog is only a company-directory hint. Bluey resolves an
//! opaque catalog ID server-side, enrolls the original public ATS board, and
//! lets the existing provider reader revalidate every posting before use.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::jobs_source_directory_catalog::{
    load_directory, ALLOWED_PROVIDERS, DISCOVERY_INTERVAL_MS,
};

use crate::{
    api::AppState,
    auth::AuthedAccount,
    db::jobs::{self, DiscoverySourceInput, DiscoverySourceSummary},
};

type ApiError = (StatusCode, String);

const SEARCH_LIMIT: usize = 24;

#[derive(Debug, Deserialize)]
pub struct CatalogSearchQuery {
    q: String,
    track_id: String,
    #[serde(default)]
    provider: String,
}

#[derive(Debug, Serialize)]
pub struct SourceDirectoryEntry {
    id: String,
    provider: String,
    company: String,
    connected: bool,
}

#[derive(Debug, Serialize)]
pub struct SourceDirectorySearchResponse {
    refreshed_at_ms: i64,
    catalog_generated_at: String,
    entries: Vec<SourceDirectoryEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ConnectDiscoverySourceRequest {
    track_id: String,
    catalog_entry_id: String,
}

pub async fn search_catalog(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Query(query): Query<CatalogSearchQuery>,
) -> Result<Json<SourceDirectorySearchResponse>, ApiError> {
    let needle = normalized_search_query(&query.q).map_err(bad_request)?;
    let track_id = query.track_id.trim();
    require_account_track(&state, &account.id, track_id)?;
    let provider = normalized_provider_filter(&query.provider).map_err(bad_request)?;
    let directory = load_directory().await.map_err(catalog_unavailable)?;
    let connected = jobs::list_discovery_sources(&state.pool, &account.id)
        .map_err(internal)?
        .into_iter()
        .filter(|source| source.track_id == track_id)
        .map(|source| (source.provider, source.source_key))
        .collect::<BTreeSet<_>>();

    let mut matches = directory
        .records
        .iter()
        .filter(|record| {
            provider
                .as_deref()
                .is_none_or(|value| record.provider == value)
        })
        .filter_map(|record| {
            catalog_search_score(&record.company, &needle).map(|score| (score, record))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| {
                left.company
                    .to_lowercase()
                    .cmp(&right.company.to_lowercase())
            })
            .then_with(|| left.provider.cmp(&right.provider))
    });
    let entries = matches
        .into_iter()
        .take(SEARCH_LIMIT)
        .map(|(_, record)| SourceDirectoryEntry {
            id: record.id.clone(),
            provider: record.provider.clone(),
            company: record.company.clone(),
            connected: connected.contains(&(record.provider.clone(), record.source_key.clone())),
        })
        .collect();

    Ok(Json(SourceDirectorySearchResponse {
        refreshed_at_ms: directory.fetched_at_ms,
        catalog_generated_at: directory.catalog_generated_at,
        entries,
    }))
}

pub async fn connect_source(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(request): Json<ConnectDiscoverySourceRequest>,
) -> Result<Json<DiscoverySourceSummary>, ApiError> {
    let track_id = request.track_id.trim();
    require_account_track(&state, &account.id, track_id)?;
    let catalog_entry_id = request.catalog_entry_id.trim();
    if catalog_entry_id.len() != 64
        || !catalog_entry_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(bad_request(
            "Choose a company from the directory.".to_string(),
        ));
    }
    let directory = load_directory().await.map_err(catalog_unavailable)?;
    let record = directory
        .records
        .iter()
        .find(|record| record.id == catalog_entry_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "That company is no longer in the directory.".to_string(),
            )
        })?;
    let source = jobs::upsert_discovery_source(
        &state.pool,
        &account.id,
        &DiscoverySourceInput {
            track_id: track_id.to_string(),
            provider: record.provider.clone(),
            source_key: record.source_key.clone(),
            company: record.company.clone(),
            run_interval_ms: DISCOVERY_INTERVAL_MS,
        },
    )
    .map_err(discovery_error)?;
    Ok(Json(DiscoverySourceSummary::from(&source)))
}

fn require_account_track(
    state: &AppState,
    account_id: &str,
    track_id: &str,
) -> Result<(), ApiError> {
    if track_id.is_empty() {
        return Err(bad_request("Choose a Career Track first.".to_string()));
    }
    let exists = jobs::list_tracks(&state.pool, account_id)
        .map_err(internal)?
        .iter()
        .any(|track| track.id == track_id);
    if !exists {
        return Err((StatusCode::NOT_FOUND, "Career Track not found.".to_string()));
    }
    Ok(())
}

fn normalized_search_query(raw: &str) -> Result<String, String> {
    let value = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if value.chars().count() < 2 || value.chars().count() > 80 {
        return Err("Enter at least 2 characters to search companies.".to_string());
    }
    Ok(value)
}

fn normalized_provider_filter(raw: &str) -> Result<Option<String>, String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() || value == "all" {
        return Ok(None);
    }
    if ALLOWED_PROVIDERS.contains(&value.as_str()) {
        Ok(Some(value))
    } else {
        Err("Choose a supported application system.".to_string())
    }
}

fn catalog_search_score(company: &str, needle: &str) -> Option<u8> {
    let haystack = company.to_lowercase();
    if haystack == needle {
        return Some(100);
    }
    if haystack.starts_with(needle) {
        return Some(90);
    }
    if haystack
        .split_whitespace()
        .any(|word| word.starts_with(needle))
    {
        return Some(80);
    }
    let terms = needle.split_whitespace().collect::<Vec<_>>();
    if terms.iter().all(|term| haystack.contains(term)) {
        return Some(60);
    }
    None
}

fn bad_request(message: String) -> ApiError {
    (StatusCode::BAD_REQUEST, message)
}

fn catalog_unavailable(error: anyhow::Error) -> ApiError {
    tracing::warn!(
        error_category = "jobs_source_directory_unavailable",
        "Jobs source directory is temporarily unavailable: {error}"
    );
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "Company search is temporarily unavailable. Try again shortly.".to_string(),
    )
}

fn discovery_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    if message.contains("limit") || message.contains("already") {
        (StatusCode::CONFLICT, message)
    } else if message.contains("not found") {
        (StatusCode::NOT_FOUND, message)
    } else if message.contains("discovery") || message.contains("Career Track") {
        (StatusCode::BAD_REQUEST, message)
    } else {
        internal(error)
    }
}

fn internal(error: anyhow::Error) -> ApiError {
    tracing::error!(error = %error, "Bluey Jobs source-directory request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Bluey Jobs could not finish that request. Please try again.".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_directory_entry_does_not_expose_source_keys() {
        let serialized = serde_json::to_string(&SourceDirectoryEntry {
            id: "0".repeat(64),
            provider: "greenhouse".to_string(),
            company: "Acme".to_string(),
            connected: false,
        })
        .unwrap();
        assert!(!serialized.contains("source_key"));
        assert!(!serialized.contains("job-boards"));
    }

    #[test]
    fn ranks_exact_and_prefix_company_matches_first() {
        assert_eq!(
            catalog_search_score("Apex Systems", "apex systems"),
            Some(100)
        );
        assert_eq!(catalog_search_score("Apex Systems", "apex"), Some(90));
        assert_eq!(catalog_search_score("Systems Apex", "apex"), Some(80));
        assert_eq!(
            catalog_search_score("Apex Global Systems", "apex systems"),
            Some(60)
        );
        assert_eq!(catalog_search_score("Completely Different", "apex"), None);
    }
}
