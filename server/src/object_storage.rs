//! Small S3-compatible object storage client used for Bluey Cloud artifact bytes.
//!
//! Cloudflare R2 speaks the S3 SigV4 API. Keeping the client narrow avoids a
//! large provider SDK dependency in the server binary.

use anyhow::{anyhow, Context, Result};
use bytes::{Bytes, BytesMut};
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{header, Method, Url};
use sha2::{Digest, Sha256};

use crate::config::ObjectStorageConfig;
use crate::db::object_uploads::{self, StorageScope};
use crate::db::DbPool;

type HmacSha256 = Hmac<Sha256>;

const DEFAULT_ACCOUNT_QUOTA_BYTES: i64 = 1024 * 1024 * 1024;
const DEFAULT_DAILY_QUOTA_BYTES: i64 = 256 * 1024 * 1024;
const DEFAULT_ACCOUNT_MAX_OBJECTS: i64 = 10_000;
const MAX_ACCOUNT_PREFIX_DELETE_ROUNDS: usize = 10_000;
const MAX_LIST_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ObjectStorage {
    config: ObjectStorageConfig,
    http: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct StoredObject {
    pub bytes: Bytes,
    pub content_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadLimits {
    pub max_object_bytes: i64,
    pub max_account_bytes: i64,
    pub max_daily_bytes: i64,
    pub max_account_objects: i64,
}

impl ObjectStorage {
    pub fn new(config: ObjectStorageConfig) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { config, http }
    }

    pub fn artifact_key(&self, account_id: &str, artifact_id: &str) -> String {
        let prefix = self.config.key_prefix.trim_matches('/');
        if prefix.is_empty() {
            format!("accounts/{account_id}/context/{artifact_id}")
        } else {
            format!("{prefix}/accounts/{account_id}/context/{artifact_id}")
        }
    }

    pub fn artifact_upload_key(&self, account_id: &str, artifact_id: &str, sha256: &str) -> String {
        self.account_key(
            &format!("context/{artifact_id}/sha256/{sha256}"),
            account_id,
        )
    }

    pub fn audit_upload_key(
        &self,
        account_id: &str,
        session_id: &str,
        bundle_id: &str,
        sha256: &str,
    ) -> String {
        self.account_key(
            &format!("sessions/{session_id}/audit/{bundle_id}/sha256/{sha256}.json"),
            account_id,
        )
    }

    pub fn resume_source_key(
        &self,
        account_id: &str,
        asset_id: &str,
        sha256: &str,
        extension: &str,
    ) -> String {
        let extension = extension
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        self.account_key(
            &format!("jobs/resumes/{asset_id}/sha256/{sha256}.{extension}"),
            account_id,
        )
    }

    pub fn browser_profile_snapshot_key(
        &self,
        account_id: &str,
        browser_profile_id: &str,
        generation: i64,
        sha256: &str,
    ) -> String {
        let profile_scope = hex::encode(Sha256::digest(browser_profile_id.as_bytes()));
        self.account_key(
            &format!(
                "jobs/browser-profiles/{profile_scope}/generation/{generation}/sha256/{sha256}.enc"
            ),
            account_id,
        )
    }

    pub fn jobs_submission_bundle_key(
        &self,
        account_id: &str,
        application_id: &str,
        receipt_id: &str,
        bundle_id: &str,
        sha256: &str,
    ) -> String {
        let application_scope = hex::encode(Sha256::digest(application_id.as_bytes()));
        let receipt_scope = hex::encode(Sha256::digest(receipt_id.as_bytes()));
        let bundle_scope = hex::encode(Sha256::digest(bundle_id.as_bytes()));
        self.account_key(
            &format!(
                "jobs/applications/{application_scope}/receipts/{receipt_scope}/bundles/\
                 {bundle_scope}/sha256/{sha256}.json"
            ),
            account_id,
        )
    }

    pub fn global_candidate_archive_key(&self, candidate_id: &str, content_hash: &str) -> String {
        let prefix = self.config.key_prefix.trim_matches('/');
        let suffix = format!("global/jobs/candidates/{candidate_id}/sha256/{content_hash}.json");
        if prefix.is_empty() {
            suffix
        } else {
            format!("{prefix}/{suffix}")
        }
    }

    pub fn key_belongs_to_account(&self, key: &str, account_id: &str) -> bool {
        let prefix = self.config.key_prefix.trim_matches('/');
        let expected = if prefix.is_empty() {
            format!("accounts/{account_id}/")
        } else {
            format!("{prefix}/accounts/{account_id}/")
        };
        key.starts_with(&expected)
    }

    pub fn retention_days(&self) -> i64 {
        self.config.retention_days
    }

    pub fn max_object_bytes(&self) -> usize {
        self.config.max_object_bytes
    }

    pub fn upload_limits(&self) -> UploadLimits {
        UploadLimits {
            max_object_bytes: i64::try_from(self.config.max_object_bytes).unwrap_or(i64::MAX),
            max_account_bytes: positive_env_i64(
                "BLUEY_UPLOAD_ACCOUNT_QUOTA_BYTES",
                DEFAULT_ACCOUNT_QUOTA_BYTES,
            ),
            max_daily_bytes: positive_env_i64(
                "BLUEY_UPLOAD_DAILY_QUOTA_BYTES",
                DEFAULT_DAILY_QUOTA_BYTES,
            ),
            max_account_objects: positive_env_i64(
                "BLUEY_UPLOAD_ACCOUNT_MAX_OBJECTS",
                DEFAULT_ACCOUNT_MAX_OBJECTS,
            ),
        }
    }

    pub async fn put(&self, key: &str, bytes: Bytes, content_type: &str) -> Result<()> {
        let payload_hash = sha256_hex(&bytes);
        let url = self.object_url(key)?;
        let auth = self.authorization(Method::PUT, &url, &payload_hash)?;
        let response = self
            .http
            .put(url)
            .header(header::CONTENT_TYPE, content_type)
            .header("x-amz-date", auth.amz_date)
            .header("x-amz-content-sha256", payload_hash)
            .header(header::AUTHORIZATION, auth.authorization)
            .body(bytes)
            .send()
            .await
            .context("put object")?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(anyhow!(
            "object put failed with status {status}: {}",
            truncate(&body, 256)
        ))
    }

    pub async fn get(&self, key: &str) -> Result<StoredObject> {
        let payload_hash = sha256_hex([]);
        let url = self.object_url(key)?;
        let auth = self.authorization(Method::GET, &url, &payload_hash)?;
        let response = self
            .http
            .get(url)
            .header("x-amz-date", auth.amz_date)
            .header("x-amz-content-sha256", payload_hash)
            .header(header::AUTHORIZATION, auth.authorization)
            .send()
            .await
            .context("get object")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "object get failed with status {status}: {}",
                truncate(&body, 256)
            ));
        }
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("application/octet-stream")
            .to_string();
        let maximum_bytes = self.config.max_object_bytes;
        if response
            .content_length()
            .is_some_and(|length| length > maximum_bytes as u64)
        {
            return Err(anyhow!(
                "object body exceeds configured maximum of {maximum_bytes} bytes"
            ));
        }
        let mut response = response;
        let mut body = BytesMut::with_capacity(
            response
                .content_length()
                .and_then(|length| usize::try_from(length).ok())
                .unwrap_or_default()
                .min(maximum_bytes),
        );
        while let Some(chunk) = response.chunk().await.context("read object body")? {
            if body
                .len()
                .checked_add(chunk.len())
                .is_none_or(|length| length > maximum_bytes)
            {
                return Err(anyhow!(
                    "object body exceeds configured maximum of {maximum_bytes} bytes"
                ));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(StoredObject {
            bytes: body.freeze(),
            content_type,
        })
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let payload_hash = sha256_hex([]);
        let url = self.object_url(key)?;
        let auth = self.authorization(Method::DELETE, &url, &payload_hash)?;
        let response = self
            .http
            .delete(url)
            .header("x-amz-date", auth.amz_date)
            .header("x-amz-content-sha256", payload_hash)
            .header(header::AUTHORIZATION, auth.authorization)
            .send()
            .await
            .context("delete object")?;
        if response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(anyhow!(
            "object delete failed with status {status}: {}",
            truncate(&body, 256)
        ))
    }

    /// Delete every object under the authenticated account namespace, including
    /// legacy bytes that predate the durable object-upload ledger. Each page is
    /// deleted before listing the prefix again so a continuation token cannot
    /// skip keys removed from an earlier page.
    pub async fn delete_all_account_objects(&self, account_id: &str) -> Result<usize> {
        let prefix = self.account_prefix(account_id)?;
        let mut deleted = 0usize;
        for _ in 0..MAX_ACCOUNT_PREFIX_DELETE_ROUNDS {
            let keys = self.list_object_keys(&prefix).await?;
            if keys.is_empty() {
                return Ok(deleted);
            }
            for key in keys {
                if !self.key_belongs_to_account(&key, account_id) {
                    return Err(anyhow!("listed object is outside the account namespace"));
                }
                self.delete(&key).await?;
                deleted = deleted
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("account object deletion count overflow"))?;
            }
        }
        Err(anyhow!(
            "account object prefix did not become empty within the deletion bound"
        ))
    }

    async fn list_object_keys(&self, prefix: &str) -> Result<Vec<String>> {
        let url = self.list_objects_url(prefix)?;
        let payload_hash = sha256_hex([]);
        let auth = self.authorization(Method::GET, &url, &payload_hash)?;
        let response = self
            .http
            .get(url)
            .header("x-amz-date", auth.amz_date)
            .header("x-amz-content-sha256", payload_hash)
            .header(header::AUTHORIZATION, auth.authorization)
            .send()
            .await
            .context("list account objects")?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "account object listing failed with status {}",
                response.status()
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_LIST_RESPONSE_BYTES as u64)
        {
            return Err(anyhow!("account object listing response is too large"));
        }
        let bytes = response
            .bytes()
            .await
            .context("read account object listing")?;
        if bytes.len() > MAX_LIST_RESPONSE_BYTES {
            return Err(anyhow!("account object listing response is too large"));
        }
        parse_list_object_keys(&bytes, prefix)
    }

    fn list_objects_url(&self, prefix: &str) -> Result<Url> {
        let mut url = self.bucket_url()?;
        url.set_query(Some(&format!(
            "list-type=2&max-keys=1000&prefix={}",
            encode_segment(prefix)
        )));
        Ok(url)
    }

    fn object_url(&self, key: &str) -> Result<Url> {
        let endpoint = self.config.endpoint_url.trim_end_matches('/');
        let encoded_path = [encode_segment(&self.config.bucket), encode_key_path(key)].join("/");
        Url::parse(&format!("{endpoint}/{encoded_path}")).context("parse object URL")
    }

    fn bucket_url(&self) -> Result<Url> {
        let endpoint = self.config.endpoint_url.trim_end_matches('/');
        Url::parse(&format!(
            "{endpoint}/{}",
            encode_segment(&self.config.bucket)
        ))
        .context("parse object storage bucket URL")
    }

    fn account_prefix(&self, account_id: &str) -> Result<String> {
        if account_id.is_empty()
            || account_id.len() > 240
            || !account_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(anyhow!("invalid account object namespace"));
        }
        let prefix = self.config.key_prefix.trim_matches('/');
        Ok(if prefix.is_empty() {
            format!("accounts/{account_id}/")
        } else {
            format!("{prefix}/accounts/{account_id}/")
        })
    }

    fn account_key(&self, suffix: &str, account_id: &str) -> String {
        let prefix = self.config.key_prefix.trim_matches('/');
        if prefix.is_empty() {
            format!("accounts/{account_id}/{suffix}")
        } else {
            format!("{prefix}/accounts/{account_id}/{suffix}")
        }
    }

    fn authorization(&self, method: Method, url: &Url, payload_hash: &str) -> Result<SignedAuth> {
        let now = Utc::now();
        let date = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let host = host_header(url)?;
        let canonical_uri = if url.path().is_empty() {
            "/".to_string()
        } else {
            url.path().to_string()
        };
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_headers =
            format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n");
        let canonical_query = canonical_query(url);
        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method.as_str(),
            canonical_uri,
            canonical_query,
            canonical_headers,
            signed_headers,
            payload_hash
        );
        let canonical_hash = sha256_hex(canonical_request.as_bytes());
        let scope = format!("{}/{}/s3/aws4_request", date, self.config.region);
        let string_to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{canonical_hash}");
        let signing_key = signing_key(&self.config.secret_access_key, &date, &self.config.region);
        let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.config.access_key_id
        );
        Ok(SignedAuth {
            amz_date,
            authorization,
        })
    }
}

