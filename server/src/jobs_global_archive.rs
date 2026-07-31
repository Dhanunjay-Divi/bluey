//! Verified cold-storage lifecycle for expired global job candidates.
//!
//! PostgreSQL remains the live search and relationship index. This worker
//! archives only expired, unreferenced candidate payloads and replaces the
//! database payload with a small tombstone only after object-store read-back
//! matches the exact bytes and SHA-256 written.

use crate::{
    config::ObjectStorageConfig,
    db::{jobs, DbPool},
    object_storage::ObjectStorage,
};
use anyhow::{Context, Result};
use bytes::Bytes;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::task::JoinHandle;

const DEFAULT_RETENTION_DAYS: i64 = 30;
const DEFAULT_POLL_SECONDS: u64 = 60 * 60;
const DEFAULT_BATCH_SIZE: usize = 25;
const DEFAULT_LEASE_SECONDS: i64 = 5 * 60;
const MAX_BATCHES_PER_PASS: usize = 4;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GlobalCandidateArchiveEnvelope<'a> {
    schema_version: u8,
    candidate_id: &'a str,
    canonical_key: &'a str,
    content_hash: &'a str,
    encrypted_candidate_json: &'a str,
}

#[derive(Debug, Clone, Copy)]
struct ArchiveWorkerConfig {
    retention_ms: i64,
    poll_seconds: u64,
    batch_size: usize,
    lease_ms: i64,
}

pub fn spawn_global_candidate_archive_worker(
    pool: DbPool,
    storage_config: Option<ObjectStorageConfig>,
) -> Result<Option<JoinHandle<()>>> {
    if !env_enabled("BLUEY_JOBS_GLOBAL_ARCHIVE_ENABLED", false) {
        tracing::info!("Jobs global candidate archive worker disabled");
        return Ok(None);
    }
    let storage_config = storage_config
        .context("BLUEY_JOBS_GLOBAL_ARCHIVE_ENABLED requires configured object storage")?;
    let config = archive_worker_config();
    let storage = ObjectStorage::new(storage_config);
    let owner = format!("global-candidate-archive-{}", uuid::Uuid::new_v4());
    Ok(Some(tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(config.poll_seconds));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if let Err(error) =
                run_global_candidate_archive_pass(&pool, &storage, &owner, config).await
            {
                tracing::warn!(error = %error, "Jobs global candidate archive pass failed");
            }
        }
    })))
}

async fn run_global_candidate_archive_pass(
    pool: &DbPool,
    storage: &ObjectStorage,
    owner: &str,
    config: ArchiveWorkerConfig,
) -> Result<usize> {
    let mut archived_count = 0;
    for _ in 0..MAX_BATCHES_PER_PASS {
        let now = jobs::now_ms();
        let stale_before_ms = now.saturating_sub(config.retention_ms);
        let leases = jobs::claim_global_candidate_archive_jobs(
            pool,
            owner,
            now,
            stale_before_ms,
            config.lease_ms,
            config.batch_size,
        )?;
        if leases.is_empty() {
            break;
        }
        let lease_count = leases.len();
        for lease in leases {
            match archive_candidate(storage, &lease).await {
                Ok(archive) => {
                    let completed = jobs::complete_global_candidate_archive(
                        pool,
                        &lease,
                        &archive.storage_key,
                        &archive.sha256,
                        archive.size_bytes,
                        jobs::now_ms(),
                    )?;
                    if completed {
                        archived_count += 1;
                    } else {
                        tracing::info!(
                            candidate_id = %lease.candidate_id,
                            "global candidate changed or became referenced before archival completed"
                        );
                    }
                }
                Err(error) => {
                    let retry_recorded =
                        jobs::fail_global_candidate_archive(pool, &lease, jobs::now_ms())?;
                    tracing::warn!(
                        candidate_id = %lease.candidate_id,
                        retry_recorded,
                        error = %error,
                        "global candidate archival failed"
                    );
                }
            }
        }
        if lease_count < config.batch_size {
            break;
        }
    }
    if archived_count > 0 {
        tracing::info!(
            archived_count,
            "expired global job candidate payloads archived"
        );
    }
    Ok(archived_count)
}

struct VerifiedArchive {
    storage_key: String,
    sha256: String,
    size_bytes: i64,
}

