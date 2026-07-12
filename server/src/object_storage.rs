//! Small S3-compatible object storage client used for Bluey Cloud artifact bytes.
//!
//! Cloudflare R2 speaks the S3 SigV4 API. Keeping the client narrow avoids a
//! large provider SDK dependency in the server binary.

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
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
        let bytes = response.bytes().await.context("read object body")?;
        Ok(StoredObject {
            bytes,
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

    fn object_url(&self, key: &str) -> Result<Url> {
        let endpoint = self.config.endpoint_url.trim_end_matches('/');
        let encoded_path = [encode_segment(&self.config.bucket), encode_key_path(key)].join("/");
        Url::parse(&format!("{endpoint}/{encoded_path}")).context("parse object URL")
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
        let canonical_request = format!(
            "{}\n{}\n\n{}\n{}\n{}",
            method.as_str(),
            canonical_uri,
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
    use wiremock::matchers::{method, path};
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
        let hash = sha256_hex("worker payload");
        let key = storage.artifact_upload_key("acct_worker", "artifact_worker", &hash);
        let reservation = reserve_upload(
            &pool,
            &NewObjectUpload {
                account_id: "acct_worker".into(),
                object_kind: ObjectKind::Artifact,
                logical_id: "artifact_worker".into(),
                session_id: None,
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