fn parse_list_object_keys(xml: &[u8], prefix: &str) -> Result<Vec<String>> {
    use quick_xml::{events::Event, Reader};

    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut elements = Vec::<Vec<u8>>::new();
    let mut keys = Vec::new();
    let mut listed_prefix = None;
    let mut key_count = None;
    let mut is_truncated = None;
    let mut saw_prefix = false;
    let mut saw_key_count = false;
    let mut saw_is_truncated = false;
    let mut open_contents_key_count = None;
    let mut open_key_had_value = None;
    let mut root_closed = false;
    loop {
        match reader
            .read_event()
            .context("parse account object listing")?
        {
            Event::Start(element) => {
                let name = element.name().as_ref().to_vec();
                if root_closed {
                    return Err(anyhow!(
                        "account object listing contains content after its root element"
                    ));
                }
                if elements.is_empty() {
                    if name.as_slice() != b"ListBucketResult" {
                        return Err(anyhow!(
                            "account object listing has an unexpected root element"
                        ));
                    }
                } else if matches!(
                    elements.as_slice(),
                    [root] if root.as_slice() == b"ListBucketResult"
                ) && matches!(name.as_slice(), b"Prefix" | b"KeyCount" | b"IsTruncated")
                {
                    let already_seen = match name.as_slice() {
                        b"Prefix" => std::mem::replace(&mut saw_prefix, true),
                        b"KeyCount" => std::mem::replace(&mut saw_key_count, true),
                        b"IsTruncated" => std::mem::replace(&mut saw_is_truncated, true),
                        _ => unreachable!(),
                    };
                    if already_seen {
                        return Err(anyhow!("account object listing repeats an authority field"));
                    }
                } else if name.as_slice() == b"Contents" {
                    if !matches!(
                        elements.as_slice(),
                        [root] if root.as_slice() == b"ListBucketResult"
                    ) || open_contents_key_count.replace(keys.len()).is_some()
                    {
                        return Err(anyhow!("account object listing contents are malformed"));
                    }
                } else if name.as_slice() == b"Key"
                    && (!matches!(
                        elements.as_slice(),
                        [root, contents]
                            if root.as_slice() == b"ListBucketResult"
                                && contents.as_slice() == b"Contents"
                    ) || open_key_had_value.replace(false).is_some())
                {
                    return Err(anyhow!("account object listing key is malformed"));
                }
                elements.push(name);
            }
            Event::Empty(_) => {
                return Err(anyhow!(
                    "account object listing contains an unexpected empty element"
                ));
            }
            Event::Text(value) => {
                let value = value
                    .unescape()
                    .context("decode account object listing value")?
                    .into_owned();
                match elements.as_slice() {
                    [root, field]
                        if root.as_slice() == b"ListBucketResult"
                            && field.as_slice() == b"Prefix"
                            && listed_prefix.replace(value.clone()).is_some() =>
                    {
                        return Err(anyhow!(
                            "account object listing repeats its requested prefix"
                        ));
                    }
                    [root, field]
                        if root.as_slice() == b"ListBucketResult"
                            && field.as_slice() == b"KeyCount" =>
                    {
                        let value = value
                            .parse::<usize>()
                            .context("parse account object listing key count")?;
                        if value > 1_000 || key_count.replace(value).is_some() {
                            return Err(anyhow!("account object listing has an invalid key count"));
                        }
                    }
                    [root, field]
                        if root.as_slice() == b"ListBucketResult"
                            && field.as_slice() == b"IsTruncated" =>
                    {
                        let value = match value.as_str() {
                            "true" => true,
                            "false" => false,
                            _ => {
                                return Err(anyhow!(
                                    "account object listing has an invalid truncation marker"
                                ));
                            }
                        };
                        if is_truncated.replace(value).is_some() {
                            return Err(anyhow!(
                                "account object listing repeats its truncation marker"
                            ));
                        }
                    }
                    [root, contents, field]
                        if root.as_slice() == b"ListBucketResult"
                            && contents.as_slice() == b"Contents"
                            && field.as_slice() == b"Key" =>
                    {
                        if open_key_had_value != Some(false)
                            || value.is_empty()
                            || !value.starts_with(prefix)
                            || keys.len() >= 1_000
                            || keys.contains(&value)
                        {
                            return Err(anyhow!(
                                "account object listing escaped its requested prefix"
                            ));
                        }
                        open_key_had_value = Some(true);
                        keys.push(value);
                    }
                    [root] if root.as_slice() == b"ListBucketResult" && !value.is_empty() => {
                        return Err(anyhow!(
                            "account object listing contains text inside its result root"
                        ));
                    }
                    [] if !value.is_empty() => {
                        return Err(anyhow!(
                            "account object listing contains text outside its result root"
                        ));
                    }
                    _ => {}
                }
            }
            Event::End(element) => {
                let Some(opened) = elements.pop() else {
                    return Err(anyhow!(
                        "account object listing contains an unmatched closing element"
                    ));
                };
                if opened.as_slice() != element.name().as_ref() {
                    return Err(anyhow!(
                        "account object listing contains mismatched elements"
                    ));
                }
                if opened.as_slice() == b"Key" && open_key_had_value.take() != Some(true) {
                    return Err(anyhow!("account object listing key is empty"));
                }
                if opened.as_slice() == b"Contents" {
                    let Some(previous_count) = open_contents_key_count.take() else {
                        return Err(anyhow!("account object listing contents are malformed"));
                    };
                    if keys.len() != previous_count + 1 {
                        return Err(anyhow!(
                            "account object listing contents must contain exactly one key"
                        ));
                    }
                }
                if elements.is_empty() {
                    root_closed = true;
                }
            }
            Event::Eof => {
                if !elements.is_empty() {
                    return Err(anyhow!("account object listing is incomplete"));
                }
                break;
            }
            Event::CData(_) => {
                return Err(anyhow!(
                    "account object listing contains an unsupported value"
                ));
            }
            Event::Decl(_) | Event::Comment(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    if !root_closed {
        return Err(anyhow!("account object listing is missing its result root"));
    }
    if listed_prefix.as_deref() != Some(prefix) {
        return Err(anyhow!(
            "account object listing does not match its requested prefix"
        ));
    }
    let key_count =
        key_count.ok_or_else(|| anyhow!("account object listing is missing its key count"))?;
    let is_truncated = is_truncated
        .ok_or_else(|| anyhow!("account object listing is missing its truncation marker"))?;
    if key_count != keys.len() {
        return Err(anyhow!(
            "account object listing key count does not match its contents"
        ));
    }
    if is_truncated && keys.is_empty() {
        return Err(anyhow!(
            "account object listing is truncated without a deletable page"
        ));
    }
    Ok(keys)
}

fn canonical_query(url: &Url) -> String {
    let mut pairs = url
        .query_pairs()
        .map(|(key, value)| (encode_segment(&key), encode_segment(&value)))
        .collect::<Vec<_>>();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

pub fn spawn_cleanup_worker(
    pool: DbPool,
    artifact_config: Option<ObjectStorageConfig>,
    audit_config: Option<ObjectStorageConfig>,
) -> Option<tokio::task::JoinHandle<()>> {
    let mut stores = Vec::new();
    if let Some(config) = artifact_config {
        stores.push((StorageScope::Artifact, ObjectStorage::new(config)));
    }
    if let Some(config) = audit_config {
        stores.push((StorageScope::Audit, ObjectStorage::new(config)));
    }
    if stores.is_empty() {
        return None;
    }

    let interval_seconds =
        positive_env_i64("BLUEY_OBJECT_CLEANUP_INTERVAL_SECONDS", 60).clamp(5, 3_600) as u64;
    Some(tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_seconds));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            for (scope, storage) in &stores {
                run_cleanup_pass(&pool, *scope, storage).await;
            }
        }
    }))
}

