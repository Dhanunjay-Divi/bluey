//! Strict fetch, validation, and caching for the external company directory.

use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use reqwest::{redirect::Policy, Client, Url};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::OnceLock};
use tokio::sync::Mutex;

use crate::db::jobs;

const MANIFEST_URL: &str = "https://storage.stapply.ai/jobhive/v1/manifest.json";
const CATALOG_HOST: &str = "storage.stapply.ai";
const CATALOG_PATH_PREFIX: &str = "/jobhive/v1/";
const MANIFEST_MAX_BYTES: usize = 256 * 1024;
const COMPANY_CSV_MAX_BYTES: usize = 2 * 1024 * 1024;
const CATALOG_TTL_MS: i64 = 6 * 60 * 60 * 1_000;
const MAX_REJECTED_ROWS: usize = 100;
const MAX_REJECTED_ROW_PERCENT: usize = 2;

pub(super) const DISCOVERY_INTERVAL_MS: i64 = 4 * 60 * 60 * 1_000;
pub(super) const ALLOWED_PROVIDERS: [&str; 5] =
    ["greenhouse", "lever", "ashby", "smartrecruiters", "workday"];

#[derive(Debug, Clone)]
pub(super) struct CatalogRecord {
    pub(super) id: String,
    pub(super) provider: String,
    pub(super) company: String,
    pub(super) source_key: String,
}

#[derive(Debug, Clone)]
pub(super) struct CachedDirectory {
    pub(super) fetched_at_ms: i64,
    pub(super) catalog_generated_at: String,
    pub(super) records: Vec<CatalogRecord>,
}