async fn archive_candidate(
    storage: &ObjectStorage,
    lease: &jobs::GlobalCandidateArchiveLease,
) -> Result<VerifiedArchive> {
    let envelope = GlobalCandidateArchiveEnvelope {
        schema_version: 1,
        candidate_id: &lease.candidate_id,
        canonical_key: &lease.canonical_key,
        content_hash: &lease.content_hash,
        encrypted_candidate_json: &lease.candidate_json,
    };
    let bytes = serde_json::to_vec(&envelope).context("serialize global candidate archive")?;
    if bytes.is_empty() || bytes.len() > storage.max_object_bytes() {
        anyhow::bail!("global candidate archive exceeds configured object size")
    }
    let sha256 = sha256_hex(&bytes);
    let storage_key =
        storage.global_candidate_archive_key(&lease.candidate_id, &lease.content_hash);
    storage
        .put(&storage_key, Bytes::from(bytes.clone()), "application/json")
        .await
        .context("write global candidate archive")?;
    let stored = storage
        .get(&storage_key)
        .await
        .context("read back global candidate archive")?;
    let readback_sha256 = sha256_hex(&stored.bytes);
    if stored.bytes.as_ref() != bytes.as_slice() || readback_sha256 != sha256 {
        anyhow::bail!("global candidate archive read-back verification failed")
    }
    Ok(VerifiedArchive {
        storage_key,
        sha256,
        size_bytes: i64::try_from(bytes.len()).context("archive size exceeds database range")?,
    })
}

fn archive_worker_config() -> ArchiveWorkerConfig {
    let retention_days = env_i64(
        "BLUEY_JOBS_GLOBAL_ARCHIVE_RETENTION_DAYS",
        DEFAULT_RETENTION_DAYS,
    )
    .clamp(7, 3_650);
    let poll_seconds = env_u64(
        "BLUEY_JOBS_GLOBAL_ARCHIVE_POLL_SECONDS",
        DEFAULT_POLL_SECONDS,
    )
    .clamp(60, 24 * 60 * 60);
    let batch_size =
        env_usize("BLUEY_JOBS_GLOBAL_ARCHIVE_BATCH_SIZE", DEFAULT_BATCH_SIZE).clamp(1, 250);
    let lease_seconds = env_i64(
        "BLUEY_JOBS_GLOBAL_ARCHIVE_LEASE_SECONDS",
        DEFAULT_LEASE_SECONDS,
    )
    .clamp(60, 60 * 60);
    ArchiveWorkerConfig {
        retention_ms: retention_days.saturating_mul(24 * 60 * 60 * 1_000),
        poll_seconds,
        batch_size,
        lease_ms: lease_seconds.saturating_mul(1_000),
    }
}

fn env_enabled(name: &str, default: bool) -> bool {
    env_value_enabled(std::env::var(name).ok().as_deref(), default)
}