async fn run_cleanup_pass(pool: &DbPool, scope: StorageScope, storage: &ObjectStorage) {
    const MAX_BATCHES_PER_PASS: usize = 4;
    const BATCH_SIZE: i64 = 100;

    for _ in 0..MAX_BATCHES_PER_PASS {
        let now = unix_now_ms();
        let stale_before_ms = now.saturating_sub(24 * 60 * 60 * 1000);
        let jobs = match object_uploads::claim_global_cleanup_jobs(
            pool,
            scope,
            now,
            stale_before_ms,
            BATCH_SIZE,
        ) {
            Ok(jobs) => jobs,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    storage_scope = scope.as_str(),
                    "object cleanup worker failed to claim jobs"
                );
                return;
            }
        };
        let job_count = jobs.len();
        for job in jobs {
            let result = if storage.key_belongs_to_account(&job.object_key, &job.account_id) {
                storage.delete(&job.object_key).await
            } else {
                Err(anyhow!("object cleanup key is outside account scope"))
            };
            match result {
                Ok(()) => {
                    if let Err(error) =
                        object_uploads::mark_cleanup_succeeded(pool, &job.upload_id, unix_now_ms())
                    {
                        tracing::error!(
                            error = %error,
                            storage_scope = scope.as_str(),
                            "object cleanup worker could not complete metadata"
                        );
                    }
                }
                Err(error) => {
                    if let Err(index_error) = object_uploads::mark_cleanup_failed(
                        pool,
                        &job.upload_id,
                        &error.to_string(),
                        unix_now_ms(),
                    ) {
                        tracing::error!(
                            error = %index_error,
                            storage_scope = scope.as_str(),
                            "object cleanup worker could not persist retry"
                        );
                    }
                    tracing::warn!(
                        error = %error,
                        account_id_hash = %cue_core::account_id_hash_prefix(&job.account_id),
                        storage_scope = scope.as_str(),
                        "object cleanup worker will retry deletion"
                    );
                }
            }
        }
        if job_count < BATCH_SIZE as usize {
            break;
        }
    }
}

