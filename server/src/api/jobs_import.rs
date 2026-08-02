//! Server-owned import of public ATS job facts.
//!
//! Only exact, allowlisted ATS hosts are resolved. User-controlled hosts are
//! never fetched by the API; unsupported links remain manual Review-only jobs.

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use reqwest::{redirect::Policy, StatusCode, Url};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::Duration;

const MAX_JOB_RESPONSE_BYTES: u64 = 1_048_576;
const IMPORT_USER_AGENT: &str = "Bluey-Jobs-Importer/1.0";

#[derive(Debug, PartialEq)]
pub(super) struct ImportedJob {
    pub source: String,
    pub external_id: String,
    pub company: String,
    pub title: String,
    pub location: String,
    pub workplace: String,
    pub canonical_url: String,
    pub description: String,
    pub compensation: String,
    pub employment_type: String,
    pub posted_at_ms: Option<i64>,
    pub verified_at_ms: i64,
    pub evidence_hash: String,
}

#[derive(Debug)]
pub(super) enum JobImportError {
    Invalid(String),
    NotFound(String),
    Temporary(String),
}

pub(super) async fn import_supported_job(
    raw_url: &str,
) -> Result<Option<ImportedJob>, JobImportError> {
    let parsed_url = Url::parse(raw_url)
        .map_err(|_| JobImportError::Invalid("Use a complete https job link.".to_string()))?;
    if parsed_url.scheme() != "https"
        || !parsed_url.username().is_empty()
        || parsed_url.password().is_some()
    {
        return Err(JobImportError::Invalid(
            "Use a public https employer job link.".to_string(),
        ));
    }
    if parsed_url.port().is_some() {
        return Err(JobImportError::Invalid(
            "Use the default HTTPS port for an employer job link.".to_string(),
        ));
    }
    let host = parsed_url
        .host_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let provider = match host.as_str() {
        "jobs.lever.co" | "jobs.eu.lever.co" => "lever",
        "boards.greenhouse.io" | "job-boards.greenhouse.io" => "greenhouse",
        "jobs.ashbyhq.com" => "ashby",
        "jobs.smartrecruiters.com" => "smartrecruiters",
        _ if host.ends_with(".myworkdayjobs.com") => "workday",
        _ => return Ok(None),
    };
    // The scheduler resolves precisely the same public URL grammar. Normalize
    // it before any fetch so a verified import cannot later be rejected during
    // discovery enrollment (for example, repeated `jobs` or `job` segments).
    let canonical_url =
        crate::db::jobs::canonical_public_discovery_url(provider, parsed_url.as_str())
            .map_err(|_| {
                JobImportError::Invalid("Use a direct public employer job link.".to_string())
            })?
            .0;
    let url = Url::parse(&canonical_url)
        .map_err(|_| JobImportError::Invalid("Use a complete https job link.".to_string()))?;
    let mut imported = match provider {
        "lever" => import_lever(&url).await.map(Some),
        "greenhouse" => import_greenhouse(&url).await.map(Some),
        "ashby" => import_ashby(&url).await.map(Some),
        "smartrecruiters" => import_smartrecruiters(&url).await.map(Some),
        "workday" => import_workday(&url).await.map(Some),
        _ => unreachable!(),
    }?;
    if let Some(imported) = imported.as_mut() {
        imported.verified_at_ms = Utc::now().timestamp_millis();
        imported.evidence_hash = imported_job_evidence_hash(imported);
    }
    Ok(imported)
}

fn imported_job_evidence_hash(imported: &ImportedJob) -> String {
    let mut hasher = Sha256::new();
    for field in [
        imported.source.as_str(),
        imported.external_id.as_str(),
        imported.company.as_str(),
        imported.title.as_str(),
        imported.location.as_str(),
        imported.workplace.as_str(),
        imported.canonical_url.as_str(),
        imported.description.as_str(),
        imported.compensation.as_str(),
        imported.employment_type.as_str(),
    ] {
        hasher.update(field.as_bytes());
        hasher.update([0]);
    }
    hasher.update(imported.posted_at_ms.unwrap_or_default().to_le_bytes());
    hex::encode(hasher.finalize())
}