fn env_value_enabled(value: Option<&str>, default: bool) -> bool {
    value
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn env_i64(name: &str, default: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(default)
}

fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::{
        env_value_enabled, run_global_candidate_archive_pass, sha256_hex, ArchiveWorkerConfig,
        GlobalCandidateArchiveEnvelope,
    };
    use crate::{
        config::ObjectStorageConfig,
        db::{self, jobs::DiscoveredJobInput, DbPool},
        object_storage::ObjectStorage,
    };
    use rusqlite::params;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn archive_test_pool(candidate_id: &str, content_hash: &str) -> (DbPool, DiscoveredJobInput) {
        let path = std::env::temp_dir().join(format!(
            "bluey-global-archive-test-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        let input = DiscoveredJobInput {
            external_id: format!("external-{candidate_id}"),
            canonical_url: format!("https://jobs.lever.co/acme/{candidate_id}"),
            company: "Acme".to_string(),
            source_catalog_id: "jobhive:lever".to_string(),
            requires_original_revalidation: true,
            title: "Software Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            description: "Build reliable products with Rust and TypeScript.".to_string(),
            compensation: "$170k-$200k".to_string(),
            employment_type: "full_time".to_string(),
            engagement_type: "direct_hire".to_string(),
            posted_at_ms: Some(1),
        };
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_global_candidates (
                    id, canonical_key, candidate_json, company, title, location, workplace,
                    canonical_url, role_family, posted_at_ms, availability_status,
                    first_seen_at_ms, last_seen_at_ms, updated_at_ms, content_hash
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'software_engineering',
                           1, 'expired', 1, 1, 1, ?9)",
                params![
                    candidate_id,
                    format!("canonical-{candidate_id}"),
                    serde_json::to_string(&input).unwrap(),
                    input.company,
                    input.title,
                    input.location,
                    input.workplace,
                    input.canonical_url,
                    content_hash,
                ],
            )
            .unwrap();
        (pool, input)
    }

    fn archive_test_storage(server: &MockServer) -> ObjectStorage {
        ObjectStorage::new(ObjectStorageConfig {
            endpoint_url: server.uri(),
            bucket: "bluey-test".to_string(),
            access_key_id: "test-access".to_string(),
            secret_access_key: "test-secret".to_string(),
            region: "auto".to_string(),
            key_prefix: "bluey-cloud".to_string(),
            retention_days: 30,
            max_object_bytes: 1024 * 1024,
        })
    }

    fn archive_test_config() -> ArchiveWorkerConfig {
        ArchiveWorkerConfig {
            retention_ms: 0,
            poll_seconds: 60,
            batch_size: 25,
            lease_ms: 60_000,
        }
    }

    #[test]
    fn archive_worker_requires_explicit_enablement() {
        assert!(!env_value_enabled(None, false));
        assert!(!env_value_enabled(Some("false"), false));
        assert!(!env_value_enabled(Some("0"), false));
        assert!(env_value_enabled(Some("true"), false));
        assert!(env_value_enabled(Some(" yes "), false));
    }

    #[test]
    fn archive_hash_is_stable_for_identical_bytes() {
        assert_eq!(sha256_hex(b"archive"), sha256_hex(b"archive"));
        assert_ne!(sha256_hex(b"archive"), sha256_hex(b"other"));
    }

    #[tokio::test]
    async fn verified_r2_readback_tombstones_the_database_payload() {
        let candidate_id = "candidate-verified";
        let content_hash = "a".repeat(64);
        let (pool, input) = archive_test_pool(candidate_id, &content_hash);
        let original_payload: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT candidate_json FROM jobs_global_candidates WHERE id = ?1",
                params![candidate_id],
                |row| row.get(0),
            )
            .unwrap();
        let canonical_key = format!("canonical-{candidate_id}");
        let expected = serde_json::to_vec(&GlobalCandidateArchiveEnvelope {
            schema_version: 1,
            candidate_id,
            canonical_key: &canonical_key,
            content_hash: &content_hash,
            encrypted_candidate_json: &original_payload,
        })
        .unwrap();
        let object_path = format!(
            "/bluey-test/bluey-cloud/global/jobs/candidates/{candidate_id}/sha256/{content_hash}.json"
        );
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path(object_path.clone()))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(object_path))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_bytes(expected.clone()),
            )
            .expect(1)
            .mount(&server)
            .await;

        let archived = run_global_candidate_archive_pass(
            &pool,
            &archive_test_storage(&server),
            "archive-worker",
            archive_test_config(),
        )
        .await
        .unwrap();

        assert_eq!(archived, 1);
        let archived_row: (String, String, i64, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT archive_state, archive_sha256, archive_size_bytes, candidate_json
                   FROM jobs_global_candidates
                  WHERE id = ?1",
                params![candidate_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(archived_row.0, "archived");
        assert_eq!(archived_row.1, sha256_hex(&expected));
        assert_eq!(archived_row.2, i64::try_from(expected.len()).unwrap());
        let tombstone: DiscoveredJobInput = serde_json::from_str(
            &crate::db::jobs::decrypt_payload_for_test(&archived_row.3).unwrap(),
        )
        .unwrap();
        assert_eq!(tombstone.title, input.title);
        assert_eq!(tombstone.company, input.company);
        assert!(tombstone.description.is_empty());
        assert!(tombstone.compensation.is_empty());
    }

    #[tokio::test]
    async fn mismatched_r2_readback_keeps_the_full_payload_for_retry() {
        let candidate_id = "candidate-readback-mismatch";
        let content_hash = "b".repeat(64);
        let (pool, input) = archive_test_pool(candidate_id, &content_hash);
        let original_payload: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT candidate_json FROM jobs_global_candidates WHERE id = ?1",
                params![candidate_id],
                |row| row.get(0),
            )
            .unwrap();
        let object_path = format!(
            "/bluey-test/bluey-cloud/global/jobs/candidates/{candidate_id}/sha256/{content_hash}.json"
        );
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path(object_path.clone()))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(object_path))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_bytes(b"different bytes"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let archived = run_global_candidate_archive_pass(
            &pool,
            &archive_test_storage(&server),
            "archive-worker",
            archive_test_config(),
        )
        .await
        .unwrap();

        assert_eq!(archived, 0);
        let retry_row: (String, i64, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT archive_state, archive_attempt_count, candidate_json
                   FROM jobs_global_candidates
                  WHERE id = ?1",
                params![candidate_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(retry_row.0, "retry");
        assert_eq!(retry_row.1, 1);
        assert_eq!(retry_row.2, original_payload);
        let retained: DiscoveredJobInput = serde_json::from_str(&retry_row.2).unwrap();
        assert_eq!(retained.description, input.description);
        assert_eq!(retained.compensation, input.compensation);
    }
}