static DIRECTORY_CACHE: OnceLock<Mutex<Option<CachedDirectory>>> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct CatalogManifest {
    by_ats_companies: BTreeMap<String, CatalogDescriptor>,
    generated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogDescriptor {
    csv: String,
    rows: usize,
    sha256: String,
    size_bytes: usize,
}

#[derive(Debug, Deserialize)]
struct CompanyCsvRow {
    name: String,
    slug: String,
    url: String,
}

pub(super) async fn load_directory() -> Result<CachedDirectory> {
    let cache = DIRECTORY_CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().await;
    let now = jobs::now_ms();
    if let Some(current) = guard.as_ref() {
        if now.saturating_sub(current.fetched_at_ms) < CATALOG_TTL_MS {
            return Ok(current.clone());
        }
    }
    match fetch_directory(now).await {
        Ok(refreshed) => {
            *guard = Some(refreshed.clone());
            Ok(refreshed)
        }
        Err(error) => {
            if let Some(stale) = guard.as_ref() {
                tracing::warn!(
                    error_category = "jobs_source_directory_refresh",
                    "Using stale Jobs source directory after refresh failed"
                );
                Ok(stale.clone())
            } else {
                Err(error)
            }
        }
    }
}

async fn fetch_directory(fetched_at_ms: i64) -> Result<CachedDirectory> {
    let client = Client::builder()
        .redirect(Policy::none())
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .context("build Jobs source-directory client")?;
    let manifest_bytes = fetch_bounded(&client, MANIFEST_URL, MANIFEST_MAX_BYTES).await?;
    let manifest: CatalogManifest =
        serde_json::from_slice(&manifest_bytes).context("parse Jobs source-directory manifest")?;
    if manifest.generated_at.trim().is_empty() || manifest.generated_at.len() > 80 {
        bail!("Jobs source-directory manifest timestamp is invalid")
    }

    let mut records = Vec::new();
    for provider in ALLOWED_PROVIDERS {
        let descriptor = manifest
            .by_ats_companies
            .get(provider)
            .with_context(|| format!("Jobs source directory has no {provider} catalog"))?;
        validate_descriptor(provider, descriptor)?;
        let bytes = fetch_bounded(&client, &descriptor.csv, COMPANY_CSV_MAX_BYTES).await?;
        verify_catalog_file(descriptor, &bytes)?;
        records.extend(parse_company_csv(provider, descriptor, &bytes)?);
    }
    records.sort_by(|left, right| {
        left.company
            .to_lowercase()
            .cmp(&right.company.to_lowercase())
            .then_with(|| left.provider.cmp(&right.provider))
    });
    records.dedup_by(|left, right| {
        left.provider == right.provider && left.source_key == right.source_key
    });
    if records.is_empty() {
        bail!("Jobs source directory is empty")
    }

    Ok(CachedDirectory {
        fetched_at_ms,
        catalog_generated_at: manifest.generated_at,
        records,
    })
}

async fn fetch_bounded(client: &Client, raw_url: &str, max_bytes: usize) -> Result<Vec<u8>> {
    validate_catalog_url(raw_url)?;
    let response = client
        .get(raw_url)
        .send()
        .await
        .context("fetch Jobs source-directory file")?;
    if !response.status().is_success() {
        bail!("Jobs source-directory file was unavailable")
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        bail!("Jobs source-directory file exceeds the size limit")
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("read Jobs source-directory file")?;
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            bail!("Jobs source-directory file exceeds the size limit")
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn validate_descriptor(provider: &str, descriptor: &CatalogDescriptor) -> Result<()> {
    validate_catalog_url(&descriptor.csv)?;
    let expected_path = format!("{CATALOG_PATH_PREFIX}{provider}/companies.csv");
    let url = Url::parse(&descriptor.csv)?;
    if url.path() != expected_path {
        bail!("Jobs source-directory descriptor path is invalid")
    }
    if descriptor.rows == 0
        || descriptor.rows > 20_000
        || descriptor.size_bytes == 0
        || descriptor.size_bytes > COMPANY_CSV_MAX_BYTES
        || descriptor.sha256.len() != 64
        || !descriptor
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("Jobs source-directory descriptor is invalid")
    }
    Ok(())
}

fn validate_catalog_url(raw_url: &str) -> Result<()> {
    let url = Url::parse(raw_url).context("parse Jobs source-directory URL")?;
    if url.scheme() != "https"
        || url.host_str() != Some(CATALOG_HOST)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.path().starts_with(CATALOG_PATH_PREFIX)
    {
        bail!("Jobs source-directory URL is not allowlisted")
    }
    Ok(())
}

fn verify_catalog_file(descriptor: &CatalogDescriptor, bytes: &[u8]) -> Result<()> {
    if bytes.len() != descriptor.size_bytes {
        bail!("Jobs source-directory file size did not match its manifest")
    }
    let actual = hex::encode(Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(&descriptor.sha256) {
        bail!("Jobs source-directory file checksum did not match its manifest")
    }
    Ok(())
}

fn parse_company_csv(
    provider: &str,
    descriptor: &CatalogDescriptor,
    bytes: &[u8],
) -> Result<Vec<CatalogRecord>> {
    let mut reader = csv::ReaderBuilder::new().flexible(false).from_reader(bytes);
    let headers = reader
        .headers()
        .context("read Jobs source-directory CSV headers")?;
    if headers.iter().collect::<Vec<_>>() != ["name", "slug", "url"] {
        bail!("Jobs source-directory CSV headers are invalid")
    }
    let mut records = Vec::with_capacity(descriptor.rows);
    let mut raw_rows = 0usize;
    let mut rejected_rows = 0usize;
    for row in reader.deserialize::<CompanyCsvRow>() {
        let row = row.context("parse Jobs source-directory CSV row")?;
        raw_rows = raw_rows.saturating_add(1);
        if let Some(record) = catalog_record_from_row(provider, row) {
            records.push(record);
        } else {
            rejected_rows = rejected_rows.saturating_add(1);
        }
    }
    if raw_rows != descriptor.rows {
        bail!("Jobs source-directory CSV row count did not match its manifest")
    }
    let rejection_limit = descriptor
        .rows
        .saturating_mul(MAX_REJECTED_ROW_PERCENT)
        .div_ceil(100)
        .min(MAX_REJECTED_ROWS);
    if rejected_rows > rejection_limit {
        bail!("Jobs source-directory CSV contained too many invalid rows")
    }
    if rejected_rows > 0 {
        tracing::warn!(
            provider,
            raw_rows,
            accepted_rows = records.len(),
            rejected_rows,
            "Filtered invalid rows from the Jobs source directory"
        );
    }
    if records.is_empty() {
        bail!("Jobs source-directory CSV has no trusted rows")
    }
    Ok(records)
}

fn catalog_record_from_row(provider: &str, row: CompanyCsvRow) -> Option<CatalogRecord> {
    let company = row.name.trim();
    if company.is_empty() || company.chars().count() > 200 || company.chars().any(char::is_control)
    {
        return None;
    }
    let source_key = board_source_key(provider, &row.url)?;
    if provider != "workday" && source_key != row.slug.trim() {
        return None;
    }
    let id = opaque_catalog_id(provider, &source_key);
    Some(CatalogRecord {
        id,
        provider: provider.to_string(),
        company: company.to_string(),
        source_key,
    })
}

fn board_source_key(provider: &str, raw_url: &str) -> Option<String> {
    let url = Url::parse(raw_url).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let segments = url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let source_key = match provider {
        "greenhouse"
            if matches!(
                host.as_str(),
                "boards.greenhouse.io" | "job-boards.greenhouse.io"
            ) && segments.len() == 1 =>
        {
            segments[0].to_string()
        }
        "lever"
            if matches!(host.as_str(), "jobs.lever.co" | "jobs.eu.lever.co")
                && segments.len() == 1 =>
        {
            segments[0].to_string()
        }
        "ashby" if host == "jobs.ashbyhq.com" && segments.len() == 1 => segments[0].to_string(),
        "smartrecruiters" if host == "careers.smartrecruiters.com" && segments.len() == 1 => {
            segments[0].to_string()
        }
        "workday" if segments.len() == 1 => {
            let parts = host.split('.').collect::<Vec<_>>();
            if parts.len() != 4
                || parts[2] != "myworkdayjobs"
                || parts[3] != "com"
                || !parts[1].starts_with("wd")
            {
                return None;
            }
            format!("{}~{}~{}", parts[0], parts[1], segments[0])
        }
        _ => return None,
    };
    valid_source_key(provider, &source_key).then_some(source_key)
}

fn valid_source_key(provider: &str, value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'~'))
        && (provider == "workday" || !value.contains('~'))
}

fn opaque_catalog_id(provider: &str, source_key: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(provider.as_bytes());
    digest.update([0]);
    digest.update(source_key.as_bytes());
    hex::encode(digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor_for(bytes: &[u8], rows: usize) -> CatalogDescriptor {
        CatalogDescriptor {
            csv: "https://storage.stapply.ai/jobhive/v1/greenhouse/companies.csv".to_string(),
            rows,
            sha256: hex::encode(Sha256::digest(bytes)),
            size_bytes: bytes.len(),
        }
    }

    #[test]
    fn parses_only_manifest_bound_company_rows() {
        let bytes = b"name,slug,url\nAcme,acme,https://job-boards.greenhouse.io/acme\n";
        let descriptor = descriptor_for(bytes, 1);
        verify_catalog_file(&descriptor, bytes).unwrap();
        let rows = parse_company_csv("greenhouse", &descriptor, bytes).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].company, "Acme");
        assert_eq!(rows[0].source_key, "acme");
        assert_eq!(rows[0].id.len(), 64);
    }

    #[test]
    fn verifies_raw_manifest_rows_before_filtering_untrusted_records() {
        let bytes = b"name,slug,url\nAcme,acme,https://job-boards.greenhouse.io/acme\nUntrusted,untrusted,https://evil.example/untrusted\n";
        let descriptor = descriptor_for(bytes, 2);
        let rows = parse_company_csv("greenhouse", &descriptor, bytes).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].company, "Acme");
    }

    #[test]
    fn rejects_raw_manifest_count_mismatches() {
        let bytes = b"name,slug,url\nAcme,acme,https://job-boards.greenhouse.io/acme\n";
        let descriptor = descriptor_for(bytes, 2);
        assert!(parse_company_csv("greenhouse", &descriptor, bytes).is_err());
    }

    #[test]
    fn rejects_catalogs_with_too_many_untrusted_rows() {
        let mut csv = String::from("name,slug,url\n");
        csv.push_str("Acme,acme,https://job-boards.greenhouse.io/acme\n");
        for index in 0..2 {
            csv.push_str(&format!(
                "Untrusted {index},untrusted-{index},https://evil.example/untrusted-{index}\n"
            ));
        }
        let bytes = csv.as_bytes();
        let descriptor = descriptor_for(bytes, 3);
        assert!(parse_company_csv("greenhouse", &descriptor, bytes).is_err());
    }

    #[test]
    fn rejects_manifest_mismatch_and_untrusted_urls() {
        let bytes = b"name,slug,url\nAcme,acme,https://job-boards.greenhouse.io/acme\n";
        let mut descriptor = descriptor_for(bytes, 1);
        descriptor.sha256 = "0".repeat(64);
        assert!(verify_catalog_file(&descriptor, bytes).is_err());
        assert!(validate_catalog_url("https://example.com/jobhive/v1/manifest.json").is_err());
        assert!(
            validate_catalog_url("https://storage.stapply.ai/jobhive/v1/manifest.json?x=1")
                .is_err()
        );
    }

    #[test]
    fn derives_all_five_provider_board_keys() {
        assert_eq!(
            board_source_key("greenhouse", "https://job-boards.greenhouse.io/acme"),
            Some("acme".to_string())
        );
        assert_eq!(
            board_source_key("lever", "https://jobs.lever.co/acme"),
            Some("acme".to_string())
        );
        assert_eq!(
            board_source_key("ashby", "https://jobs.ashbyhq.com/acme"),
            Some("acme".to_string())
        );
        assert_eq!(
            board_source_key(
                "smartrecruiters",
                "https://careers.smartrecruiters.com/acme"
            ),
            Some("acme".to_string())
        );
        assert_eq!(
            board_source_key("workday", "https://acme.wd103.myworkdayjobs.com/Careers"),
            Some("acme~wd103~Careers".to_string())
        );
    }

    #[test]
    fn rejects_ambiguous_or_off_domain_board_urls() {
        assert_eq!(
            board_source_key("greenhouse", "https://evil.example/acme"),
            None
        );
        assert_eq!(
            board_source_key("lever", "https://jobs.lever.co/acme/jobs"),
            None
        );
        assert_eq!(
            board_source_key("workday", "https://acme.example/Careers"),
            None
        );
        assert_eq!(
            board_source_key("ashby", "https://jobs.ashbyhq.com/acme?next=evil"),
            None
        );
    }
}