async fn import_lever(url: &Url) -> Result<ImportedJob, JobImportError> {
    let segments = path_segments(url)?;
    if segments.len() < 2 {
        return Err(JobImportError::Invalid(
            "Use the direct Lever job link, including its job ID.".to_string(),
        ));
    }
    let site = checked_identifier(&segments[0])?;
    let posting_id = checked_identifier(&segments[1])?;
    let api_url = format!("https://api.lever.co/v0/postings/{site}/{posting_id}?mode=json");
    let payload = fetch_json(&api_url).await?;
    let title = required_text(&payload, "text", "This Lever job has no role title.")?;
    let page_title = fetch_text(url.as_str())
        .await
        .ok()
        .and_then(|body| html_title(&body));
    let company = page_title
        .as_deref()
        .and_then(|value| company_from_page_title(value, &title))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| humanize_slug(site));
    lever_job_from_payload(url, posting_id, company, &payload)
}

fn lever_job_from_payload(
    url: &Url,
    posting_id: &str,
    company: String,
    payload: &Value,
) -> Result<ImportedJob, JobImportError> {
    let title = required_text(payload, "text", "This Lever job has no role title.")?;
    let categories = payload.get("categories").and_then(Value::as_object);
    let location = categories
        .and_then(|value| value.get("location"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let workplace = normalize_workplace(
        payload
            .get("workplaceType")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        &location,
    );
    let mut sections = Vec::new();
    push_text(&mut sections, payload.get("descriptionPlain"));
    if let Some(lists) = payload.get("lists").and_then(Value::as_array) {
        for list in lists {
            push_text(&mut sections, list.get("text"));
            if let Some(content) = list.get("content").and_then(Value::as_str) {
                let plain = strip_html(content);
                if !plain.is_empty() {
                    sections.push(plain);
                }
            }
        }
    }
    for field in ["additionalPlain", "additional"] {
        let Some(additional) = payload.get(field).and_then(Value::as_str) else {
            continue;
        };
        let plain = strip_html(additional);
        if !plain.is_empty() {
            sections.push(plain);
            break;
        }
    }

    Ok(ImportedJob {
        source: "lever_import".to_string(),
        external_id: posting_id.to_string(),
        company,
        title,
        location,
        workplace,
        canonical_url: url.as_str().to_string(),
        description: sections.join("\n\n"),
        compensation: lever_compensation(payload),
        employment_type: normalize_employment_type(
            categories
                .and_then(|value| value.get("commitment"))
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        posted_at_ms: payload.get("createdAt").and_then(Value::as_i64),
        verified_at_ms: 0,
        evidence_hash: String::new(),
    })
}

async fn import_greenhouse(url: &Url) -> Result<ImportedJob, JobImportError> {
    let segments = path_segments(url)?;
    let jobs_index = segments
        .iter()
        .position(|segment| segment == "jobs")
        .ok_or_else(|| {
            JobImportError::Invalid(
                "Use the direct Greenhouse job link, including its job ID.".to_string(),
            )
        })?;
    if jobs_index == 0 || segments.len() <= jobs_index + 1 {
        return Err(JobImportError::Invalid(
            "Use the direct Greenhouse job link, including its job ID.".to_string(),
        ));
    }
    let board = checked_identifier(&segments[jobs_index - 1])?;
    let posting_id = checked_identifier(&segments[jobs_index + 1])?;
    let api_url = format!(
        "https://boards-api.greenhouse.io/v1/boards/{board}/jobs/{posting_id}?content=true"
    );
    let payload = fetch_json(&api_url).await?;
    let board_url = format!("https://boards-api.greenhouse.io/v1/boards/{board}");
    let company = fetch_json(&board_url)
        .await
        .ok()
        .and_then(|value| {
            value
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| humanize_slug(board));
    let title = required_text(&payload, "title", "This Greenhouse job has no role title.")?;
    let location = payload
        .get("location")
        .and_then(Value::as_object)
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let description = payload
        .get("content")
        .and_then(Value::as_str)
        .map(strip_html)
        .unwrap_or_default();
    let posted_at_ms = payload
        .get("updated_at")
        .and_then(Value::as_str)
        .and_then(parse_timestamp_ms);

    Ok(ImportedJob {
        source: "greenhouse_import".to_string(),
        external_id: posting_id.to_string(),
        company,
        title,
        location: location.clone(),
        workplace: normalize_workplace("", &location),
        canonical_url: url.as_str().to_string(),
        description,
        compensation: String::new(),
        employment_type: String::new(),
        posted_at_ms,
        verified_at_ms: 0,
        evidence_hash: String::new(),
    })
}

async fn import_ashby(url: &Url) -> Result<ImportedJob, JobImportError> {
    let segments = path_segments(url)?;
    if segments.len() < 2 {
        return Err(JobImportError::Invalid(
            "Use the direct Ashby job link, including its job ID.".to_string(),
        ));
    }
    let board = checked_identifier(&segments[0])?;
    let posting_id = checked_identifier(&segments[1])?;
    let payload = fetch_json(&format!(
        "https://api.ashbyhq.com/posting-api/job-board/{board}"
    ))
    .await?;
    let posting = payload
        .get("jobs")
        .and_then(Value::as_array)
        .and_then(|jobs| {
            jobs.iter().find(|job| {
                job.get("id")
                    .or_else(|| job.get("jobId"))
                    .and_then(Value::as_str)
                    == Some(posting_id)
            })
        })
        .ok_or_else(|| JobImportError::NotFound("That job is no longer available.".to_string()))?;
    let page_title = fetch_text(url.as_str())
        .await
        .ok()
        .and_then(|body| html_title(&body));
    ashby_job_from_payload(url, board, posting_id, posting, page_title.as_deref())
}

fn ashby_job_from_payload(
    url: &Url,
    board: &str,
    posting_id: &str,
    payload: &Value,
    page_title: Option<&str>,
) -> Result<ImportedJob, JobImportError> {
    if payload.get("isListed").and_then(Value::as_bool) == Some(false) {
        return Err(JobImportError::NotFound(
            "That job is no longer available.".to_string(),
        ));
    }
    let title = required_text(payload, "title", "This Ashby job has no role title.")?;
    let location = payload
        .get("location")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let workplace_value = payload
        .get("workplaceType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let description = payload
        .get("descriptionPlain")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            payload
                .get("descriptionHtml")
                .and_then(Value::as_str)
                .map(strip_html)
        })
        .unwrap_or_default();

    Ok(ImportedJob {
        source: "ashby_import".to_string(),
        external_id: posting_id.to_string(),
        company: payload
            .get("companyName")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| page_title.and_then(|value| company_after_page_title(value, &title)))
            .unwrap_or_else(|| humanize_slug(board)),
        title,
        location: location.clone(),
        workplace: normalize_workplace(workplace_value, &location),
        canonical_url: url.as_str().to_string(),
        description,
        compensation: String::new(),
        employment_type: normalize_employment_type(
            payload
                .get("employmentType")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        posted_at_ms: payload
            .get("publishedAt")
            .and_then(Value::as_str)
            .and_then(parse_timestamp_ms),
        verified_at_ms: 0,
        evidence_hash: String::new(),
    })
}

async fn import_smartrecruiters(url: &Url) -> Result<ImportedJob, JobImportError> {
    let segments = path_segments(url)?;
    if segments.len() < 2 {
        return Err(JobImportError::Invalid(
            "Use the direct SmartRecruiters job link, including its job ID.".to_string(),
        ));
    }
    let company_id = checked_identifier(&segments[0])?;
    let posting_slug = checked_identifier(&segments[1])?;
    let posting_id = checked_identifier(posting_slug.split('-').next().unwrap_or_default())?;
    let payload = fetch_json(&format!(
        "https://api.smartrecruiters.com/v1/companies/{company_id}/postings/{posting_id}"
    ))
    .await?;
    smartrecruiters_job_from_payload(url, company_id, posting_id, &payload)
}

fn smartrecruiters_job_from_payload(
    url: &Url,
    company_id: &str,
    posting_id: &str,
    payload: &Value,
) -> Result<ImportedJob, JobImportError> {
    let title = required_text(
        payload,
        "name",
        "This SmartRecruiters job has no role title.",
    )?;
    let location_object = payload.get("location").and_then(Value::as_object);
    let location = ["city", "region", "country"]
        .iter()
        .filter_map(|key| {
            location_object
                .and_then(|value| value.get(*key))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .collect::<Vec<_>>()
        .join(", ");
    let workplace_value = if location_object
        .and_then(|value| value.get("remote"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        "remote"
    } else {
        payload
            .get("workplaceType")
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    let mut sections = Vec::new();
    if let Some(job_sections) = payload
        .get("jobAd")
        .and_then(|value| value.get("sections"))
        .and_then(Value::as_object)
    {
        for section in job_sections.values() {
            if let Some(text) = section
                .get("text")
                .and_then(Value::as_str)
                .map(strip_html)
                .filter(|value| !value.is_empty())
            {
                sections.push(text);
            }
        }
    }

    Ok(ImportedJob {
        source: "smartrecruiters_import".to_string(),
        external_id: posting_id.to_string(),
        company: payload
            .get("company")
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| humanize_slug(company_id)),
        title,
        location: location.clone(),
        workplace: normalize_workplace(workplace_value, &location),
        canonical_url: url.as_str().to_string(),
        description: sections.join("\n\n"),
        compensation: String::new(),
        employment_type: normalize_employment_type(
            payload
                .get("typeOfEmployment")
                .and_then(|value| value.get("label").or_else(|| value.get("id")))
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        posted_at_ms: payload
            .get("releasedDate")
            .and_then(Value::as_str)
            .and_then(parse_timestamp_ms),
        verified_at_ms: 0,
        evidence_hash: String::new(),
    })
}

async fn import_workday(url: &Url) -> Result<ImportedJob, JobImportError> {
    let host = url.host_str().unwrap_or_default();
    let host_parts = host.split('.').collect::<Vec<_>>();
    if host_parts.len() != 4 || host_parts[2..] != ["myworkdayjobs", "com"] {
        return Err(JobImportError::Invalid(
            "Use a direct Workday employer job link.".to_string(),
        ));
    }
    let tenant = checked_identifier(host_parts[0])?;
    let instance = checked_identifier(host_parts[1])?;
    if !instance.starts_with("wd") {
        return Err(JobImportError::Invalid(
            "Use a direct Workday employer job link.".to_string(),
        ));
    }
    let segments = path_segments(url)?;
    let job_index = segments
        .iter()
        .position(|segment| segment == "job")
        .ok_or_else(|| {
            JobImportError::Invalid(
                "Use the direct Workday job page, including its requisition.".to_string(),
            )
        })?;
    if job_index < 1 || segments.len() <= job_index + 1 {
        return Err(JobImportError::Invalid(
            "Use the direct Workday job page, including its requisition.".to_string(),
        ));
    }
    let site = checked_identifier(&segments[job_index - 1])?;
    let external_path = segments[job_index..].join("/");
    let payload = fetch_json(&format!(
        "https://{host}/wday/cxs/{tenant}/{site}/{external_path}"
    ))
    .await?;
    workday_job_from_payload(
        url,
        tenant,
        payload.get("jobPostingInfo").unwrap_or(&payload),
    )
}

fn workday_job_from_payload(
    url: &Url,
    tenant: &str,
    payload: &Value,
) -> Result<ImportedJob, JobImportError> {
    let title = required_text(payload, "title", "This Workday job has no role title.")?;
    let location = payload
        .get("location")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let external_id = payload
        .get("jobReqId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            path_segments(url)
                .ok()
                .and_then(|segments| segments.last().cloned())
        })
        .ok_or_else(|| {
            JobImportError::Temporary("This Workday job has no requisition ID.".to_string())
        })?;

    Ok(ImportedJob {
        source: "workday_import".to_string(),
        external_id,
        company: payload
            .get("hiringOrganization")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| humanize_slug(tenant)),
        title,
        location: location.clone(),
        workplace: normalize_workplace(
            payload
                .get("remoteType")
                .or_else(|| payload.get("workplaceType"))
                .and_then(Value::as_str)
                .unwrap_or_default(),
            &location,
        ),
        canonical_url: url.as_str().to_string(),
        description: payload
            .get("jobDescription")
            .and_then(Value::as_str)
            .map(strip_html)
            .unwrap_or_default(),
        compensation: String::new(),
        employment_type: normalize_employment_type(
            payload
                .get("timeType")
                .or_else(|| payload.get("employmentType"))
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        posted_at_ms: payload
            .get("startDate")
            .and_then(Value::as_str)
            .and_then(parse_timestamp_ms),
        verified_at_ms: 0,
        evidence_hash: String::new(),
    })
}

fn http_client() -> Result<reqwest::Client, JobImportError> {
    reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(10))
        .user_agent(IMPORT_USER_AGENT)
        .build()
        .map_err(|_| JobImportError::Temporary("Bluey could not start the job import.".to_string()))
}

async fn fetch_json(url: &str) -> Result<Value, JobImportError> {
    let response = http_client()?.get(url).send().await.map_err(|_| {
        JobImportError::Temporary("The job site did not answer in time.".to_string())
    })?;
    let body = checked_body(response).await?;
    serde_json::from_slice(&body).map_err(|_| {
        JobImportError::Temporary("The job site returned an unreadable listing.".to_string())
    })
}

async fn fetch_text(url: &str) -> Result<String, JobImportError> {
    let response = http_client()?.get(url).send().await.map_err(|_| {
        JobImportError::Temporary("The job page did not answer in time.".to_string())
    })?;
    let body = checked_body(response).await?;
    String::from_utf8(body.to_vec())
        .map_err(|_| JobImportError::Temporary("The job page was unreadable.".to_string()))
}

async fn checked_body(response: reqwest::Response) -> Result<bytes::Bytes, JobImportError> {
    let status = response.status();
    if matches!(status, StatusCode::NOT_FOUND | StatusCode::GONE) {
        return Err(JobImportError::NotFound(
            "That job is no longer available.".to_string(),
        ));
    }
    if !status.is_success() {
        return Err(JobImportError::Temporary(
            "The job site could not verify this listing right now.".to_string(),
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_JOB_RESPONSE_BYTES)
    {
        return Err(JobImportError::Invalid(
            "That job listing is too large to import safely.".to_string(),
        ));
    }
    let mut stream = response.bytes_stream();
    let mut body = bytes::BytesMut::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| {
            JobImportError::Temporary("The job listing was interrupted.".to_string())
        })?;
        if body.len().saturating_add(chunk.len()) as u64 > MAX_JOB_RESPONSE_BYTES {
            // Dropping the stream cancels an oversized chunked response before
            // it can be buffered in full.
            return Err(JobImportError::Invalid(
                "That job listing is too large to import safely.".to_string(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body.freeze())
}

fn path_segments(url: &Url) -> Result<Vec<String>, JobImportError> {
    url.path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .map(str::to_string)
                .collect()
        })
        .ok_or_else(|| JobImportError::Invalid("Use a direct employer job link.".to_string()))
}

fn checked_identifier(value: &str) -> Result<&str, JobImportError> {
    if value.is_empty()
        || value.len() > 120
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(JobImportError::Invalid(
            "That ATS job link has an invalid identifier.".to_string(),
        ));
    }
    Ok(value)
}

fn required_text(payload: &Value, key: &str, message: &str) -> Result<String, JobImportError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| JobImportError::Temporary(message.to_string()))
}

fn push_text(sections: &mut Vec<String>, value: Option<&Value>) {
    if let Some(value) = value.and_then(Value::as_str).map(str::trim) {
        if !value.is_empty() {
            sections.push(value.to_string());
        }
    }
}

fn normalize_workplace(value: &str, location: &str) -> String {
    let normalized = format!("{} {}", value, location).to_ascii_lowercase();
    if normalized.contains("remote") {
        "Remote".to_string()
    } else if normalized.contains("hybrid") {
        "Hybrid".to_string()
    } else if normalized.contains("on-site") || normalized.contains("onsite") {
        "On-site".to_string()
    } else {
        "Unknown".to_string()
    }
}

fn normalize_employment_type(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    if normalized.contains("intern") {
        "internship".to_string()
    } else if normalized.contains("contract") || normalized.contains("temporary") {
        "contract".to_string()
    } else if normalized.contains("part_time") || normalized == "parttime" {
        "part_time".to_string()
    } else if normalized.contains("full_time") || normalized == "fulltime" {
        "full_time".to_string()
    } else {
        String::new()
    }
}

fn lever_compensation(payload: &Value) -> String {
    let Some(range) = payload.get("salaryRange").and_then(Value::as_object) else {
        return String::new();
    };
    let minimum = number_text(range.get("min"));
    let maximum = number_text(range.get("max"));
    if minimum.is_empty() && maximum.is_empty() {
        return String::new();
    }
    let currency = range
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or("USD");
    let interval = range
        .get("interval")
        .and_then(Value::as_str)
        .unwrap_or_default();
    format!("{currency} {minimum}-{maximum} {interval}")
        .trim()
        .to_string()
}

fn number_text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_f64)
        .map(|number| format!("{number:.0}"))
        .unwrap_or_default()
}

fn parse_timestamp_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.timestamp_millis())
}