#[derive(Debug, Clone)]
struct SignedAuth {
    amz_date: String,
    authorization: String,
}

pub fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}

fn signing_key(secret: &str, date: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    hmac_sha256(&k_service, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn host_header(url: &Url) -> Result<String> {
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("object URL is missing host"))?;
    match url.port() {
        Some(port) => Ok(format!("{host}:{port}")),
        None => Ok(host.to_string()),
    }
}

fn encode_key_path(key: &str) -> String {
    key.split('/')
        .filter(|segment| !segment.is_empty())
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_segment(segment: &str) -> String {
    let mut out = String::new();
    for byte in segment.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    value.chars().take(max_chars).collect()
}

fn positive_env_i64(name: &str, default: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::object_uploads::{
        record_put_failure, reserve_upload, NewObjectUpload, ObjectKind,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn object_key_is_account_scoped() {
        let storage = ObjectStorage::new(ObjectStorageConfig {
            endpoint_url: "https://example.invalid".into(),
            bucket: "bucket".into(),
            access_key_id: "ak".into(),
            secret_access_key: "sk".into(),
            region: "auto".into(),
            key_prefix: "bluey-cloud".into(),
            retention_days: 365,
            max_object_bytes: 10,
        });
        let key = storage.artifact_key("acct", "artifact");
        assert_eq!(key, "bluey-cloud/accounts/acct/context/artifact");
        assert!(storage.key_belongs_to_account(&key, "acct"));
        assert!(!storage.key_belongs_to_account(&key, "other"));

        let hash = "a".repeat(64);
        let content_key = storage.artifact_upload_key("acct", "artifact", &hash);
        assert_eq!(
            content_key,
            format!("bluey-cloud/accounts/acct/context/artifact/sha256/{hash}")
        );
        let audit_key = storage.audit_upload_key("acct", "session", "bundle", &hash);
        assert_eq!(
            audit_key,
            format!("bluey-cloud/accounts/acct/sessions/session/audit/bundle/sha256/{hash}.json")
        );
        let archive_key = storage.global_candidate_archive_key("candidate", &hash);
        assert_eq!(
            archive_key,
            format!("bluey-cloud/global/jobs/candidates/candidate/sha256/{hash}.json")
        );
        let bundle_key = storage.jobs_submission_bundle_key(
            "acct",
            "app-secret",
            "receipt-secret",
            "bundle-secret",
            &hash,
        );
        assert!(bundle_key.starts_with("bluey-cloud/accounts/acct/jobs/applications/"));
        assert!(bundle_key.ends_with(&format!("/sha256/{hash}.json")));
        assert!(!bundle_key.contains("app-secret"));
        assert!(!bundle_key.contains("receipt-secret"));
        assert!(!bundle_key.contains("bundle-secret"));
    }

    #[tokio::test]
    async fn object_get_rejects_a_body_larger_than_the_configured_maximum() {
        let object_store = MockServer::start().await;
        let storage = ObjectStorage::new(ObjectStorageConfig {
            endpoint_url: object_store.uri(),
            bucket: "bucket".into(),
            access_key_id: "ak".into(),
            secret_access_key: "sk".into(),
            region: "auto".into(),
            key_prefix: "bluey-cloud".into(),
            retention_days: 365,
            max_object_bytes: 4,
        });
        Mock::given(method("GET"))
            .and(path("/bucket/oversized"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0_u8; 5]))
            .expect(1)
            .mount(&object_store)
            .await;

        let error = storage.get("oversized").await.unwrap_err();
        assert!(error
            .to_string()
            .contains("object body exceeds configured maximum of 4 bytes"));
    }

    #[tokio::test]
    async fn account_prefix_delete_removes_legacy_objects_until_listing_is_empty() {
        let object_store = MockServer::start().await;
        let storage = ObjectStorage::new(ObjectStorageConfig {
            endpoint_url: object_store.uri(),
            bucket: "bucket".into(),
            access_key_id: "ak".into(),
            secret_access_key: "sk".into(),
            region: "auto".into(),
            key_prefix: "bluey-cloud".into(),
            retention_days: 365,
            max_object_bytes: 100,
        });
        let prefix = "bluey-cloud/accounts/acct-one/";
        let first_key = format!("{prefix}jobs/browser-profiles/legacy-one.enc");
        let second_key = format!("{prefix}jobs/resumes/legacy-two.pdf");
        let list_calls = Arc::new(AtomicUsize::new(0));
        let list_responder = Arc::clone(&list_calls);
        let first_key_for_xml = first_key.clone();
        let second_key_for_xml = second_key.clone();
        Mock::given(method("GET"))
            .and(path("/bucket"))
            .and(query_param("list-type", "2"))
            .and(query_param("max-keys", "1000"))
            .and(query_param("prefix", prefix))
            .respond_with(move |_request: &wiremock::Request| {
                if list_responder.fetch_add(1, Ordering::SeqCst) == 0 {
                    ResponseTemplate::new(200).set_body_string(format!(
                        "<ListBucketResult><Prefix>{prefix}</Prefix><KeyCount>2</KeyCount>\
                         <IsTruncated>false</IsTruncated>\
                         <Contents><Key>{first_key_for_xml}</Key></Contents>\
                         <Contents><Key>{second_key_for_xml}</Key></Contents></ListBucketResult>"
                    ))
                } else {
                    ResponseTemplate::new(200).set_body_string(format!(
                        "<ListBucketResult><Prefix>{prefix}</Prefix><KeyCount>0</KeyCount>\
                         <IsTruncated>false</IsTruncated></ListBucketResult>"
                    ))
                }
            })
            .expect(2)
            .mount(&object_store)
            .await;
        for key in [&first_key, &second_key] {
            Mock::given(method("DELETE"))
                .and(path(format!("/bucket/{key}")))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&object_store)
                .await;
        }

        assert_eq!(
            storage
                .delete_all_account_objects("acct-one")
                .await
                .unwrap(),
            2
        );
        assert_eq!(list_calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn account_object_listing_rejects_a_key_outside_the_requested_prefix() {
        let error = parse_list_object_keys(
            b"<ListBucketResult><Prefix>bluey-cloud/accounts/acct-one/</Prefix>\
              <KeyCount>1</KeyCount><IsTruncated>false</IsTruncated>\
              <Contents><Key>bluey-cloud/accounts/other/private</Key></Contents>\
              </ListBucketResult>",
            "bluey-cloud/accounts/acct-one/",
        )
        .unwrap_err();
        assert!(error.to_string().contains("escaped its requested prefix"));
    }

    #[test]
    fn account_object_listing_rejects_non_authoritative_success_bodies() {
        let prefix = "bluey-cloud/accounts/acct-one/";
        for body in [
            b"".as_slice(),
            b"<Error><Code>AccessDenied</Code></Error>".as_slice(),
            b"<ListBucketResult></ListBucketResult>".as_slice(),
            b"<ListBucketResult><Prefix>bluey-cloud/accounts/acct-one/</Prefix>\
              <KeyCount>0</KeyCount></ListBucketResult>"
                .as_slice(),
            b"<ListBucketResult><Prefix>bluey-cloud/accounts/acct-one/</Prefix>\
              <KeyCount>0</KeyCount><IsTruncated>false</IsTruncated>\
              <Contents><Key></Key></Contents></ListBucketResult>"
                .as_slice(),
            b"<ListBucketResult><Prefix>bluey-cloud/accounts/acct-one/</Prefix>\
              <KeyCount>0</KeyCount><IsTruncated>false</IsTruncated>\
              <Contents></Contents></ListBucketResult>"
                .as_slice(),
            b"<ListBucketResult>garbage\
              <Prefix>bluey-cloud/accounts/acct-one/</Prefix><KeyCount>0</KeyCount>\
              <IsTruncated>false</IsTruncated></ListBucketResult>"
                .as_slice(),
            b"<ListBucketResult><Prefix></Prefix>\
              <Prefix>bluey-cloud/accounts/acct-one/</Prefix><KeyCount>0</KeyCount>\
              <IsTruncated>false</IsTruncated></ListBucketResult>"
                .as_slice(),
        ] {
            assert!(parse_list_object_keys(body, prefix).is_err());
        }
    }

    #[test]
    fn account_object_listing_rejects_inconsistent_empty_pages() {
        let prefix = "bluey-cloud/accounts/acct-one/";
        let error = parse_list_object_keys(
            b"<ListBucketResult><Prefix>bluey-cloud/accounts/acct-one/</Prefix>\
              <KeyCount>0</KeyCount><IsTruncated>true</IsTruncated></ListBucketResult>",
            prefix,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("truncated without a deletable page"));
    }

    #[test]
    fn account_object_listing_query_is_sigv4_canonical() {
        let storage = ObjectStorage::new(ObjectStorageConfig {
            endpoint_url: "https://objects.example.test".into(),
            bucket: "bucket".into(),
            access_key_id: "ak".into(),
            secret_access_key: "sk".into(),
            region: "auto".into(),
            key_prefix: "bluey cloud".into(),
            retention_days: 365,
            max_object_bytes: 100,
        });
        let url = storage
            .list_objects_url("bluey cloud/accounts/acct-one/")
            .unwrap();
        let expected = "list-type=2&max-keys=1000&prefix=bluey%20cloud%2Faccounts%2Facct-one%2F";

        assert_eq!(url.query(), Some(expected));
        assert_eq!(canonical_query(&url), expected);
        assert!(!url.as_str().contains('+'));
    }

    #[tokio::test]
    async fn cleanup_worker_pass_deletes_stale_pending_object() {
        let object_store = MockServer::start().await;
        let storage = ObjectStorage::new(ObjectStorageConfig {
            endpoint_url: object_store.uri(),
            bucket: "bucket".into(),
            access_key_id: "ak".into(),
            secret_access_key: "sk".into(),
            region: "auto".into(),
            key_prefix: "bluey-cloud".into(),
            retention_days: 365,
            max_object_bytes: 100,
        });
        let pool = crate::db::open_pool(":memory:".as_ref()).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts(id, email, password_hash)
                 VALUES ('acct_worker', 'worker@example.test', 'hash')",
                [],
            )
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO cloud_sessions (
                    account_id, session_id, title, status, created_at_ms, updated_at_ms,
                    metadata_json
                 ) VALUES ('acct_worker', 'session_worker', 'Worker', 'active', 1, 1, '{}')",
                [],
            )
            .unwrap();
        let hash = sha256_hex("worker payload");
        let key = storage.artifact_upload_key("acct_worker", "artifact_worker", &hash);
        let reservation = reserve_upload(
            &pool,
            &NewObjectUpload {
                account_id: "acct_worker".into(),
                object_kind: ObjectKind::Artifact,
                logical_id: "artifact_worker".into(),
                session_id: Some("session_worker".into()),
                storage_scope: StorageScope::Artifact,
                object_key: key.clone(),
                size_bytes: 14,
                sha256: hash,
                content_type: "text/plain".into(),
                expires_at_ms: 86_401_000,
                metadata_json: serde_json::json!({}),
                now_ms: 1_000,
                limits: UploadLimits {
                    max_object_bytes: 100,
                    max_account_bytes: 1_000,
                    max_daily_bytes: 1_000,
                    max_account_objects: 10,
                },
            },
        )
        .unwrap();
        record_put_failure(&pool, &reservation.upload.id, "uncertain PUT", 1_100).unwrap();

        Mock::given(method("DELETE"))
            .and(path(format!("/bucket/{key}")))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&object_store)
            .await;
        run_cleanup_pass(&pool, StorageScope::Artifact, &storage).await;

        let state: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state FROM object_uploads WHERE id = ?1",
                rusqlite::params![reservation.upload.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(state, "deleted");
    }
}
