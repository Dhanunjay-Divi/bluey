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

type HmacSha256 = Hmac<Sha256>;

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

impl ObjectStorage {
    pub fn new(config: ObjectStorageConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    pub fn artifact_key(&self, account_id: &str, artifact_id: &str) -> String {
        let prefix = self.config.key_prefix.trim_matches('/');
        if prefix.is_empty() {
            format!("accounts/{account_id}/context/{artifact_id}")
        } else {
            format!("{prefix}/accounts/{account_id}/context/{artifact_id}")
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