fn html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title>")? + "<title>".len();
    let end = lower[start..].find("</title>")? + start;
    let title = decode_entities(html[start..end].trim());
    (!title.is_empty()).then_some(title)
}

fn company_from_page_title(page_title: &str, job_title: &str) -> Option<String> {
    let suffix = format!(" - {job_title}");
    page_title
        .strip_suffix(&suffix)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn company_after_page_title(page_title: &str, job_title: &str) -> Option<String> {
    let prefix = format!("{job_title} @ ");
    page_title
        .strip_prefix(&prefix)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn humanize_slug(value: &str) -> String {
    value
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + characters.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_html(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => {
                in_tag = true;
                if output
                    .chars()
                    .last()
                    .is_some_and(|character| !character.is_whitespace())
                {
                    output.push(' ');
                }
            }
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    decode_entities(&output)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[test]
    fn unknown_hosts_are_not_imported() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = runtime
            .block_on(import_supported_job("https://example.com/jobs/123"))
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn importer_rejects_non_https_and_malformed_ats_paths() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        assert!(matches!(
            runtime.block_on(import_supported_job("http://jobs.lever.co/acme/123")),
            Err(JobImportError::Invalid(_))
        ));
        assert!(matches!(
            runtime.block_on(import_supported_job("https://jobs.lever.co/acme")),
            Err(JobImportError::Invalid(_))
        ));
    }

    #[test]
    fn page_title_and_html_helpers_keep_public_job_text_readable() {
        assert_eq!(
            html_title("<html><title>Acme &amp; Co - Platform Engineer</title></html>"),
            Some("Acme & Co - Platform Engineer".to_string())
        );
        assert_eq!(
            company_from_page_title("Acme & Co - Platform Engineer", "Platform Engineer"),
            Some("Acme & Co".to_string())
        );
        assert_eq!(
            strip_html("<p>Build <b>reliable</b> systems.</p>"),
            "Build reliable systems."
        );
    }

    #[test]
    fn workplace_and_compensation_are_normalized() {
        let payload = serde_json::json!({
            "salaryRange": { "min": 150000, "max": 220000, "currency": "USD", "interval": "year" }
        });
        assert_eq!(normalize_workplace("on-site", "Sunnyvale, CA"), "On-site");
        assert_eq!(lever_compensation(&payload), "USD 150000-220000 year");
    }

    #[test]
    fn lever_payload_keeps_server_owned_freshness_and_listing_facts() {
        let url = Url::parse("https://jobs.lever.co/acme/job-123").unwrap();
        let payload = serde_json::json!({
            "text": "Platform Engineer",
            "createdAt": 1_744_222_396_719_i64,
            "workplaceType": "on-site",
            "categories": { "location": "Sunnyvale, CA", "commitment": "Full-time" },
            "descriptionPlain": "Build Java services on AWS.",
            "lists": [{ "text": "What you bring", "content": "<li>Java</li><li>AWS</li>" }],
            "additional": "<div>Visa Sponsorship</div><div>This position is eligible for visa sponsorship.</div>",
            "salaryRange": { "min": 150000, "max": 220000, "currency": "USD", "interval": "year" }
        });
        let imported =
            lever_job_from_payload(&url, "job-123", "Acme".to_string(), &payload).unwrap();
        assert_eq!(imported.company, "Acme");
        assert_eq!(imported.title, "Platform Engineer");
        assert_eq!(imported.location, "Sunnyvale, CA");
        assert_eq!(imported.workplace, "On-site");
        assert_eq!(imported.posted_at_ms, Some(1_744_222_396_719));
        assert_eq!(imported.compensation, "USD 150000-220000 year");
        assert_eq!(imported.employment_type, "full_time");
        assert!(imported.description.contains("Build Java services on AWS."));
        assert!(imported.description.contains("What you bring"));
        assert!(imported
            .description
            .contains("This position is eligible for visa sponsorship."));
    }

    #[test]
    fn lever_payload_falls_back_when_additional_plain_has_no_text() {
        let url = Url::parse("https://jobs.lever.co/acme/job-123").unwrap();
        for additional_plain in [
            Value::Null,
            Value::String(String::new()),
            Value::String("<div></div>".to_string()),
        ] {
            let mut payload = serde_json::json!({
                "text": "Platform Engineer",
                "additional": "<div>Fallback sponsorship details.</div>"
            });
            payload["additionalPlain"] = additional_plain;

            let imported =
                lever_job_from_payload(&url, "job-123", "Acme".to_string(), &payload).unwrap();
            assert!(imported
                .description
                .contains("Fallback sponsorship details."));
        }

        let payload = serde_json::json!({
            "text": "Platform Engineer",
            "additionalPlain": "Preferred plain details.",
            "additional": "<div>Fallback details that must not be duplicated.</div>"
        });
        let imported =
            lever_job_from_payload(&url, "job-123", "Acme".to_string(), &payload).unwrap();
        assert!(imported.description.contains("Preferred plain details."));
        assert!(!imported.description.contains("Fallback details"));
    }

    #[test]
    fn ashby_payload_keeps_provider_owned_listing_facts() {
        let url = Url::parse("https://jobs.ashbyhq.com/acme/job-123").unwrap();
        let payload = serde_json::json!({
            "id": "job-123",
            "title": "Data Engineer",
            "location": "New York, NY",
            "workplaceType": "Hybrid",
            "employmentType": "FullTime",
            "descriptionHtml": "<p>Build reliable data systems.</p>",
            "publishedAt": "2026-07-15T16:00:00Z",
            "isListed": true
        });
        let imported = ashby_job_from_payload(
            &url,
            "acme",
            "job-123",
            &payload,
            Some("Data Engineer @ Acme Labs"),
        )
        .unwrap();
        assert_eq!(imported.source, "ashby_import");
        assert_eq!(imported.company, "Acme Labs");
        assert_eq!(imported.title, "Data Engineer");
        assert_eq!(imported.workplace, "Hybrid");
        assert_eq!(imported.employment_type, "full_time");
        assert_eq!(imported.description, "Build reliable data systems.");
        assert_eq!(
            imported.posted_at_ms,
            Some(parse_timestamp_ms("2026-07-15T16:00:00Z").unwrap())
        );
    }

    #[test]
    fn smartrecruiters_payload_flattens_public_job_sections() {
        let url =
            Url::parse("https://jobs.smartrecruiters.com/Acme/744000123456789-platform-engineer")
                .unwrap();
        let payload = serde_json::json!({
            "id": "744000123456789",
            "name": "Platform Engineer",
            "company": { "name": "Acme Corp" },
            "location": { "city": "Austin", "region": "TX", "country": "US", "remote": true },
            "typeOfEmployment": { "label": "Full-time" },
            "releasedDate": "2026-07-16T14:30:00Z",
            "jobAd": { "sections": {
                "jobDescription": { "text": "<p>Build platform services.</p>" },
                "qualifications": { "text": "<p>Go and Kubernetes.</p>" }
            }}
        });
        let imported =
            smartrecruiters_job_from_payload(&url, "Acme", "744000123456789", &payload).unwrap();
        assert_eq!(imported.company, "Acme Corp");
        assert_eq!(imported.location, "Austin, TX, US");
        assert_eq!(imported.workplace, "Remote");
        assert_eq!(imported.employment_type, "full_time");
        assert!(imported.description.contains("Build platform services."));
        assert!(imported.description.contains("Go and Kubernetes."));
    }

    #[test]
    fn workday_payload_uses_requisition_and_public_posting_date() {
        let url = Url::parse(
            "https://acme.wd5.myworkdayjobs.com/en-US/Careers/job/Austin/Software-Engineer_R12345",
        )
        .unwrap();
        let payload = serde_json::json!({
            "title": "Software Engineer",
            "jobReqId": "R12345",
            "location": "Austin, TX",
            "remoteType": "Hybrid",
            "timeType": "Full time",
            "hiringOrganization": "Acme Corp",
            "jobDescription": "<p>Build Java services on AWS.</p>",
            "startDate": "2026-07-14T00:00:00Z"
        });
        let imported = workday_job_from_payload(&url, "acme", &payload).unwrap();
        assert_eq!(imported.source, "workday_import");
        assert_eq!(imported.external_id, "R12345");
        assert_eq!(imported.company, "Acme Corp");
        assert_eq!(imported.workplace, "Hybrid");
        assert_eq!(imported.employment_type, "full_time");
        assert_eq!(imported.description, "Build Java services on AWS.");
    }

    #[test]
    fn direct_urls_for_all_five_public_ats_families_are_recognized() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        assert!(matches!(
            runtime.block_on(import_supported_job("https://jobs.ashbyhq.com/acme")),
            Err(JobImportError::Invalid(_))
        ));
        assert!(matches!(
            runtime.block_on(import_supported_job(
                "https://jobs.smartrecruiters.com/acme"
            )),
            Err(JobImportError::Invalid(_))
        ));
        assert!(matches!(
            runtime.block_on(import_supported_job(
                "https://acme.wd5.myworkdayjobs.com/en-US/Careers"
            )),
            Err(JobImportError::Invalid(_))
        ));
    }

    #[test]
    fn import_rejects_non_default_ports_and_ambiguous_workday_hosts_before_fetching() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        assert!(matches!(
            runtime.block_on(import_supported_job(
                "https://jobs.ashbyhq.com:444/acme/job-123"
            )),
            Err(JobImportError::Invalid(_))
        ));
        assert!(matches!(
            runtime.block_on(import_supported_job(
                "https://acme.wd5.extra.myworkdayjobs.com/en-US/Careers/job/Austin/Software-Engineer_R12345"
            )),
            Err(JobImportError::Invalid(_))
        ));
    }

    #[test]
    fn importer_and_discovery_share_the_strict_ats_url_grammar() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        assert!(matches!(
            runtime.block_on(import_supported_job(
                "https://boards.greenhouse.io/acme/jobs/jobs/123"
            )),
            Err(JobImportError::Invalid(_))
        ));
        assert!(matches!(
            runtime.block_on(import_supported_job(
                "https://acme.wd5.myworkdayjobs.com/en-US/Careers/job/Austin/job/R12345"
            )),
            Err(JobImportError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn checked_body_stops_an_oversized_chunked_response_without_buffering_it() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).await;
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                .await;
            for chunk in [vec![b'a'; 700_000], vec![b'b'; 700_000]] {
                let _ = stream
                    .write_all(format!("{:X}\r\n", chunk.len()).as_bytes())
                    .await;
                let _ = stream.write_all(&chunk).await;
                let _ = stream.write_all(b"\r\n").await;
            }
        });
        let response = reqwest::Client::new()
            .get(format!("http://{address}/oversized"))
            .send()
            .await
            .unwrap();
        assert!(matches!(
            checked_body(response).await,
            Err(JobImportError::Invalid(_))
        ));
        server.await.unwrap();
    }
}
