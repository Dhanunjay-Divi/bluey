//! Owned PostgreSQL synthetic component journey from managed Start to public receipt.
//!
//! This fixture deliberately uses the public control-plane authority and normal
//! application/staging APIs. It does not manufacture command, lease, receipt,
//! or request-start rows.

use std::{
    collections::BTreeSet,
    ffi::OsString,
    panic::AssertUnwindSafe,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use anyhow::{Context, Result};
use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
    middleware::{self, Next},
};
use base64::Engine;
use ed25519_dalek::SigningKey as Ed25519SigningKey;
use futures_util::FutureExt;
use hmac::{Hmac, Mac};
use serde_json::json;
use serial_test::serial;
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;
use tower::ServiceExt;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[derive(Default)]
struct ReceiptObserver {
    statuses: Mutex<Vec<u16>>,
    puts: AtomicU8,
    accepted_puts: AtomicU8,
    gets: AtomicU8,
    accepted_gets: AtomicU8,
}

impl ReceiptObserver {
    fn observe_status(&self, status: u16) {
        if let Ok(mut statuses) = self.statuses.lock() {
            // At most four responses are expected. Retain the fifth as
            // overflow evidence so more requests cannot look like success.
            if statuses.len() < 5 {
                statuses.push(status);
            }
        }
    }

    fn statuses_match(&self, expected: &[u16]) -> bool {
        self.statuses
            .lock()
            .is_ok_and(|statuses| statuses.as_slice() == expected)
    }

    fn observe(counter: &AtomicU8) {
        let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_add(1))
        });
    }

    fn diagnosis(&self) -> String {
        format!(
            "owned receipt statuses={:?} object puts={}/{} gets={}/{} (accepted/observed)",
            self.statuses.lock().map(|statuses| statuses.clone()).ok(),
            self.accepted_puts.load(Ordering::Relaxed),
            self.puts.load(Ordering::Relaxed),
            self.accepted_gets.load(Ordering::Relaxed),
            self.gets.load(Ordering::Relaxed),
        )
    }
}

use super::*;
use crate::api::jobs_runner_volumes::{
    runner_volume_execution_lease_claim_payload_sha256, RunnerVolumeExecutionLeaseClaimPayload,
    RunnerVolumeManagedExecutionLeaseClaimPayload,
};
use crate::db::jobs::integration_test_runner_control_plane::{
    RunnerControlPlaneBootstrapRequest, RunnerControlPlaneInspectAutoSubmitClaimRequest,
    RunnerControlPlanePostQueueRequest,
};
use crate::db::{
    self,
    jobs::{
        self, BrowserSession, CareerTrack, CareerTrackPolicy, JobPreferences,
        JobsWorkflowCommandState, ManagedCloudGatewayAuthority, ManagedCloudScope,
        ResumeSourceAsset, StageCloudWorkflowStart,
    },
    jobs_beta_access::{
        self, PublicBetaAccessReason, PublicBetaCohortState, PublicBetaCohortUpdate,
    },
};

type HmacSha256 = Hmac<Sha256>;

const OWNED_DELIVERY_UNKNOWN_CASE: &str = "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_managed_start_dispatch_marks_delivery_unknown_when_configured";
const OWNED_ACCEPTED_CASE: &str = "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_managed_start_accepted_receipt_replay_is_read_only";
const OWNED_TERMINAL_RETRY_CASE: &str = "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_managed_receipt_replay_recovers_terminal_update_failure";
const OWNED_WARMUP_CASE: &str = "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_managed_receipt_grounds_authenticated_interview_prep";
const OWNED_WARMUP_BROWSER_CASE: &str = "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_managed_receipt_browser_downloads_interview_prep";
use crate::db::jobs::integration_test_runner_control_plane::VERIFIED_SIGNUP_CANDIDATE_EMAIL as NEW_CANDIDATE_EMAIL;
use crate::db::owned_postgres_test::VERIFIED_SIGNUP_MANAGED_RECEIPT_CASE as OWNED_VERIFIED_SIGNUP_CASE;
use crate::db::owned_postgres_test::VERIFIED_SIGNUP_REVIEW_FIRST_APPROVAL_CASE as OWNED_REVIEW_FIRST_CASE;
use crate::db::owned_postgres_test::VERIFIED_SIGNUP_REVIEW_FIRST_MOCK_RECEIPT_CASE as OWNED_REVIEW_FIRST_MOCK_RECEIPT_CASE;
const JOURNEY_JWT_SECRET: &str = "managed-claim-test-secret-at-least-32-bytes";
const REVIEW_FIRST_MOCK_WORKER_TOKEN: &str = "synthetic-review-first-receipt-only";

#[path = "jobs_owned_signup_fixture.rs"]
mod owned_signup_fixture;
use crate::jobs_review_first_mock_receipt_fixture as review_first_mock_receipt_fixture;
#[path = "jobs_review_first_signup_fixture.rs"]
mod review_first_signup_fixture;

const PREP_BROWSER_DRIVER_SHA256: &str =
    "54df4ad104e0fcdc9fc61f443065ac98ba091ceafa25c975e6703d3966dd2e6c";
const PREP_BROWSER_GATEWAY_SHA256: &str =
    "f8063e3bffd659ee7f887983eb294c5ee9d77eff555f442394ebfd2c108930ae";
const PREP_BROWSER_WITNESS_SHA256: &str =
    "be685b19ce54e4cacf7f542f246887c758bd96659042f340cd0a45402441fb2b";
const WARMUP_CONTENT: &str = "Explain why a missing feed should block an official report.";
const ACCEPTED_TEMPORAL_RUN_ID: &str = "phase660-accepted-temporal-run-0001";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReceiptFollowup {
    Replay,
    RecoverTerminalUpdate,
    InterviewPrep,
    InterviewPrepBrowser,
    VerifiedSignup,
}

impl ReceiptFollowup {
    fn needs_interview_prep(self) -> bool {
        matches!(self, Self::InterviewPrep | Self::InterviewPrepBrowser)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManagedReceiptJourneyCase {
    DeliveryUnknown,
    Accepted,
}

impl ManagedReceiptJourneyCase {
    fn owned_case(self) -> &'static str {
        match self {
            Self::DeliveryUnknown => OWNED_DELIVERY_UNKNOWN_CASE,
            Self::Accepted => OWNED_ACCEPTED_CASE,
        }
    }

    fn expected_state(self) -> JobsWorkflowCommandState {
        match self {
            Self::DeliveryUnknown => JobsWorkflowCommandState::DeliveryUnknown,
            Self::Accepted => JobsWorkflowCommandState::Accepted,
        }
    }

    fn workflow_id(self, run_id: &str) -> String {
        match self {
            Self::DeliveryUnknown => {
                format!("managed-request-start-workflow-{}", uuid::Uuid::new_v4())
            }
            Self::Accepted => format!("bluey-jobs-v2-{run_id}"),
        }
    }
}
const GATEWAY_TOKEN: &str = "managed-request-start-test-token-1234567890";
const MAX_STORED_GATEWAY_AUTHORITY_BASE64URL_BYTES: usize = 174_763;

// Private component bridge: no browser/native-volume custody is implied. The
// separately reviewed qualifier must build the exact runner/automation/workflows capsule.
const JOURNEY_DRIVER_SHA256: &str =
    "5fd3f1f669303f7dff81606f89a5a1c4ee54a4406cb2dec68da966acd5ea8bfb";
const JOURNEY_EVENT: &str = "phase658_managed_receipt_journey_node_event_v1";
const JOURNEY_RESULT: &str = "phase658_managed_receipt_journey_node_result_v1";

fn journey_sha(value: &serde_json::Value) -> Result<String> {
    Ok(hex::encode(Sha256::digest(
        managed_authority_test_canonical_bytes(value)?,
    )))
}

fn journey_keys(value: &serde_json::Value, fields: &[&str]) -> Result<()> {
    let object = value.as_object().context("private journey frame object")?;
    anyhow::ensure!(
        object.keys().map(String::as_str).collect::<BTreeSet<_>>()
            == fields.iter().copied().collect::<BTreeSet<_>>(),
        "private journey exact frame fields"
    );
    Ok(())
}

fn journey_digest(value: &serde_json::Value, field: &str) -> Result<String> {
    let digest = value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .context("private journey digest field")?;
    anyhow::ensure!(
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "private journey canonical digest"
    );
    Ok(digest.to_string())
}

fn journey_png() -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&[0, 0, 0])?;
    }
    Ok(bytes)
}

#[cfg(unix)]
fn journey_stable_file(path: &std::path::Path, maximum: u64, owner: u32) -> Result<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;
    let named_before = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(
        named_before.is_file() && !named_before.file_type().is_symlink(),
        "private journey source/tool regular path"
    );
    let mut file = std::fs::File::open(path)?;
    let before = file.metadata()?;
    anyhow::ensure!(
        before.is_file()
            && before.uid() == owner
            && before.nlink() == 1
            && before.mode() & 0o022 == 0
            && before.len() > 0
            && before.len() <= maximum,
        "private journey source/tool identity"
    );
    let mut bytes = Vec::new();
    file.by_ref().take(maximum + 1).read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    let named = std::fs::symlink_metadata(path)?;
    let identity = |m: &std::fs::Metadata| {
        (
            m.dev(),
            m.ino(),
            m.uid(),
            m.mode(),
            m.nlink(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    };
    anyhow::ensure!(
        identity(&named_before) == identity(&before)
            && identity(&before) == identity(&after)
            && identity(&after) == identity(&named)
            && bytes.len() as u64 == before.len(),
        "private journey source/tool content changed"
    );
    Ok(bytes)
}

fn journey_pinned_file(path: &std::path::Path, digest: &str, owner: u32) -> Result<()> {
    anyhow::ensure!(
        hex::encode(Sha256::digest(journey_stable_file(
            path,
            128 * 1024 * 1024,
            owner
        )?)) == digest,
        "private journey measured source/tool hash"
    );
    Ok(())
}

#[cfg(not(unix))]
fn journey_stable_file(_path: &std::path::Path, _maximum: u64, _owner: u32) -> Result<Vec<u8>> {
    anyhow::bail!("private journey requires the Unix owned launcher")
}

fn journey_directory(path: &std::path::Path, owner: Option<u32>, private: bool) -> Result<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(path)?;
        anyhow::ensure!(
            metadata.is_dir()
                && path.canonicalize()? == path
                && owner.is_none_or(|uid| metadata.uid() == uid)
                && if private {
                    metadata.mode() & 0o777 == 0o700
                } else {
                    metadata.mode() & 0o022 == 0
                },
            "private journey directory custody"
        );
        Ok(metadata.uid())
    }
    #[cfg(not(unix))]
    {
        let _ = (path, owner, private);
        anyhow::bail!("private journey requires Unix")
    }
}

struct JourneyNode {
    child: tokio::process::Child,
    input: Option<tokio::process::ChildStdin>,
    events: tokio::io::BufReader<tokio::process::ChildStderr>,
    output: Option<JoinHandle<Result<Vec<u8>>>>,
    early_terminal: tokio::sync::oneshot::Receiver<JourneyEarlyTerminal>,
    nonce: String,
    event_bytes: usize,
    driver: std::path::PathBuf,
    tool: std::path::PathBuf,
    tool_sha256: String,
    descriptor: std::path::PathBuf,
    descriptor_sha256: String,
    owner: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct JourneyEarlyTerminal {
    outcome: &'static str,
    code: &'static str,
    phase: &'static str,
}

fn journey_early_terminal(bytes: &[u8]) -> JourneyEarlyTerminal {
    let fallback = JourneyEarlyTerminal {
        outcome: "unrecognized",
        code: "unrecognized",
        phase: "unrecognized",
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return fallback;
    };
    if managed_authority_test_canonical_bytes(&value)
        .ok()
        .as_deref()
        != Some(bytes)
    {
        return fallback;
    }
    let Some(object) = value.as_object() else {
        return fallback;
    };
    let keys = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if value.get("schema") != Some(&serde_json::Value::String(JOURNEY_RESULT.to_string())) {
        return fallback;
    }
    if value.get("ok") == Some(&serde_json::Value::Bool(true)) {
        return if keys
            == [
                "schema",
                "ok",
                "nonce",
                "applicationId",
                "runId",
                "resultSha256",
                "receiptStatus",
                "replayStatus",
                "fingerprintStatus",
            ]
            .into_iter()
            .collect()
        {
            JourneyEarlyTerminal {
                outcome: "success",
                code: "none",
                phase: "none",
            }
        } else {
            fallback
        };
    }
    if value.get("ok") != Some(&serde_json::Value::Bool(false))
        || keys != ["schema", "ok", "code", "phase"].into_iter().collect()
    {
        return fallback;
    }
    let code = match value.get("code").and_then(serde_json::Value::as_str) {
        Some("driver_failed") => "driver_failed",
        Some("invalid_schema" | "invalid_shape" | "invalid_frame" | "unexpected_eof") => "input",
        Some("claim_binding_mismatch") => "claim_binding",
        Some("receipt_timing_failed") => "receipt_timing_failed",
        Some("receipt_create_failed") => "receipt_create_failed",
        Some("receipt_checkpoint_metadata_failed") => "receipt_checkpoint_metadata_failed",
        Some("result_storage_setup_failed") => "result_storage_setup_failed",
        Some("result_stage_write_failed") => "result_stage_write_failed",
        Some("result_stage_readback_failed") => "result_stage_readback_failed",
        Some("result_stage_mismatch") => "result_stage_mismatch",
        Some("finish_submitted_failed") => "finish_submitted_failed",
        Some("finish_acknowledgement_invalid") => "finish_acknowledgement_invalid",
        Some("receipt_post_400") => "receipt_post_400",
        Some("receipt_post_401") => "receipt_post_401",
        Some("receipt_post_403") => "receipt_post_403",
        Some("receipt_post_404") => "receipt_post_404",
        Some("receipt_post_409") => "receipt_post_409",
        Some("receipt_post_413") => "receipt_post_413",
        Some("receipt_post_422") => "receipt_post_422",
        Some("receipt_post_429") => "receipt_post_429",
        Some("receipt_post_5xx") => "receipt_post_5xx",
        Some("receipt_post_other") => "receipt_post_other",
        Some("receipt_post_200_wrong_id") => "receipt_post_200_wrong_id",
        Some("receipt_post_200_wrong_run") => "receipt_post_200_wrong_run",
        Some("receipt_post_200_wrong_state") => "receipt_post_200_wrong_state",
        Some("receipt_response_headers_invalid") => "receipt_response_headers_invalid",
        Some("receipt_terminalization_not_deferred") => "receipt_terminalization_not_deferred",
        Some("receipt_request_replay_mismatch") => "receipt_request_replay_mismatch",
        Some(_) => "unrecognized",
        None => return fallback,
    };
    let phase = match value.get("phase").and_then(serde_json::Value::as_str) {
        Some("input") => "input",
        Some("claim") => "claim",
        Some("effect_checksum_rejected") => "effect_checksum_rejected",
        Some("effect_authorized") => "effect_authorized",
        Some("irreversible_checksum_rejected") => "irreversible_checksum_rejected",
        Some("irreversible_fence_rejected") => "irreversible_fence_rejected",
        Some("irreversible_authorized") => "irreversible_authorized",
        Some("finish_before_commit") => "finish_before_commit",
        Some("result_committed") => "result_committed",
        Some("receipt_terminalization_deferred") => "receipt_terminalization_deferred",
        Some("receipt_posted") => "receipt_posted",
        Some("receipt_replayed") => "receipt_replayed",
        Some("fingerprint_rejected") => "fingerprint_rejected",
        Some(_) => "unrecognized",
        None => return fallback,
    };
    if matches!(
        code,
        "receipt_timing_failed"
            | "receipt_create_failed"
            | "receipt_checkpoint_metadata_failed"
            | "result_storage_setup_failed"
            | "result_stage_write_failed"
            | "result_stage_readback_failed"
            | "result_stage_mismatch"
            | "finish_submitted_failed"
            | "finish_acknowledgement_invalid"
    ) && phase != "irreversible_authorized"
    {
        return fallback;
    }
    if matches!(
        code,
        "receipt_post_400"
            | "receipt_post_401"
            | "receipt_post_403"
            | "receipt_post_404"
            | "receipt_post_409"
            | "receipt_post_413"
            | "receipt_post_422"
            | "receipt_post_429"
            | "receipt_post_5xx"
            | "receipt_post_other"
            | "receipt_post_200_wrong_id"
            | "receipt_post_200_wrong_run"
            | "receipt_post_200_wrong_state"
            | "receipt_response_headers_invalid"
            | "receipt_terminalization_not_deferred"
    ) && phase != "receipt_posted"
    {
        return fallback;
    }
    if code == "receipt_request_replay_mismatch"
        && !matches!(
            phase,
            "receipt_posted" | "receipt_replayed" | "fingerprint_rejected"
        )
    {
        return fallback;
    }
    JourneyEarlyTerminal {
        outcome: "driver_error",
        code,
        phase,
    }
}

async fn journey_read_terminal<R>(
    stdout: R,
    early_terminal: tokio::sync::oneshot::Sender<JourneyEarlyTerminal>,
) -> Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let mut stdout = tokio::io::BufReader::new(stdout);
    let mut bytes = Vec::new();
    loop {
        let byte = stdout.read_u8().await?;
        bytes.push(byte);
        anyhow::ensure!(
            bytes.len() <= 16 * 1024,
            "private journey final output bound"
        );
        if byte == b'\n' {
            break;
        }
    }
    let _ = early_terminal.send(journey_early_terminal(&bytes));
    let remaining = (16 * 1024 + 1 - bytes.len()) as u64;
    stdout.take(remaining).read_to_end(&mut bytes).await?;
    anyhow::ensure!(
        bytes.len() <= 16 * 1024,
        "private journey final output bound"
    );
    Ok(bytes)
}

enum JourneyPhaseWait {
    Event,
    EarlyTerminal(JourneyEarlyTerminal),
}

impl JourneyNode {
    async fn spawn(initial: &serde_json::Value, nonce: String) -> Result<Self> {
        use tokio::io::AsyncWriteExt;
        let root = std::path::PathBuf::from(
            std::env::var_os("BLUEY_TEST_RUN_ROOT")
                .context("private journey requires the canonical owned run root")?,
        );
        anyhow::ensure!(
            root.is_absolute() && root.canonicalize()? == root,
            "private journey canonical run root"
        );
        let owner = journey_directory(&root, None, true)?;
        let driver =
            root.join("receipt-journey-node/jobs/runner/tests/managed-receipt-journey-driver.mjs");
        for parent in driver
            .parent()
            .context("journey driver parent")?
            .ancestors()
        {
            if parent == root {
                break;
            }
            journey_directory(parent, Some(owner), false)
                .context("receipt journey driver capsule must be staged in the owned run root")?;
        }
        let capsule = root.join("receipt-journey-node");
        let tool = capsule.join("toolchain/bin/node");
        let descriptor = capsule.join("node-tool.json");
        journey_directory(&capsule.join("toolchain"), Some(owner), false)
            .context("receipt journey Node toolchain directory unavailable")?;
        journey_directory(&capsule.join("toolchain/bin"), Some(owner), false)
            .context("receipt journey Node toolchain bin directory unavailable")?;
        let descriptor_bytes = journey_stable_file(&descriptor, 4096, owner)
            .context("receipt journey Node tool descriptor unavailable")?;
        let descriptor_sha256 = hex::encode(Sha256::digest(&descriptor_bytes));
        let descriptor_value: serde_json::Value = serde_json::from_slice(&descriptor_bytes)?;
        journey_keys(
            &descriptor_value,
            &[
                "schema",
                "platform",
                "arch",
                "version",
                "relativePath",
                "sha256",
            ],
        )?;
        let platform = match std::env::consts::OS {
            "macos" => "darwin",
            "linux" => "linux",
            _ => anyhow::bail!("private journey unsupported Node platform"),
        };
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86_64" => "x64",
            _ => anyhow::bail!("private journey unsupported Node architecture"),
        };
        anyhow::ensure!(
            descriptor_value["schema"] == "phase658_receipt_journey_node_tool_v1"
                && descriptor_value["platform"] == platform
                && descriptor_value["arch"] == arch
                && descriptor_value["version"] == "v22.23.4"
                && descriptor_value["relativePath"] == "toolchain/bin/node",
            "private journey closed platform/tool descriptor"
        );
        let tool_sha256 = journey_digest(&descriptor_value, "sha256")?;
        journey_pinned_file(&tool, &tool_sha256, owner)?;
        journey_pinned_file(&driver, JOURNEY_DRIVER_SHA256, owner)?;
        // Descriptor/hash continuity is not publisher authentication. The separately
        // reviewed builder must bind these exact bytes and the entire tool closure.
        let mut bytes = serde_json::to_vec(initial)?;
        anyhow::ensure!(
            !bytes.is_empty() && bytes.len() <= 512 * 1024,
            "private journey initial frame bound"
        );
        bytes.push(b'\n');
        let mut child = tokio::process::Command::new(&tool)
            .arg(&driver)
            .current_dir(&root)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", root.join("home"))
            .env("TMPDIR", root.join("tmp"))
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("spawn private journey driver")?;
        let input = child.stdin.take().context("journey stdin")?;
        let stdout = child.stdout.take().context("journey stdout")?;
        let events = tokio::io::BufReader::new(child.stderr.take().context("journey stderr")?);
        let (early_terminal_sender, early_terminal) = tokio::sync::oneshot::channel();
        let output = tokio::spawn(journey_read_terminal(stdout, early_terminal_sender));
        let mut node = Self {
            child,
            input: Some(input),
            events,
            output: Some(output),
            early_terminal,
            nonce,
            event_bytes: 0,
            driver,
            tool,
            tool_sha256,
            descriptor,
            descriptor_sha256,
            owner,
        };
        // Ownership is still local until this frame is sent. Bound BOTH writes
        // before returning the child to the outer cleanup slot.
        let sent = tokio::time::timeout(Duration::from_secs(20), async {
            let input = node
                .input
                .as_mut()
                .ok_or_else(|| std::io::Error::other("journey stdin unexpectedly closed"))?;
            input.write_all(&bytes).await?;
            input.flush().await?;
            Ok::<(), std::io::Error>(())
        })
        .await;
        bytes.fill(0);
        match sent {
            Ok(Ok(())) => Ok(node),
            Ok(Err(error)) => {
                node.stop()
                    .await
                    .context("private journey initial write stop/join")?;
                Err(error.into())
            }
            Err(_) => {
                node.stop()
                    .await
                    .context("private journey initial timeout stop/join")?;
                anyhow::bail!("private journey initial frame deadline")
            }
        }
    }

    async fn phase(&mut self, phase: &'static str, fields: &[&str]) -> Result<serde_json::Value> {
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        let waited = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                tokio::select! {
                    biased;
                    terminal = &mut self.early_terminal => {
                        let terminal = terminal.unwrap_or(JourneyEarlyTerminal {
                            outcome: "unrecognized", code: "unrecognized", phase: "unrecognized",
                        });
                        return Ok::<JourneyPhaseWait, anyhow::Error>(JourneyPhaseWait::EarlyTerminal(terminal));
                    }
                    byte = self.events.read_u8() => {
                        let byte = byte?;
                        bytes.push(byte);
                        anyhow::ensure!(bytes.len() <= 16 * 1024, "private journey phase overflow");
                        if byte == b'\n' {
                            return Ok(JourneyPhaseWait::Event);
                        }
                    }
                }
            }
        })
        .await
        .with_context(|| format!("private journey phase deadline awaiting {phase}"))??;
        if let JourneyPhaseWait::EarlyTerminal(terminal) = waited {
            anyhow::bail!(
                "private journey early terminal awaiting {phase}: {}:{}:{}",
                terminal.outcome,
                terminal.code,
                terminal.phase,
            );
        }
        self.event_bytes += bytes.len();
        anyhow::ensure!(
            !bytes.is_empty()
                && bytes.len() <= 16 * 1024
                && self.event_bytes <= 64 * 1024
                && bytes.last() == Some(&b'\n')
                && !bytes.contains(&b'\r'),
            "private journey bounded machine-only phase"
        );
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            managed_authority_test_canonical_bytes(&value)? == bytes,
            "private journey canonical phase bytes/no duplicate keys"
        );
        let mut expected = vec!["schema", "kind", "nonce", "phase"];
        expected.extend_from_slice(fields);
        journey_keys(&value, &expected)?;
        anyhow::ensure!(
            value["schema"] == JOURNEY_EVENT
                && value["kind"] == "phase_ready"
                && value["nonce"] == self.nonce
                && value["phase"] == phase,
            "private journey phase identity/order"
        );
        Ok(value)
    }

    async fn proceed(&mut self, phase: &str) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        let mut frame = serde_json::to_vec(&json!({
            "schema": "phase658_managed_receipt_journey_node_continue_v1",
            "kind": "continue", "nonce": self.nonce, "phase": phase,
        }))?;
        frame.push(b'\n');
        anyhow::ensure!(frame.len() <= 1024, "private journey continue bound");
        let input = self.input.as_mut().context("journey stdin open")?;
        input.write_all(&frame).await?;
        input.flush().await?;
        Ok(())
    }

    async fn finish(&mut self) -> Result<serde_json::Value> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut input = self.input.take().context("journey stdin open")?;
        input.shutdown().await?;
        drop(input);
        let status = tokio::time::timeout(Duration::from_secs(20), self.child.wait())
            .await
            .context("private journey terminal deadline")??;
        let output = tokio::time::timeout(
            Duration::from_secs(5),
            self.output.take().context("journey stdout join")?,
        )
        .await
        .context("private journey stdout close deadline")???;
        let mut remaining = Vec::new();
        tokio::time::timeout(
            Duration::from_secs(5),
            (&mut self.events).take(2).read_to_end(&mut remaining),
        )
        .await
        .context("private journey stderr close deadline")??;
        anyhow::ensure!(
            remaining.is_empty()
                && status.success()
                && status.code() == Some(0)
                && output.last() == Some(&b'\n')
                && output.iter().filter(|byte| **byte == b'\n').count() == 1,
            "private journey terminal machine contract"
        );
        let value: serde_json::Value = serde_json::from_slice(&output)?;
        anyhow::ensure!(
            managed_authority_test_canonical_bytes(&value)? == output,
            "private journey canonical final bytes/no duplicate keys"
        );
        journey_keys(
            &value,
            &[
                "schema",
                "ok",
                "nonce",
                "applicationId",
                "runId",
                "resultSha256",
                "receiptStatus",
                "replayStatus",
                "fingerprintStatus",
            ],
        )?;
        anyhow::ensure!(
            value["schema"] == JOURNEY_RESULT
                && value["ok"] == true
                && value["nonce"] == self.nonce,
            "private journey final identity"
        );
        journey_pinned_file(&self.driver, JOURNEY_DRIVER_SHA256, self.owner)?;
        journey_pinned_file(&self.tool, &self.tool_sha256, self.owner)?;
        journey_pinned_file(&self.descriptor, &self.descriptor_sha256, self.owner)?;
        Ok(value)
    }

    async fn stop(&mut self) -> Result<()> {
        drop(self.input.take());
        if self.child.try_wait()?.is_none() {
            self.child.start_kill()?;
            tokio::time::timeout(Duration::from_secs(5), self.child.wait())
                .await
                .context("private journey kill/join deadline")??;
        }
        if let Some(output) = self.output.take() {
            output.abort();
            match output.await {
                Err(error) if error.is_cancelled() => {}
                result => {
                    result??;
                }
            }
        }
        Ok(())
    }
}

#[tokio::test]
async fn journey_terminal_projection_arrives_before_stdout_eof() -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let (mut writer, reader) = tokio::io::duplex(1024);
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(journey_read_terminal(reader, sender));
    let line = b"{\"code\":\"driver_failed\",\"ok\":false,\"phase\":\"claim\",\"schema\":\"phase658_managed_receipt_journey_node_result_v1\"}\n";
    writer.write_all(line).await?;
    let terminal = tokio::time::timeout(Duration::from_secs(1), receiver).await??;
    anyhow::ensure!(
        terminal
            == JourneyEarlyTerminal {
                outcome: "driver_error",
                code: "driver_failed",
                phase: "claim",
            }
            && !task.is_finished(),
        "private journey terminal projection must precede EOF"
    );
    drop(writer);
    let output = task.await??;
    anyhow::ensure!(
        output.as_slice() == line,
        "private journey terminal bytes must survive EOF"
    );
    Ok(())
}

impl Drop for JourneyNode {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        if let Some(output) = self.output.take() {
            output.abort();
        }
    }
}

fn journey_durable_snapshot(pool: &crate::db::DbPool, account_id: &str) -> Result<Vec<String>> {
    db::run_blocking_db(|| {
        let mut connection = pool.get_pg()?;
        let mut tx = connection
            .build_transaction()
            .isolation_level(postgres::IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()?;
        let mut snapshot = Vec::new();
        for table in [
            "jobs_applications",
            "jobs_browser_sessions",
            "jobs_application_evidence",
            "jobs_execution_leases",
            "jobs_application_ats_certification_bindings",
            "jobs_managed_cloud_irreversible_effect_receipts",
            "jobs_submission_evidence_capacity",
            "jobs_attempt_reservations",
            "object_uploads",
            "object_upload_daily_usage",
            "object_storage_outbox",
            "jobs_workflow_commands",
            "jobs_workflow_command_attempts",
            "jobs_workflow_command_attempt_events",
            "jobs_workflow_executions",
        ] {
            // Names are the fixed local list above; no input supplies SQL identifiers.
            for row in tx.query(
                &format!(
                    "SELECT row_to_json(t)::text FROM {table} t WHERE account_id=$1 ORDER BY 1"
                ),
                &[&account_id],
            )? {
                snapshot.push(format!("{table}:{}", row.get::<_, String>(0)));
            }
        }
        tx.commit()?;
        Ok(snapshot)
    })
}

fn journey_workflow_execution_snapshot(
    pool: &crate::db::DbPool,
    account_id: &str,
    workflow_id: &str,
) -> Result<Option<String>> {
    db::run_blocking_db(|| {
        let mut connection = pool.get_pg()?;
        let mut tx = connection
            .build_transaction()
            .isolation_level(postgres::IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()?;
        let snapshot = tx
            .query_opt(
                "SELECT row_to_json(execution)::text
                   FROM jobs_workflow_executions execution
                  WHERE account_id=$1 AND workflow_id=$2",
                &[&account_id, &workflow_id],
            )?
            .map(|row| row.get::<_, String>(0));
        tx.commit()?;
        Ok(snapshot)
    })
}

fn install_receipt_terminal_failure(
    pool: &crate::db::DbPool,
    account_id: &str,
    workflow_id: &str,
    command_id: &str,
) -> Result<()> {
    db::run_blocking_db(|| {
        let mut connection = pool.get_pg()?;
        let mut tx = connection.transaction()?;
        tx.batch_execute("SET LOCAL lock_timeout = '2s'; SET LOCAL statement_timeout = '5s'")?;
        let row = tx.query_one(
            "SELECT count(*), count(*) FILTER (WHERE account_id=$2 AND workflow_id=$3
                    AND start_command_id=$4 AND lifecycle_state IN ('accepted', 'running')
                    AND terminal_at_ms IS NULL)
               FROM jobs_workflow_executions WHERE first_execution_run_id=$1",
            &[
                &ACCEPTED_TEMPORAL_RUN_ID,
                &account_id,
                &workflow_id,
                &command_id,
            ],
        )?;
        anyhow::ensure!(
            row.get::<_, i64>(0) == 1 && row.get::<_, i64>(1) == 1,
            "terminal failure fixture must bind one exact nonterminal execution"
        );
        tx.batch_execute(
            "CREATE FUNCTION bluey_test_defer_receipt_terminal() RETURNS trigger
             LANGUAGE plpgsql AS $$ BEGIN
                 RAISE EXCEPTION 'owned synthetic receipt terminal update failure';
             END; $$;
             CREATE TRIGGER bluey_test_defer_receipt_terminal
             BEFORE UPDATE OF lifecycle_state ON jobs_workflow_executions
             FOR EACH ROW WHEN (OLD.first_execution_run_id = 'phase660-accepted-temporal-run-0001'
               AND OLD.lifecycle_state IN ('accepted', 'running') AND NEW.lifecycle_state = 'terminal')
             EXECUTE FUNCTION bluey_test_defer_receipt_terminal();",
        )?;
        tx.commit()?;
        Ok(())
    })
}

fn remove_receipt_terminal_failure(pool: &crate::db::DbPool) -> Result<()> {
    db::run_blocking_db(|| {
        let mut connection = pool.get_pg()?;
        let mut tx = connection.transaction()?;
        tx.batch_execute("SET LOCAL lock_timeout = '2s'; SET LOCAL statement_timeout = '5s'")?;
        tx.batch_execute(
            "DROP TRIGGER IF EXISTS bluey_test_defer_receipt_terminal ON jobs_workflow_executions;
             DROP FUNCTION IF EXISTS bluey_test_defer_receipt_terminal();",
        )?;
        tx.commit()?;
        let absent: bool = connection
            .query_one(
                "SELECT NOT EXISTS (
                   SELECT 1 FROM pg_trigger WHERE tgrelid='jobs_workflow_executions'::regclass
                     AND tgname='bluey_test_defer_receipt_terminal'
                 ) AND to_regprocedure('bluey_test_defer_receipt_terminal()') IS NULL",
                &[],
            )?
            .get(0);
        anyhow::ensure!(
            absent,
            "owned terminal failure trigger and function must be absent"
        );
        Ok(())
    })
}

fn without_workflow_execution(rows: Vec<String>) -> Vec<String> {
    rows.into_iter()
        .filter(|row| !row.starts_with("jobs_workflow_executions:"))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct JourneyAcceptedAttemptSnapshot {
    attempt_id: String,
    fence: i64,
    rows: Vec<String>,
}

fn journey_accepted_attempt_snapshot(
    pool: &crate::db::DbPool,
    account_id: &str,
    command_id: &str,
    temporal_run_id: &str,
) -> Result<JourneyAcceptedAttemptSnapshot> {
    db::run_blocking_db(|| {
        let mut connection = pool.get_pg()?;
        let mut tx = connection
            .build_transaction()
            .isolation_level(postgres::IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()?;
        let attempts = tx.query(
            "SELECT row_to_json(attempt)::text, id, attempt_no, fence
               FROM jobs_workflow_command_attempts attempt
              WHERE account_id=$1 AND command_id=$2
              ORDER BY attempt_no, id",
            &[&account_id, &command_id],
        )?;
        anyhow::ensure!(attempts.len() == 1, "one exact accepted command attempt");
        let attempt = &attempts[0];
        let attempt_id: String = attempt.get(1);
        let attempt_no: i64 = attempt.get(2);
        let fence: i64 = attempt.get(3);
        anyhow::ensure!(
            attempt_no == 1 && fence == 1,
            "accepted command must retain its first attempt and fence"
        );
        let events = tx.query(
            "SELECT row_to_json(event)::text, attempt_id, fence, event_phase,
                    event_kind, reason_code, temporal_run_id
               FROM jobs_workflow_command_attempt_events event
              WHERE account_id=$1 AND command_id=$2
              ORDER BY event_phase, event_kind, id",
            &[&account_id, &command_id],
        )?;
        anyhow::ensure!(
            events.len() == 2,
            "accepted command must have two exact events"
        );
        let mut request_started = false;
        let mut accepted = false;
        let mut rows = vec![format!("attempt:{}", attempt.get::<_, String>(0))];
        for event in events {
            let event_attempt_id: String = event.get(1);
            let event_fence: i64 = event.get(2);
            let event_phase: String = event.get(3);
            let event_kind: String = event.get(4);
            let reason_code: Option<String> = event.get(5);
            let event_temporal_run_id: Option<String> = event.get(6);
            anyhow::ensure!(
                event_attempt_id == attempt_id && event_fence == fence,
                "accepted command events must retain attempt and fence"
            );
            match (event_phase.as_str(), event_kind.as_str()) {
                ("request_started", "request_started") => {
                    anyhow::ensure!(
                        !request_started
                            && reason_code.is_none()
                            && event_temporal_run_id.is_none(),
                        "accepted request-start event authority"
                    );
                    request_started = true;
                }
                ("terminal", "accepted") => {
                    anyhow::ensure!(
                        !accepted
                            && reason_code.is_none()
                            && event_temporal_run_id.as_deref() == Some(temporal_run_id),
                        "accepted terminal event must retain the Temporal receipt"
                    );
                    accepted = true;
                }
                _ => anyhow::bail!("unexpected accepted command event"),
            }
            rows.push(format!("event:{}", event.get::<_, String>(0)));
        }
        anyhow::ensure!(
            request_started && accepted,
            "complete accepted command event pair"
        );
        tx.commit()?;
        Ok(JourneyAcceptedAttemptSnapshot {
            attempt_id,
            fence,
            rows,
        })
    })
}

fn journey_case_accepted_attempt_snapshot(
    pool: &crate::db::DbPool,
    case: ManagedReceiptJourneyCase,
    account_id: &str,
    command_id: &str,
) -> Result<Option<JourneyAcceptedAttemptSnapshot>> {
    match case {
        ManagedReceiptJourneyCase::DeliveryUnknown => Ok(None),
        ManagedReceiptJourneyCase::Accepted => journey_accepted_attempt_snapshot(
            pool,
            account_id,
            command_id,
            ACCEPTED_TEMPORAL_RUN_ID,
        )
        .map(Some),
    }
}

async fn journey_account_get(
    router: &axum::Router,
    jwt_secret: &str,
    account_id: &str,
    path: &str,
    maximum: usize,
) -> Result<(StatusCode, Vec<u8>)> {
    let token =
        crate::auth::jwt::issue(jwt_secret, account_id, crate::auth::jwt::TokenKind::Access)?;
    journey_bearer_get(router, &token, path, maximum).await
}

async fn journey_bearer_get(
    router: &axum::Router,
    token: &str,
    path: &str,
    maximum: usize,
) -> Result<(StatusCode, Vec<u8>)> {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())?,
        )
        .await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), maximum).await?.to_vec();
    Ok((status, bytes))
}

// Unlike the receipt driver, this child can start Chromium. The owned Linux
// controller must supply the isolated process/network namespace and final
// process census. No ambient tool, browser, provider or Jobs URL is accepted.
async fn run_owned_interview_prep_browser(
    node: &JourneyNode,
    frame: &serde_json::Value,
    nonce: &str,
    request_sha256: &str,
) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    anyhow::ensure!(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "browser receipt followup requires the qualified Linux x86-64 fixture"
    );
    let fence = "qualified-linux-interview-prep-fixture-v1";
    anyhow::ensure!(
        std::env::var("BLUEY_INTERVIEW_PREP_OUTER_FENCE").as_deref() == Ok(fence),
        "owned browser outer controller admission is missing"
    );
    let root = std::path::PathBuf::from(
        std::env::var_os("BLUEY_TEST_RUN_ROOT").context("owned browser run root")?,
    );
    anyhow::ensure!(
        root.is_absolute() && root.canonicalize()? == root,
        "owned browser canonical root"
    );
    journey_directory(&root, Some(node.owner), true)?;
    let capsule = root.join("receipt-prep-browser");
    journey_directory(&capsule, Some(node.owner), true)?;
    let driver = capsule.join("driver.mjs");
    let pinned = [
        (&driver, PREP_BROWSER_DRIVER_SHA256),
        (
            &capsule.join("interview-prep-fixture-gateway.mjs"),
            PREP_BROWSER_GATEWAY_SHA256,
        ),
        (
            &capsule.join("interview-prep-witness.mjs"),
            PREP_BROWSER_WITNESS_SHA256,
        ),
    ];
    for (path, digest) in pinned {
        journey_pinned_file(path, digest, node.owner)?;
    }
    journey_pinned_file(&node.tool, &node.tool_sha256, node.owner)?;
    let mut input = serde_json::to_vec(frame)?;
    input.push(b'\n');
    anyhow::ensure!(input.len() <= 16 * 1024, "owned browser frame bound");
    let mut child = tokio::process::Command::new(&node.tool)
        .arg(&driver)
        .current_dir(&root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", root.join("home"))
        .env("TMPDIR", root.join("tmp"))
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("BLUEY_INTERVIEW_PREP_OUTER_FENCE", fence)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("spawn owned interview prep browser driver")?;
    let mut stdin = child.stdin.take().context("owned browser stdin")?;
    let mut stdout = child
        .stdout
        .take()
        .context("owned browser stdout")?
        .take(4097);
    let mut stderr = child
        .stderr
        .take()
        .context("owned browser stderr")?
        .take(1025);
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    let terminal = tokio::time::timeout(Duration::from_secs(75), async {
        tokio::try_join!(
            async {
                stdin.write_all(&input).await?;
                stdin.shutdown().await?;
                drop(stdin);
                Ok::<(), std::io::Error>(())
            },
            stdout.read_to_end(&mut output),
            stderr.read_to_end(&mut diagnostics),
            child.wait(),
        )
    })
    .await;
    input.fill(0);
    let (_, _, _, status) = match terminal {
        Ok(Ok(result)) => result,
        failure => {
            if child.try_wait()?.is_none() {
                tokio::time::timeout(Duration::from_secs(5), child.kill())
                    .await
                    .context("owned browser kill/join deadline")??;
            }
            if failure.is_err() {
                anyhow::bail!("owned browser terminal deadline");
            }
            anyhow::bail!("owned browser pipe I/O");
        }
    };
    anyhow::ensure!(
        status.success() && diagnostics.is_empty(),
        "owned browser child exited unsuccessfully"
    );
    anyhow::ensure!(
        output.len() <= 4096
            && output.last() == Some(&b'\n')
            && output.iter().filter(|&&b| b == b'\n').count() == 1,
        "owned browser terminal contract"
    );
    let result: serde_json::Value = serde_json::from_slice(&output)
        .map_err(|_| anyhow::anyhow!("owned browser result JSON"))?;
    anyhow::ensure!(
        result
            == json!({
                "schema": "jobs-interview-prep-browser-result-v2",
                "nonce": nonce,
                "committedWorkspaceReceiptObserved": true,
                "loginFormObserved": false,
                "exactGenerateRequests": 1,
                "structuredDraftDownloads": 2,
                "textDraftDownloads": 2,
                "prepForwardAttempts": 1,
                "prepRequestSha256": request_sha256,
            }),
        "owned browser receipt/download witness"
    );
    for (path, digest) in pinned {
        journey_pinned_file(path, digest, node.owner)?;
    }
    journey_pinned_file(&node.tool, &node.tool_sha256, node.owner)
}

struct InterviewPrepActors<'a> {
    account_id: &'a str,
    other_id: &'a str,
}

async fn verify_postgres_interview_prep_from_receipt(
    pool: &crate::db::DbPool,
    router: &axum::Router,
    provider: &MockServer,
    jwt_secret: &str,
    actors: InterviewPrepActors<'_>,
    application: &jobs::JobApplication,
    browser: Option<(&JourneyNode, u16)>,
) -> Result<()> {
    let InterviewPrepActors {
        account_id,
        other_id,
    } = actors;
    let model_origin = provider.uri();
    anyhow::ensure!(
        provider.address().ip().is_loopback()
            && std::env::var("BLUEY_TEST_OPENAI_URL").as_deref() == Ok(model_origin.as_str()),
        "warmup provider must remain bound to the owned loopback mock"
    );
    let receipt = &application.receipt;
    let resume_id = application
        .resume_version_id
        .as_deref()
        .context("submitted resume identity")?;
    let resume =
        jobs::get_resume_version(pool, account_id, resume_id)?.context("submitted resume")?;
    let evidence = jobs::list_application_evidence(pool, account_id, Some(&application.id))?;
    let resume_evidence = evidence
        .iter()
        .find(|item| item.kind == "resume")
        .context("durable submitted resume evidence")?;
    // This is the server's canonical receipt serialization for the consent
    // fence, not the receipt-object blob digest.
    let receipt_fingerprint = hex::encode(Sha256::digest(receipt.to_string().as_bytes()));
    let census = |identity: &str| -> Result<[i64; 8]> {
        let mut hasher = Sha256::new();
        hasher.update(b"bluey-provider-cost-hold-v1\0account");
        hasher.update((identity.len() as u64).to_be_bytes());
        hasher.update(identity.as_bytes());
        let scope = hex::encode(hasher.finalize());
        db::run_blocking_db(|| {
            let row = pool.get_pg()?.query_one(
                "SELECT balance_cents,trial_seconds_remaining,
                   (SELECT count(*) FROM usage_events WHERE account_id=$1 AND kind='llm' AND request_id LIKE $2),
                   (SELECT count(*) FROM usage_events WHERE account_id=$1 AND kind='llm_attempt' AND request_id LIKE $2),
                   (SELECT count(*) FROM usage_reservations WHERE account_id=$1 AND status='settled' AND request_id LIKE $2),
                   (SELECT count(*) FROM jobs_provider_cost_holds WHERE account_scope_hash=$3 AND status='settled'),
                   (SELECT count(*) FROM usage_reservations WHERE account_id=$1 AND request_id LIKE $2),
                   (SELECT count(*) FROM jobs_provider_cost_holds WHERE account_scope_hash=$3)
                 FROM accounts WHERE id=$1",
                &[&identity, &"bluey-internal:jobs-interview-prep-v3-%", &scope],
            )?;
            Ok([
                row.try_get(0)?,
                row.try_get(1)?,
                row.try_get(2)?,
                row.try_get(3)?,
                row.try_get(4)?,
                row.try_get(5)?,
                row.try_get(6)?,
                row.try_get(7)?,
            ])
        })
    };
    let endpoint = format!("/api/jobs/applications/{}/interview-prep", application.id);
    let post = |identity: Option<&str>, body: &str| -> Result<Request<Body>> {
        let mut builder =
            Request::post(endpoint.as_str()).header("content-type", "application/json");
        if let Some(identity) = identity {
            let token =
                crate::auth::jwt::issue(jwt_secret, identity, crate::auth::jwt::TokenKind::Access)?;
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        Ok(builder.body(Body::from(body.as_bytes().to_vec()))?)
    };
    let before = census(account_id)?;
    let other_before = census(other_id)?;
    let consent_body = serde_json::to_string(&json!({
        "prepare": true,
        "receipt_fingerprint": receipt_fingerprint,
    }))?;
    let refused_consent_body = serde_json::to_string(&json!({
        "prepare": false,
        "receipt_fingerprint": receipt_fingerprint,
    }))?;
    anyhow::ensure!(
        before[0] == 0 && before[1] > 0 && before[2..] == [0, 0, 0, 0, 0, 0],
        "warmup must start with untouched synthetic trial allowance"
    );
    for (identity, input, status) in [
        (None, consent_body.as_str(), StatusCode::UNAUTHORIZED),
        (Some(other_id), consent_body.as_str(), StatusCode::NOT_FOUND),
        (
            Some(account_id),
            refused_consent_body.as_str(),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = router.clone().oneshot(post(identity, input)?).await?;
        anyhow::ensure!(
            response.status() == status,
            "warmup refusal status: {}",
            response.status()
        );
    }
    anyhow::ensure!(
        census(account_id)? == before
            && census(other_id)? == other_before
            && provider
                .received_requests()
                .await
                .context("warmup request capture")?
                .is_empty(),
        "refusal must not invoke model or alter either usage ledger"
    );
    let stale_fingerprint = if receipt_fingerprint == "0".repeat(64) {
        "1".repeat(64)
    } else {
        "0".repeat(64)
    };
    let stale_consent_body = serde_json::to_string(&json!({
        "prepare": true,
        "receipt_fingerprint": stale_fingerprint,
    }))?;
    for input in [r#"{"prepare":true}"#, stale_consent_body.as_str()] {
        let response = router
            .clone()
            .oneshot(post(Some(account_id), input)?)
            .await?;
        anyhow::ensure!(
            response.status() == StatusCode::CONFLICT,
            "missing or stale first-party consent must conflict: {}",
            response.status()
        );
    }
    anyhow::ensure!(
        census(account_id)? == before
            && census(other_id)? == other_before
            && provider
                .received_requests()
                .await
                .context("warmup stale consent capture")?
                .is_empty(),
        "missing or stale consent must not invoke model or alter either usage ledger"
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer synthetic-warmup-provider"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": WARMUP_CONTENT}}],
            "usage": {"prompt_tokens": 20, "completion_tokens": 15},
        })))
        .mount(provider)
        .await;
    let browser_census = if let Some((node, port)) = browser {
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let token =
            crate::auth::jwt::issue(jwt_secret, account_id, crate::auth::jwt::TokenKind::Access)?;
        let frame = json!({
            "schema": "jobs-interview-prep-browser-v1",
            "nonce": nonce,
            "jobsPort": port,
            "accountId": account_id,
            "accessToken": token,
            "balanceCents": before[0],
            "expected": {
                "applicationId": application.id,
                "receiptId": receipt["receiptId"],
                "resumeVersionId": resume_id,
                "resumeVersionNo": resume.version_no,
                "resumeChecksum": resume.checksum,
                "resumeDocumentSha256": resume_evidence.sha256,
                "receiptFingerprint": receipt_fingerprint,
                "company": receipt["job"]["company"],
                "jobTitle": receipt["job"]["title"],
                "provider": "openai",
                "model": "gpt-5.5",
                "content": WARMUP_CONTENT,
            },
        });
        let request_sha256 = hex::encode(Sha256::digest(
            format!("POST\n{endpoint}\n{consent_body}").as_bytes(),
        ));
        run_owned_interview_prep_browser(node, &frame, &nonce, &request_sha256).await?;
        let after = census(account_id)?;
        anyhow::ensure!(
            after[0] == before[0]
                && after[1] < before[1]
                && after[2..] == [1, 1, 1, 1, 1, 1]
                && provider
                    .received_requests()
                    .await
                    .context("warmup request capture")?
                    .len()
                    == 1
                && census(other_id)? == other_before,
            "browser first request must settle exactly one owned model invocation"
        );
        Some(after)
    } else {
        None
    };
    let mut first = None;
    let direct_attempts = if browser_census.is_some() { 1 } else { 2 };
    for _ in 0..direct_attempts {
        let response = router
            .clone()
            .oneshot(post(Some(account_id), &consent_body)?)
            .await?;
        anyhow::ensure!(
            response.status() == StatusCode::OK,
            "warmup generation status: {}",
            response.status()
        );
        let draft: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await?)?;
        let id = draft["id"].as_str().context("warmup draft identity")?;
        anyhow::ensure!(
            id.len() == 29
                && id.starts_with("prep-")
                && draft["application_id"] == application.id
                && draft["content"] == WARMUP_CONTENT
                && draft["provider"] == "openai"
                && draft["cost_cents"] == 0
                && draft["grounding"]["receipt_id"] == receipt["receiptId"]
                && draft["grounding"]["receipt_fingerprint"] == receipt_fingerprint
                && draft["grounding"]["resume_version_id"] == resume_id
                && draft["grounding"]["resume_checksum"] == resume.checksum
                && draft["grounding"]["resume_document_sha256"] == resume_evidence.sha256,
            "warmup output must bind the exact committed receipt and submitted resume"
        );
        let after = census(account_id)?;
        if let Some(browser_census) = browser_census {
            anyhow::ensure!(
                after == browser_census,
                "direct replay must preserve browser settlement"
            );
        } else if let Some((first_id, first_census)) = &first {
            anyhow::ensure!(
                id == first_id && after == *first_census,
                "warmup replay must not settle twice"
            );
        } else {
            anyhow::ensure!(
                after[0] == before[0] && after[1] < before[1] && after[2..] == [1, 1, 1, 1, 1, 1],
                "warmup must settle model and synthetic trial usage once"
            );
            first = Some((id.to_owned(), after));
        }
    }
    let requests = provider
        .received_requests()
        .await
        .context("warmup request readback")?;
    anyhow::ensure!(
        requests.len() == 1 && census(other_id)? == other_before,
        "only the accepted warmup may invoke the owned model or consume trial usage"
    );
    let provider_body = std::str::from_utf8(&requests[0].body)?;
    let title = receipt
        .pointer("/job/title")
        .and_then(serde_json::Value::as_str)
        .context("frozen posting title")?;
    anyhow::ensure!(
        provider_body.contains(title)
            && !provider_body.contains(&format!("{account_id}@example.test")),
        "provider prompt uses the frozen role and excludes candidate contact data"
    );
    Ok(())
}

struct EnvironmentRestore {
    previous: Vec<(&'static str, Option<OsString>)>,
}

impl EnvironmentRestore {
    fn install(values: &[(&'static str, Option<String>)]) -> Self {
        let previous = values
            .iter()
            .map(|(name, value)| {
                let previous = std::env::var_os(name);
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
                (*name, previous)
            })
            .collect();
        Self { previous }
    }
}

impl Drop for EnvironmentRestore {
    fn drop(&mut self) {
        for (name, previous) in self.previous.iter().rev() {
            match previous {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

fn exact_stored_request_start_gateway_authority(
    pool: &crate::db::DbPool,
    account_id: &str,
    command_id: &str,
) -> Result<ManagedCloudGatewayAuthority> {
    db::run_blocking_db(|| -> Result<_> {
        let mut connection = pool.get_pg()?;
        let mut transaction = connection
            .build_transaction()
            .isolation_level(postgres::IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .context("start read-only request-start authority inspection")?;
        let row = transaction
            .query_opt(
                "SELECT account_id, command_id, gateway_authority_base64url, gateway_authority_sha256
                   FROM jobs_managed_cloud_request_start_authorities
                  WHERE account_id = $1 AND command_id = $2",
                &[&account_id, &command_id],
            )?
            .context("stored managed request-start authority")?;
        let stored_account_id: String = row.get(0);
        let stored_command_id: String = row.get(1);
        let encoded: String = row.get(2);
        let stored_sha256: String = row.get(3);
        anyhow::ensure!(
            stored_account_id == account_id && stored_command_id == command_id,
            "stored request-start authority row identity"
        );
        anyhow::ensure!(
            !encoded.is_empty() && encoded.len() <= MAX_STORED_GATEWAY_AUTHORITY_BASE64URL_BYTES,
            "stored request-start authority base64url bound"
        );
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&encoded)
            .context("decode stored request-start authority base64url")?;
        anyhow::ensure!(
            !bytes.is_empty()
                && base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes) == encoded,
            "stored request-start authority canonical base64url"
        );
        anyhow::ensure!(
            hex::encode(Sha256::digest(&bytes)) == stored_sha256,
            "stored request-start authority sha256"
        );
        let authority = serde_json::from_slice(&bytes)
            .context("parse stored request-start managed authority")?;
        transaction.commit()?;
        Ok(authority)
    })
}

fn require_exact_gateway_authority(
    outbound: &ManagedCloudGatewayAuthority,
    stored: &ManagedCloudGatewayAuthority,
) -> Result<()> {
    anyhow::ensure!(
        outbound == stored,
        "outbound managed authority differs from the stored request-start authority"
    );
    Ok(())
}

async fn abort_dispatcher(handle: JoinHandle<()>) -> Result<()> {
    handle.abort();
    match handle.await {
        Err(error) if error.is_cancelled() => Ok(()),
        Err(error) => anyhow::bail!("dispatcher abort join failed: {error}"),
        Ok(()) => anyhow::bail!("dispatcher returned unexpectedly instead of cancelling"),
    }
}

fn drop_gateway(gateway: MockServer) -> std::thread::Result<()> {
    std::panic::catch_unwind(AssertUnwindSafe(|| drop(gateway)))
}

// Test-only spelling of managed_cloud_canonical_json: recursive lexical object
// ordering, safe integers only, compact serde strings/arrays, terminal LF.
// Contracts: managed_cloud_release_authority.rs:1562-1625, :457-465 and :1367-1382.
fn managed_authority_test_canonical_bytes(value: &serde_json::Value) -> Result<Vec<u8>> {
    fn ordered(value: &serde_json::Value) -> Result<serde_json::Value> {
        Ok(match value {
            serde_json::Value::Object(object) => {
                let mut keys: Vec<_> = object.keys().collect();
                keys.sort_unstable();
                let mut output = serde_json::Map::new();
                for key in keys {
                    output.insert(key.clone(), ordered(&object[key])?);
                }
                serde_json::Value::Object(output)
            }
            serde_json::Value::Array(array) => {
                serde_json::Value::Array(array.iter().map(ordered).collect::<Result<Vec<_>>>()?)
            }
            serde_json::Value::Number(number) => {
                const MAX: i64 = 9_007_199_254_740_991;
                anyhow::ensure!(
                    !number.is_f64()
                        && (number.as_i64().is_some_and(|n| (-MAX..=MAX).contains(&n))
                            || number.as_u64().is_some_and(|n| n <= MAX as u64)),
                    "managed test canonical number must be a safe integer"
                );
                value.clone()
            }
            _ => value.clone(),
        })
    }
    let mut bytes = serde_json::to_vec(&ordered(value)?)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn managed_readiness_test_digest_at(
    readiness: &jobs::ManagedCloudReadiness,
    evaluated_at_ms: i64,
) -> Result<String> {
    // Exactly ManagedCloudReadinessDigestAuthority, not a provisioning-signing input.
    let bytes = managed_authority_test_canonical_bytes(&json!({
        "version": 1,
        "audience": "bluey-jobs-managed-cloud-readiness-v1",
        "status": readiness.status,
        "evaluatedAtMs": evaluated_at_ms,
        "missingRoles": readiness.missing_roles,
        "staleRoles": readiness.stale_roles,
        "cohortEligible": readiness.cohort_eligible,
    }))?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}

fn managed_recovery_test_digest(
    execution: &jobs::ManagedCloudExecutionAuthority,
    current: &jobs::ManagedCloudCurrentAuthorization,
) -> Result<String> {
    let bytes = managed_authority_test_canonical_bytes(&json!({
        "version": 1,
        "audience": "bluey-jobs-managed-cloud-recovery-authorization-v1",
        "bindingSha256": execution.binding_sha256,
        "currentHeadRevision": current.current_head_revision,
        "currentTransitionSha256": current.current_transition_sha256,
        "currentActivationSha256": current.current_activation_sha256,
        "currentManifestSha256": current.current_manifest_sha256,
        "currentActivationExpiresAtMs": current.current_activation_expires_at_ms,
        "currentTaskQueueSha256": current.current_task_queue_sha256,
        "currentFailureConverterSha256": current.current_failure_converter_sha256,
        "currentReadinessSha256": current.current_readiness_sha256,
        "frozenActivationSha256": execution.admission.activation_sha256,
        "frozenManifestSha256": execution.admission.manifest_sha256,
        "recoveryAccepted": current.recovery_accepted,
        "authorizedAtMs": current.authorized_at_ms,
    }))?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}

fn managed_effect_test_gateway(
    execution: &jobs::ManagedCloudExecutionAuthority,
    readiness: &jobs::ManagedCloudReadiness,
    authorized_at_ms: i64,
) -> Result<ManagedCloudGatewayAuthority> {
    let status = &readiness.status;
    anyhow::ensure!(
        status.authority_ready
            && status.customer_admission
            && status.unavailability_reason.is_none()
            && readiness.cohort_eligible
            && readiness.missing_roles.is_empty()
            && readiness.stale_roles.is_empty(),
        "real fixture readiness must admit this managed effect"
    );
    let mut current = jobs::ManagedCloudCurrentAuthorization {
        current_head_revision: status.head_revision,
        current_transition_sha256: status
            .transition_sha256
            .clone()
            .context("current transition")?,
        current_activation_sha256: status
            .activation_sha256
            .clone()
            .context("current activation")?,
        current_manifest_sha256: status.manifest_sha256.clone().context("current manifest")?,
        current_activation_expires_at_ms: status
            .expires_at_ms
            .context("current activation expiry")?,
        current_task_queue_sha256: status
            .task_queue_sha256
            .clone()
            .context("current task queue")?,
        current_failure_converter_sha256: status
            .failure_converter_sha256
            .clone()
            .context("current failure converter")?,
        current_readiness_sha256: managed_readiness_test_digest_at(readiness, authorized_at_ms)?,
        recovery_accepted: false,
        recovery_authorization_sha256: String::new(),
        authorized_at_ms,
    };
    let frozen = &execution.admission;
    anyhow::ensure!(
        status.scope == frozen.scope
            && current.current_head_revision == frozen.head_revision
            && current.current_transition_sha256 == frozen.transition_sha256
            && current.current_activation_sha256 == frozen.activation_sha256
            && current.current_manifest_sha256 == frozen.manifest_sha256
            && current.current_activation_expires_at_ms == frozen.activation_expires_at_ms
            && current.current_task_queue_sha256 == frozen.task_queue_sha256
            && current.current_failure_converter_sha256 == frozen.failure_converter_sha256
            && authorized_at_ms >= frozen.resolved_at_ms
            && authorized_at_ms < current.current_activation_expires_at_ms,
        "owned fixture current authority must remain the exact frozen head, not recovery"
    );
    current.recovery_authorization_sha256 = managed_recovery_test_digest(execution, &current)?;
    Ok(ManagedCloudGatewayAuthority {
        version: 1,
        execution: execution.clone(),
        authorization: current,
    })
}

fn managed_effect_protected_state(
    pool: &crate::db::DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    volume_id: &str,
    enrollment_epoch: i64,
) -> Result<Vec<String>> {
    db::run_blocking_db(|| -> Result<_> {
        let mut connection = pool.get_pg()?;
        let mut tx = connection
            .build_transaction()
            .isolation_level(postgres::IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()?;
        // Entire domain rows include all twelve immutable managed columns,
        // token hash, fence, expiry, Phase A/no Phase B and every proof use.
        let rows = tx.query(
            "SELECT 'lease:' || row_to_json(lease)::text FROM jobs_execution_leases lease
               WHERE account_id=$1 AND application_id=$2 AND run_id=$3
             UNION ALL SELECT 'volume:' || row_to_json(binding)::text
               FROM jobs_execution_lease_volume_bindings binding WHERE run_id=$3
             UNION ALL SELECT 'runtime:' || row_to_json(runtime)::text
               FROM jobs_execution_lease_process_runtime_bindings runtime WHERE run_id=$3
             UNION ALL SELECT 'subject:' || row_to_json(subject)::text
               FROM jobs_runner_account_subjects subject WHERE account_id=$1
             UNION ALL SELECT 'residency:' || row_to_json(residency)::text
               FROM jobs_runner_volume_residencies residency
               JOIN jobs_runner_account_subjects subject
                 ON subject.purge_subject=residency.purge_subject
               WHERE subject.account_id=$1 AND residency.volume_id=$4 AND residency.volume_epoch=$5
             UNION ALL SELECT 'proof:' || row_to_json(proof)::text
               FROM jobs_runner_volume_authority_uses proof
               WHERE volume_id=$4 AND enrollment_epoch=$5
             UNION ALL SELECT 'application:' || row_to_json(application)::text
               FROM jobs_applications application WHERE account_id=$1 AND id=$2
             UNION ALL SELECT 'session:' || row_to_json(browser_session)::text
               FROM jobs_browser_sessions browser_session WHERE account_id=$1
             UNION ALL SELECT 'ats:' || row_to_json(ats)::text
               FROM jobs_application_ats_certification_bindings ats
               WHERE account_id=$1 AND application_id=$2 AND run_id=$3
             UNION ALL SELECT 'irreversible:' || row_to_json(receipt)::text
               FROM jobs_managed_cloud_irreversible_effect_receipts receipt
               WHERE account_id=$1 AND application_id=$2 AND run_id=$3
             ORDER BY 1",
            &[
                &account_id,
                &application_id,
                &run_id,
                &volume_id,
                &enrollment_epoch,
            ],
        )?;
        let rows = rows
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();
        tx.commit()?;
        Ok(rows)
    })
}

fn signed_journey_worker_request(
    request_path: &str,
    scope: &str,
    body: &serde_json::Value,
    worker_id: &str,
    timestamp: u64,
    nonce: &str,
    signing_key: &str,
) -> Request<Body> {
    let bytes = serde_json::to_vec(body).expect("managed execution JSON");
    let content_sha256 = hex::encode(Sha256::digest(&bytes));
    let canonical = format!(
        "bluey-jobs-worker-v1\n{timestamp}\n{nonce}\n{worker_id}\n\
         bluey-jobs-api\n{scope}\nPOST\n{request_path}\n{content_sha256}"
    );
    let mut mac = HmacSha256::new_from_slice(signing_key.as_bytes()).expect("HMAC key");
    mac.update(canonical.as_bytes());
    Request::post(request_path)
        .header("x-bluey-jobs-worker-id", worker_id)
        .header("x-bluey-jobs-worker-timestamp", timestamp.to_string())
        .header("x-bluey-jobs-worker-nonce", nonce)
        .header("x-bluey-jobs-worker-audience", "bluey-jobs-api")
        .header("x-bluey-jobs-worker-scope", scope)
        .header("x-bluey-jobs-worker-content-sha256", content_sha256)
        .header(
            "x-bluey-jobs-worker-signature",
            hex::encode(mac.finalize().into_bytes()),
        )
        .header("content-type", "application/json")
        .body(Body::from(bytes))
        .expect("managed claim request")
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn postgres_managed_start_dispatch_marks_delivery_unknown_when_configured() -> Result<()> {
    postgres_managed_start_receipt_journey(
        ManagedReceiptJourneyCase::DeliveryUnknown,
        ReceiptFollowup::Replay,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn postgres_managed_start_accepted_receipt_replay_is_read_only() -> Result<()> {
    postgres_managed_start_receipt_journey(
        ManagedReceiptJourneyCase::Accepted,
        ReceiptFollowup::Replay,
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn postgres_managed_receipt_replay_recovers_terminal_update_failure() -> Result<()> {
    postgres_managed_start_receipt_journey(
        ManagedReceiptJourneyCase::Accepted,
        ReceiptFollowup::RecoverTerminalUpdate,
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn postgres_managed_receipt_grounds_authenticated_interview_prep() -> Result<()> {
    postgres_managed_start_receipt_journey(
        ManagedReceiptJourneyCase::Accepted,
        ReceiptFollowup::InterviewPrep,
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn postgres_managed_receipt_browser_downloads_interview_prep() -> Result<()> {
    postgres_managed_start_receipt_journey(
        ManagedReceiptJourneyCase::Accepted,
        ReceiptFollowup::InterviewPrepBrowser,
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn postgres_managed_receipt_after_verified_signup() -> Result<()> {
    postgres_managed_start_receipt_journey(
        ManagedReceiptJourneyCase::Accepted,
        ReceiptFollowup::VerifiedSignup,
    )
    .await
}

const REVIEW_FIRST_SOURCE: &str = concat!(
    "Casey Rowan\ncandidate@bluey-qa.test\nData Engineer\n\nSUMMARY\n",
    "Data Engineer building reliable, observable data pipelines for synthetic QA workloads.\n\n",
    "EXPERIENCE\nData Engineer\nBluey QA Systems\nJan 2023 - Present\n",
    "• Built synthetic batch pipelines with Python and SQL.\n",
    "• Improved data-quality checks for repeatable test fixtures.\n\n",
    "SKILLS\nPython, SQL, PostgreSQL, Airflow",
);
const REVIEW_FIRST_SOURCE_SHA256: &str =
    "92b41ea87bbc39031cac457edb51ba6693bb9e5773b0e18f5c370fee0e2a1cdb";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn postgres_managed_review_first_approval_after_verified_signup() -> Result<()> {
    postgres_review_first_signup_journey(ReviewFirstJourney::ApprovalOnly).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn postgres_review_first_mock_receipt_after_verified_signup() -> Result<()> {
    postgres_review_first_signup_journey(ReviewFirstJourney::MockReceipt).await
}

#[derive(Clone, Copy)]
enum ReviewFirstJourney {
    ApprovalOnly,
    MockReceipt,
}

async fn postgres_review_first_signup_journey(journey: ReviewFirstJourney) -> Result<()> {
    let case = match journey {
        ReviewFirstJourney::ApprovalOnly => OWNED_REVIEW_FIRST_CASE,
        ReviewFirstJourney::MockReceipt => OWNED_REVIEW_FIRST_MOCK_RECEIPT_CASE,
    };
    let owned_url =
        tokio::task::spawn_blocking(move || crate::db::owned_postgres_test::owned_url(case))
            .await
            .context("owned review-first PostgreSQL custody guard task")?;
    let Some(database_url) = owned_url else {
        return Ok(());
    };
    let _environment = EnvironmentRestore::install(&[
        ("BLUEY_JOBS_BETA_ENABLED", Some("1".to_string())),
        ("BLUEY_JOBS_MODEL_GENERATION_ENABLED", Some("0".to_string())),
        (DISPATCH_FLAG, Some("0".to_string())),
        (RECONCILIATION_FLAG, Some("0".to_string())),
        ("BLUEY_JOBS_WORKFLOW_ORIGIN", None),
        ("BLUEY_JOBS_WORKFLOW_TOKEN", None),
        (
            "BLUEY_JOBS_WORKER_TOKEN",
            matches!(journey, ReviewFirstJourney::MockReceipt)
                .then(|| REVIEW_FIRST_MOCK_WORKER_TOKEN.to_string()),
        ),
        (
            "BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED",
            Some("0".to_string()),
        ),
        (
            "BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED",
            Some("0".to_string()),
        ),
        ("HTTP_PROXY", None),
        ("http_proxy", None),
        ("HTTPS_PROXY", None),
        ("https_proxy", None),
        ("ALL_PROXY", None),
        ("all_proxy", None),
    ]);
    let pool = db::run_blocking_db(|| -> Result<_> {
        let pool = db::open_postgres_pool(&database_url)?;
        db::run_migrations(&pool)?;
        Ok(pool)
    })?;
    let candidate = owned_signup_fixture::signup_candidate(
        &pool,
        &database_url,
        JOURNEY_JWT_SECRET,
        NEW_CANDIDATE_EMAIL,
    )
    .await?;
    db::run_blocking_db(|| -> Result<()> {
        jobs::integration_test_runner_control_plane::bind_verified_signup_candidate(
            &pool,
            &candidate.account_id,
        )?;
        let admin = db::accounts::Account::create_with_admin(
            &pool,
            "review-first-fixture-admin@bluey-qa.test",
            "fixture-only",
            true,
        )?;
        anyhow::ensure!(db::accounts::Account::mark_email_verified(&pool, &admin.id)? == 1);
        let cohort = jobs_beta_access::get_public_beta_cohort(&pool)?;
        anyhow::ensure!(cohort.state == PublicBetaCohortState::Draft && cohort.assigned_count == 0);
        let now = jobs::now_ms();
        jobs_beta_access::update_public_beta_cohort_audited(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: cohort.revision,
                state: PublicBetaCohortState::Open,
                opens_at_ms: Some(now - 60_000),
                closes_at_ms: Some(now + 3_600_000),
                hard_cap: 1,
            },
            &admin.id,
        )?;
        let decision =
            jobs_beta_access::evaluate_or_enroll_public_beta(&pool, &candidate.account_id)?;
        anyhow::ensure!(decision.reason == PublicBetaAccessReason::Admitted);
        Ok(())
    })?;
    let bytes = REVIEW_FIRST_SOURCE.as_bytes();
    anyhow::ensure!(
        bytes.len() == 368 && hex::encode(Sha256::digest(bytes)) == REVIEW_FIRST_SOURCE_SHA256
    );
    let source = upload_verified_candidate_resume(
        &pool,
        &database_url,
        &candidate,
        bytes,
        REVIEW_FIRST_SOURCE_SHA256,
        SignupResumeFormat::BrowserSampleText,
    )
    .await?;
    let application_id = review_first_signup_fixture::review_first_approve(
        &pool,
        &database_url,
        &candidate,
        &source,
    )
    .await?;
    match journey {
        ReviewFirstJourney::ApprovalOnly => {
            println!("phase660_owned_postgres_review_first_signup_approval_complete=true");
        }
        ReviewFirstJourney::MockReceipt => {
            review_first_mock_receipt_fixture::verify_mock_receipt(
                &pool,
                owned_signup_fixture::base_config(&database_url, JOURNEY_JWT_SECRET),
                (&candidate.account_id, &candidate.access_token),
                &source,
                &application_id,
                REVIEW_FIRST_MOCK_WORKER_TOKEN,
            )
            .await?;
            println!("phase660_owned_postgres_review_first_signup_mock_receipt_complete=true");
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum SignupResumeFormat {
    SyntheticPdf,
    BrowserSampleText,
}

async fn upload_verified_candidate_resume(
    pool: &db::DbPool,
    database_url: &str,
    candidate: &owned_signup_fixture::SignedUpCandidate,
    bytes: &[u8],
    sha256: &str,
    format: SignupResumeFormat,
) -> Result<ResumeSourceAsset> {
    use crate::object_storage::ObjectStorage;

    let store = MockServer::start().await;
    anyhow::ensure!(
        store.address().ip().is_loopback(),
        "source store must be local"
    );
    let bucket = "synthetic-signup-source";
    let storage = crate::config::ObjectStorageConfig {
        endpoint_url: store.uri(),
        bucket: bucket.to_string(),
        access_key_id: "fixture-only".to_string(),
        secret_access_key: "fixture-only".to_string(),
        region: "auto".to_string(),
        key_prefix: "owned-signup".to_string(),
        retention_days: 1,
        max_object_bytes: 1024 * 1024,
    };
    let request_id = uuid::Uuid::new_v4().to_string();
    let (file_name, file_type, media_type, page_count, profile) = match format {
        SignupResumeFormat::SyntheticPdf => (format!("resume-{sha256}.pdf"), "pdf", "application/pdf", Some(1), None),
        SignupResumeFormat::BrowserSampleText => (
            "bluey-qa-resume.txt".to_string(), "txt", "text/plain", None,
            Some(jobs::CareerProfile {
                full_name: "Casey Rowan".into(),
                email: NEW_CANDIDATE_EMAIL.into(),
                headline: "Data Engineer".into(),
                summary: "Data Engineer building reliable, observable data pipelines for synthetic QA workloads.".into(),
                employment: vec![jobs::EmploymentEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    company: "Bluey QA Systems".into(), title: "Data Engineer".into(),
                    start_date: "2023-01".into(), current: true,
                    highlights: vec!["Built synthetic batch pipelines with Python and SQL.".into(),
                        "Improved data-quality checks for repeatable test fixtures.".into()],
                    ..jobs::EmploymentEntry::default()
                }],
                skills: vec!["Python".into(), "SQL".into(), "PostgreSQL".into(), "Airflow".into()],
                ..jobs::CareerProfile::default()
            }),
        ),
    };
    let object_key = ObjectStorage::new(storage.clone()).resume_source_key(
        &candidate.account_id,
        &request_id,
        sha256,
        file_type,
    );
    let object_path = format!("/{bucket}/{object_key}");
    let stored = Arc::new(Mutex::new(None::<Vec<u8>>));
    let source_put = Arc::clone(&stored);
    let exact = bytes.to_vec();
    Mock::given(method("PUT"))
        .and(path(object_path.as_str()))
        .respond_with(move |request: &wiremock::Request| {
            let Ok(mut stored) = source_put.lock() else {
                return ResponseTemplate::new(500);
            };
            if stored.is_some() || request.body != exact {
                return ResponseTemplate::new(409);
            }
            *stored = Some(request.body.clone());
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(&store)
        .await;
    let source_get = Arc::clone(&stored);
    Mock::given(method("GET"))
        .and(path(object_path.as_str()))
        .respond_with(move |_request: &wiremock::Request| {
            let Ok(stored) = source_get.lock() else {
                return ResponseTemplate::new(500);
            };
            match stored.as_ref() {
                Some(bytes) => ResponseTemplate::new(200).set_body_raw(bytes.clone(), media_type),
                None => ResponseTemplate::new(404),
            }
        })
        .expect(1)
        .mount(&store)
        .await;
    let _environment = EnvironmentRestore::install(&[
        ("BLUEY_JOBS_BETA_ENABLED", Some("1".to_string())),
        ("HTTP_PROXY", None),
        ("http_proxy", None),
        ("HTTPS_PROXY", None),
        ("https_proxy", None),
        ("ALL_PROXY", None),
        ("all_proxy", None),
    ]);
    let mut config = owned_signup_fixture::base_config(database_url, JOURNEY_JWT_SECRET);
    config.object_storage = Some(storage);
    let router = crate::api::build_jobs_router(pool.clone(), config);
    let mut input = json!({
        "request_id": request_id, "file_name": file_name, "media_type": media_type,
        "page_count": page_count, "bytes_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
    });
    if let Some(profile) = profile {
        input["profile"] = serde_json::to_value(profile)?;
    }
    let response = router
        .clone()
        .oneshot(
            Request::post("/api/jobs/resume-source")
                .header(
                    "authorization",
                    format!("Bearer {}", candidate.access_token),
                )
                .header("content-type", "application/json")
                .body(Body::from(input.to_string()))?,
        )
        .await?;
    anyhow::ensure!(
        response.status() == StatusCode::OK,
        "verified candidate source upload returned {}",
        response.status()
    );
    let upload: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
    let (status, body) = journey_bearer_get(
        &router,
        &candidate.access_token,
        "/api/jobs/resume-source",
        65536,
    )
    .await?;
    let readback: serde_json::Value = serde_json::from_slice(&body)?;
    let source = jobs::get_resume_source_asset(pool, &candidate.account_id)?
        .context("signed-up candidate must have one durable source résumé")?;
    anyhow::ensure!(
        status == StatusCode::OK
            && upload["asset"] == readback
            && source.id == request_id
            && source.file_name == file_name
            && source.file_type == file_type
            && source.media_type == media_type
            && source.storage_key == object_key
            && source.sha256 == sha256
            && source.size_bytes == i64::try_from(bytes.len())?
            && readback["id"] == source.id
            && readback["sha256"] == sha256
            && upload["profile"]["source_resume_asset_id"] == source.id,
        "source upload and independent authenticated storage readback must agree"
    );
    let requests = store
        .received_requests()
        .await
        .context("source store request census")?;
    anyhow::ensure!(
        requests.len() == 2
            && requests[0].method.as_str() == "PUT"
            && requests[1].method.as_str() == "GET"
            && requests
                .iter()
                .all(|request| request.url.path() == object_path),
        "verified candidate must use one exact source object PUT and readback"
    );
    Ok(source)
}

async fn postgres_managed_start_receipt_journey(
    case: ManagedReceiptJourneyCase,
    followup: ReceiptFollowup,
) -> Result<()> {
    // owned_url authenticates the owned live cluster with postgres::Client::connect.
    // postgres 0.19 creates its own runtime there, so the synchronous custody
    // guard must not run on this #[tokio::test] runtime thread.
    let defer_terminal_update = followup == ReceiptFollowup::RecoverTerminalUpdate;
    let use_workflow_activity =
        case == ManagedReceiptJourneyCase::Accepted && followup == ReceiptFollowup::Replay;
    let owned_case = match followup {
        ReceiptFollowup::Replay => case.owned_case(),
        ReceiptFollowup::RecoverTerminalUpdate => OWNED_TERMINAL_RETRY_CASE,
        ReceiptFollowup::InterviewPrep => OWNED_WARMUP_CASE,
        ReceiptFollowup::InterviewPrepBrowser => OWNED_WARMUP_BROWSER_CASE,
        ReceiptFollowup::VerifiedSignup => OWNED_VERIFIED_SIGNUP_CASE,
    };
    let owned_url =
        tokio::task::spawn_blocking(move || crate::db::owned_postgres_test::owned_url(owned_case))
            .await
            .context("owned PostgreSQL custody guard task must join")?;
    let Some(database_url) = owned_url else {
        return Ok(());
    };

    let pool = db::run_blocking_db(|| -> Result<_> {
        let pool = db::open_postgres_pool(&database_url).context("open owned PostgreSQL pool")?;
        db::run_migrations(&pool).context("migrate owned PostgreSQL pool")?;
        Ok(pool)
    })?;

    let signed_up = if followup == ReceiptFollowup::VerifiedSignup {
        let candidate = owned_signup_fixture::signup_candidate(
            &pool,
            &database_url,
            JOURNEY_JWT_SECRET,
            NEW_CANDIDATE_EMAIL,
        )
        .await?;
        jobs::integration_test_runner_control_plane::bind_verified_signup_candidate(
            &pool,
            &candidate.account_id,
        )?;
        Some(candidate)
    } else {
        None
    };
    let (account_id, email) = if let Some(candidate) = signed_up.as_ref() {
        (
            candidate.account_id.clone(),
            NEW_CANDIDATE_EMAIL.to_string(),
        )
    } else {
        let account_id = format!("ui_fixture_{}", uuid::Uuid::new_v4());
        let email = format!("{account_id}@example.test");
        (account_id, email)
    };
    let trial_seconds: i64 = if followup.needs_interview_prep() {
        600
    } else {
        0
    };
    if signed_up.is_none() {
        db::run_blocking_db(|| -> Result<()> {
            pool.get_pg()?
                .execute(
                    "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining, email_verified_at) \
                     VALUES ($1, $2, 'fixture-only', $3, now())",
                    &[&account_id, &email, &trial_seconds],
                )
                .context("seed only the verified synthetic fixture account")?;
            Ok(())
        })?;
    }

    // The real execution claim requires durable public-beta admission. Use
    // the same audited cohort and public enrollment APIs as the normal routers.
    let beta_admin_id = db::run_blocking_db(|| -> Result<String> {
        let admin_email = format!(
            "managed-request-start-beta-admin-{}@example.test",
            uuid::Uuid::new_v4()
        );
        let admin =
            db::accounts::Account::create_with_admin(&pool, &admin_email, "fixture-only", true)?;
        anyhow::ensure!(
            db::accounts::Account::mark_email_verified(&pool, &admin.id)? == 1,
            "verify the synthetic beta administrator"
        );
        let admin = db::accounts::Account::fetch_by_id(&pool, &admin.id)?
            .context("reload the synthetic beta administrator")?;
        anyhow::ensure!(
            admin.is_admin && !admin.is_temporary && admin.email_verified_at.is_some(),
            "beta administration requires the normal verified synthetic administrator"
        );
        let cohort = jobs_beta_access::get_public_beta_cohort(&pool)?;
        anyhow::ensure!(
            cohort.state == PublicBetaCohortState::Draft
                && cohort.hard_cap == 0
                && cohort.assigned_count == 0
                && cohort.opens_at_ms.is_none()
                && cohort.closes_at_ms.is_none(),
            "owned beta fixture must start with the canonical unassigned draft"
        );
        let now_ms = jobs::now_ms();
        let opens_at_ms = now_ms
            .checked_sub(60_000)
            .context("beta fixture window start")?;
        let closes_at_ms = now_ms
            .checked_add(24 * 60 * 60 * 1_000)
            .context("beta fixture window end")?;
        let opened = jobs_beta_access::update_public_beta_cohort_audited(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: cohort.revision,
                state: PublicBetaCohortState::Open,
                opens_at_ms: Some(opens_at_ms),
                closes_at_ms: Some(closes_at_ms),
                hard_cap: 1,
            },
            &admin.id,
        )?;
        anyhow::ensure!(
            opened.state == PublicBetaCohortState::Open
                && opened.opens_at_ms == Some(opens_at_ms)
                && opened.closes_at_ms == Some(closes_at_ms)
                && opened.hard_cap == 1
                && opened.assigned_count == 0
                && opened.revision > cohort.revision,
            "audited beta cohort opening must preserve the requested bounded authority"
        );
        let decision = jobs_beta_access::evaluate_or_enroll_public_beta(&pool, &account_id)?;
        anyhow::ensure!(
            decision.reason == PublicBetaAccessReason::Admitted,
            "the exact managed claim account must be admitted by normal public enrollment"
        );
        let snapshot = jobs_beta_access::public_beta_aggregate_snapshot(&pool)?;
        anyhow::ensure!(
            snapshot.state == PublicBetaCohortState::Open
                && snapshot.hard_cap == 1
                && snapshot.assigned_count == 1
                && snapshot.live_enrollment_count == 1
                && snapshot.public_enrollment_count == 1
                && snapshot.admin_enrollment_count == 0
                && snapshot.active_denial_count == 0,
            "beta admission must consume exactly one public slot without an override"
        );
        Ok(admin.id)
    })?;

    // Retain valid QA evidence before ATS surface/packet/signed authority freeze.
    let source_resume_bytes =
        jobs::integration_test_runner_control_plane::synthetic_receipt_journey_pdf();
    let confirmation_png = journey_png()?;
    let source_resume_sha256 = hex::encode(Sha256::digest(&source_resume_bytes));
    let confirmation_sha256 = hex::encode(Sha256::digest(&confirmation_png));
    let source_resume = if let Some(candidate) = signed_up.as_ref() {
        upload_verified_candidate_resume(
            &pool,
            &database_url,
            candidate,
            &source_resume_bytes,
            &source_resume_sha256,
            SignupResumeFormat::SyntheticPdf,
        )
        .await?
    } else {
        ResumeSourceAsset {
            id: format!("source-resume-{}", uuid::Uuid::new_v4()),
            file_name: format!("resume-{source_resume_sha256}.pdf"),
            media_type: "application/pdf".to_string(),
            file_type: "pdf".to_string(),
            storage_key: format!("accounts/{account_id}/jobs/resumes/synthetic-qa.pdf"),
            sha256: source_resume_sha256.clone(),
            size_bytes: source_resume_bytes.len() as i64,
            page_count: Some(1),
            template_status: "converted_layout".to_string(),
            created_at_ms: jobs::now_ms(),
            updated_at_ms: jobs::now_ms(),
        }
    };
    let track = db::run_blocking_db(|| -> Result<_> {
        let mut profile = if signed_up.is_some() {
            jobs::get_profile(&pool, &account_id, &email)?
        } else {
            jobs::default_profile(&email)
        };
        profile.onboarding_complete = true;
        profile.skills = vec!["Rust".into(), "TypeScript".into(), "PostgreSQL".into()];
        profile.source_resume_name = source_resume.file_name.clone();
        profile.source_resume_asset_id = source_resume.id.clone();
        profile.source_resume_sha256 = source_resume.sha256.clone();
        profile.source_resume_media_type = source_resume.media_type.clone();
        profile.source_resume_template_status = source_resume.template_status.clone();
        let profile = if signed_up.is_some() {
            profile
        } else {
            jobs::save_resume_source_asset(&pool, &account_id, &source_resume, &profile)?.1
        };
        let _profile = jobs::save_profile(&pool, &account_id, &profile)?;
        let identity = jobs::ensure_primary_application_identity(&pool, &account_id, &email)?;
        let _preferences = jobs::save_preferences(
            &pool,
            &account_id,
            &JobPreferences {
                sponsorship: "not_required".to_string(),
                ..JobPreferences::default()
            },
        )?;
        jobs::upsert_track(
            &pool,
            &account_id,
            &CareerTrack {
                id: "managed-request-start-track".to_string(),
                name: "Managed request-start".to_string(),
                role: "Software Engineer".to_string(),
                locations: vec!["New York, NY".to_string()],
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: Some(identity.id),
                policy: CareerTrackPolicy {
                    role_family: "software_engineering".to_string(),
                    ..CareerTrackPolicy::default()
                },
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
    })?;
    anyhow::ensure!(track.policy.authority.review_state == "approved");

    let gateway = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/workflow-commands"))
        .and(header("Authorization", format!("Bearer {GATEWAY_TOKEN}")))
        // Both cases stop polling after one real POST. The baseline deliberately
        // returns a malformed 202; the successor echoes the complete accepted
        // managed-v3 receipt required by the production dispatcher.
        .respond_with(move |request: &wiremock::Request| {
            std::env::set_var(DISPATCH_FLAG, "0");
            if case == ManagedReceiptJourneyCase::DeliveryUnknown {
                return ResponseTemplate::new(202).set_body_string("{}");
            }
            let Ok(outbound) = serde_json::from_slice::<serde_json::Value>(&request.body) else {
                return ResponseTemplate::new(400);
            };
            let (Some(request_id), Some(workflow_id), Some(payload_digest), Some(managed_cloud)) = (
                outbound.get("requestId").cloned(),
                outbound.get("workflowId").cloned(),
                outbound.get("payloadDigest").cloned(),
                outbound.get("managedCloud").cloned(),
            ) else {
                return ResponseTemplate::new(400);
            };
            ResponseTemplate::new(202)
                .insert_header("cache-control", "no-store")
                .insert_header("x-content-type-options", "nosniff")
                .set_body_json(json!({
                    "schemaVersion": 3,
                    "outcome": "accepted",
                    "requestId": request_id,
                    "workflowId": workflow_id,
                    "payloadDigest": payload_digest,
                    "temporalRunId": ACCEPTED_TEMPORAL_RUN_ID,
                    "managedCloud": managed_cloud,
                }))
        })
        .expect(1)
        .mount(&gateway)
        .await;
    let environment = EnvironmentRestore::install(&[
        (DISPATCH_FLAG, Some("1".to_string())),
        (RECONCILIATION_FLAG, Some("0".to_string())),
        ("BLUEY_JOBS_WORKFLOW_ORIGIN", Some(gateway.uri())),
        ("BLUEY_JOBS_WORKFLOW_TOKEN", Some(GATEWAY_TOKEN.to_string())),
        (
            "BLUEY_JOBS_WORKFLOW_COMMAND_POLL_SECONDS",
            Some("300".to_string()),
        ),
        (
            "BLUEY_JOBS_MANAGED_CLOUD_ENVIRONMENT",
            Some("staging".to_string()),
        ),
        (
            "BLUEY_JOBS_MANAGED_CLOUD_REGION",
            Some("us-east-1".to_string()),
        ),
        (
            "BLUEY_JOBS_MANAGED_CLOUD_CHANNEL",
            Some("general".to_string()),
        ),
        (
            "BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED",
            Some("1".to_string()),
        ),
        ("HTTP_PROXY", None),
        ("http_proxy", None),
        ("HTTPS_PROXY", None),
        ("https_proxy", None),
        ("ALL_PROXY", None),
        ("all_proxy", None),
        ("NO_PROXY", None),
        ("no_proxy", None),
    ]);

    let expected_scope = ManagedCloudScope {
        environment: "staging".to_string(),
        region: "us-east-1".to_string(),
        channel: "general".to_string(),
    };
    let (
        bootstrap,
        private_key,
        synthetic_final_submit,
        application_id,
        run_id,
        browser_session_id,
        browser_profile_id,
        command_id,
        request_id,
        workflow_id,
    ) = match db::run_blocking_db(|| -> Result<_> {
        let worker_id = "managed-request-start-worker".to_string();
        let (bootstrap, private_key, synthetic_final_submit) =
            jobs::integration_test_runner_control_plane::bootstrap_auto_submit_with_synthetic_pdf(
                &pool,
                &RunnerControlPlaneBootstrapRequest {
                    version: 1,
                    operation: "bootstrap_auto_submit".to_string(),
                    account_id: account_id.clone(),
                    track_id: track.id.clone(),
                    worker_id,
                },
                [7_u8; 32],
                &source_resume_bytes,
            )?;
        anyhow::ensure!(
            bootstrap.scope == expected_scope,
            "fixture and admission environment scopes must agree"
        );
        jobs::authorize_auto_submit(&pool, &account_id, &email, &track.id)?;
        let prepared = jobs::prepare_application_draft(
            &pool,
            &account_id,
            &bootstrap.job_id,
            "factual",
            "auto_submit",
        )?;
        let (application, _) = jobs::finalize_prepared_application_kit(
            &pool,
            &account_id,
            &prepared,
            prepared.baseline_resume.content.clone(),
            prepared.baseline_resume.diff.clone(),
            String::new(),
            json!({
                "status": "deterministic",
                "provider": "bluey-evidence-planner",
                "claims_added": 0,
            }),
        )?;
        anyhow::ensure!(application.state == "queued");

        let approved = application
            .receipt
            .get("approved_execution")
            .context("auto-submit finalization must freeze approved execution")?;
        let mut packet = approved
            .get("packet")
            .cloned()
            .context("approved execution packet")?;
        if signed_up.is_some() {
            let source = packet
                .pointer("/resumeContent/provenance/career_track_policy_authority")
                .context("approved résumé must retain verified signup's source authority")?;
            anyhow::ensure!(
                source["source_resume_asset_id"] == source_resume.id
                    && source["source_resume_sha256"] == source_resume.sha256,
                "synthetic approval must bind the newly uploaded résumé ID and digest"
            );
        }
        let job = approved
            .get("job")
            .cloned()
            .context("approved execution job")?;
        // Mirror the private normal API constructor
        // api::jobs::attach_approved_execution_transport. The durable approved
        // receipt remains the only source of the three transport fields.
        let approved_checksum = approved
            .get("checksum")
            .and_then(serde_json::Value::as_str)
            .context("approved execution checksum")?;
        let approved_schema_version = approved
            .get("schema_version")
            .and_then(serde_json::Value::as_i64)
            .filter(|version| matches!(*version, 1..=3))
            .context("approved execution schema")?;
        let approved_admission = matches!(approved_schema_version, 2 | 3)
            .then(|| {
                approved
                    .get("admission")
                    .filter(|value| value.is_object())
                    .cloned()
                    .context("approved execution admission")
            })
            .transpose()?;
        let packet_fields = packet
            .as_object_mut()
            .context("approved execution packet object")?;
        packet_fields.insert(
            "approvedPacketChecksum".to_string(),
            json!(approved_checksum),
        );
        packet_fields.insert(
            "approvedExecutionSchemaVersion".to_string(),
            json!(approved_schema_version),
        );
        if let Some(approved_admission) = approved_admission {
            packet_fields.insert("approvedExecutionAdmission".to_string(), approved_admission);
        }
        let application_identity_id = packet
            .get("applicationIdentityId")
            .and_then(serde_json::Value::as_str)
            .context("approved application identity")?;
        let canonical_job_key = application
            .receipt
            .pointer("/job_snapshot/canonical_key")
            .and_then(serde_json::Value::as_str)
            .context("prepared canonical job key")?;
        let browser_profile_id =
            jobs::execution_browser_profile_id(&account_id, application_identity_id);
        let run_id = format!("managed-request-start-run-{}", uuid::Uuid::new_v4());
        let workflow_id = case.workflow_id(&run_id);
        let browser_session_id = format!("cloud-{}", application.id);
        let admission = jobs::stage_cloud_workflow_start(
            &pool,
            &StageCloudWorkflowStart {
                account_id: account_id.clone(),
                application_id: application.id.clone(),
                run_id: run_id.clone(),
                workflow_id: workflow_id.clone(),
                idempotency_key: format!("managed-request-start-key-{}", uuid::Uuid::new_v4()),
                workflow_input: json!({
                    "accountId": account_id,
                    "applicationId": application.id,
                    "applicationIdentityId": application_identity_id,
                    "browserProfileId": browser_profile_id,
                    "canonicalJobKey": canonical_job_key,
                    "idempotencyKey": run_id,
                    "job": job,
                    "jobId": application.job_id,
                    "packet": packet,
                    "packetId": application.resume_version_id,
                    "runner": "cloud",
                    "url": application.receipt["job_snapshot"]["canonical_url"],
                }),
                browser_session: BrowserSession {
                    id: browser_session_id.clone(),
                    runner: "cloud".to_string(),
                    status: "queued".to_string(),
                    current_company: String::new(),
                    current_step: String::new(),
                    application_id: Some(application.id.clone()),
                    takeover_url: None,
                    created_at_ms: jobs::now_ms(),
                    updated_at_ms: jobs::now_ms(),
                },
                now_ms: jobs::now_ms(),
            },
        )?;
        Ok((
            bootstrap,
            private_key,
            synthetic_final_submit,
            application.id,
            run_id,
            browser_session_id,
            browser_profile_id,
            admission.command.id,
            admission.command.request_id,
            workflow_id,
        ))
    }) {
        Ok(staged) => staged,
        Err(primary) => {
            drop(environment);
            if drop_gateway(gateway).is_err() {
                return Err(
                    primary.context("gateway mock verification panicked during setup cleanup")
                );
            }
            return Err(primary);
        }
    };

    let mut dispatcher = None;
    let mut journey_node: Option<JourneyNode> = None;
    let mut journey_api: Option<JoinHandle<std::io::Result<()>>> = None;
    let mut journey_store: Option<MockServer> = None;
    let mut warmup_model: Option<MockServer> = None;
    let mut terminal_trigger_installed = false;
    let outcome = AssertUnwindSafe(async {
        dispatcher = spawn_jobs_workflow_command_dispatcher(pool.clone())?;
        anyhow::ensure!(dispatcher.is_some(), "dispatcher must be enabled");

        let expected_dispatch_state = case.expected_state();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let command = jobs::get_jobs_workflow_command(&pool, &account_id, &command_id)?
                    .context("queued managed command")?;
                if command.state == expected_dispatch_state {
                    anyhow::ensure!(command.first_request_started_at_ms.is_some());
                    anyhow::ensure!(command.attempt_count == 1);
                    match case {
                        ManagedReceiptJourneyCase::DeliveryUnknown => anyhow::ensure!(
                            command.first_ambiguous_at_ms.is_some()
                                && command.temporal_run_id.is_none()
                                && command.accepted_at_ms.is_none(),
                            "baseline dispatcher must retain delivery-unknown authority"
                        ),
                        ManagedReceiptJourneyCase::Accepted => anyhow::ensure!(
                            command.first_ambiguous_at_ms.is_none()
                                && command.temporal_run_id.as_deref()
                                    == Some(ACCEPTED_TEMPORAL_RUN_ID)
                                && command.accepted_at_ms.is_some(),
                            "accepted dispatcher must retain the exact Temporal receipt"
                        ),
                    };
                    break Ok::<(), anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .context("dispatcher must complete its immediate first poll")??;
        let requests = gateway
            .received_requests()
            .await
            .context("request recording disabled")?;
        anyhow::ensure!(requests.len() == 1, "exactly one gateway POST");

        // The responder closed dispatch after its one POST. Observe the
        // committed durable state, then stop the worker before read-only
        // fixture inspection or environment restoration.
        if let Some(handle) = dispatcher.take() {
            abort_dispatcher(handle).await?;
        }

        // The dispatcher is joined: restore normal admission without another poll
        // or any change to the durable request_started/DeliveryUnknown authority.
        std::env::set_var(DISPATCH_FLAG, "1");

        let command = jobs::get_jobs_workflow_command(&pool, &account_id, &command_id)?
            .context("completed managed command")?;
        let request = requests.first().context("recorded gateway POST")?;
        let outbound: serde_json::Value =
            serde_json::from_slice(&request.body).context("parse managed gateway request")?;
        let outbound_object = outbound
            .as_object()
            .context("managed gateway request must be an object")?;
        let outbound_keys: BTreeSet<_> = outbound_object.keys().map(String::as_str).collect();
        let expected_keys = BTreeSet::from([
            "managedCloud",
            "operation",
            "payloadDigest",
            "requestId",
            "schemaVersion",
            "workflowId",
        ]);
        anyhow::ensure!(
            outbound_keys == expected_keys,
            "managed Start request key set must be exact"
        );
        anyhow::ensure!(
            outbound.get("schemaVersion") == Some(&json!(3))
                && outbound.get("operation") == Some(&json!("start"))
                && outbound.get("requestId") == Some(&json!(command.request_id))
                && outbound.get("workflowId") == Some(&json!(command.workflow_id))
                && outbound.get("payloadDigest") == Some(&json!(command.payload_hmac_sha256)),
            "managed Start request identity must match the durable command"
        );
        let outbound_authority: ManagedCloudGatewayAuthority = serde_json::from_value(
            outbound
                .get("managedCloud")
                .cloned()
                .context("managed Start authority")?,
        )
        .context("managed Start authority schema")?;
        let managed_runtime = bootstrap
            .runtimes
            .iter()
            .find(|entry| entry.instance.role == "managed_runner")
            .context("fixture managed runtime")?;
        anyhow::ensure!(
            outbound_authority.version == 1
                && outbound_authority.execution.admission.scope == bootstrap.scope
                && outbound_authority.execution.admission.scope == expected_scope
                && outbound_authority.execution.admission.activation_sha256
                    == managed_runtime.instance.activation_sha256
                && outbound_authority.execution.admission.manifest_sha256
                    == managed_runtime.instance.manifest_sha256,
            "managed Start authority must bind the fixture scope and managed runtime"
        );
        let stored_authority =
            exact_stored_request_start_gateway_authority(&pool, &account_id, &command.id)?;
        require_exact_gateway_authority(&outbound_authority, &stored_authority)?;
        let mut authorization_only_control = stored_authority.clone();
        authorization_only_control.authorization.recovery_accepted =
            !authorization_only_control.authorization.recovery_accepted;
        anyhow::ensure!(
            require_exact_gateway_authority(&outbound_authority, &authorization_only_control)
                .is_err(),
            "authorization-only substitution must fail the full wire authority comparison"
        );
        let post_queue_request = RunnerControlPlanePostQueueRequest {
            version: 1,
            operation: "post_queue_auto_submit".to_string(),
            account_id: account_id.clone(),
            application_id: application_id.clone(),
            run_id: run_id.clone(),
            browser_session_id: browser_session_id.clone(),
            browser_profile_id: browser_profile_id.clone(),
            command_id: command.id,
            request_id: request_id.clone(),
            workflow_id: workflow_id.clone(),
        };
        let handoff = db::run_blocking_db(|| match case {
            ManagedReceiptJourneyCase::DeliveryUnknown => {
                jobs::integration_test_runner_control_plane::post_queue_auto_submit(
                    &pool,
                    &post_queue_request,
                    &bootstrap,
                    &private_key,
                )
            }
            ManagedReceiptJourneyCase::Accepted => {
                jobs::integration_test_runner_control_plane::post_queue_auto_submit_accepted(
                    &pool,
                    &post_queue_request,
                    &bootstrap,
                    &private_key,
                    ACCEPTED_TEMPORAL_RUN_ID,
                )
            }
        })?;
        anyhow::ensure!(
            handoff.submission_mode == "auto_submit"
                && outbound_authority.execution == handoff.managed_cloud_release.execution
                && handoff.managed_cloud_release.execution.admission.scope == bootstrap.scope,
            "outbound managed authority must equal the stored command request-start memo"
        );
        let accepted_attempt_after_handoff = journey_case_accepted_attempt_snapshot(
            &pool,
            case,
            &account_id,
            &command_id,
        )?;

        // Exercise the public Jobs router with only durable fixture authority.
        // No claim/start/lease/protocol row is manufactured here.
        let managed_runtime = bootstrap
            .runtimes
            .iter()
            .find(|entry| entry.instance.role == "managed_runner")
            .context("managed runtime for ordinary claim")?;
        let volume = &bootstrap.volume;
        let worker_id = &private_key.worker_id;
        anyhow::ensure!(
            bootstrap.account_id == account_id
                && private_key.volume == *volume
                && managed_runtime.instance.worker_id == *worker_id
                && handoff.account_id == account_id
                && handoff.application_id == application_id
                && handoff.run_id == run_id
                && handoff.browser_profile_id == browser_profile_id
                && handoff.command_id == command_id
                && handoff.request_id == request_id,
            "claim fixture, private key, managed runtime and post-queue identities"
        );
        let seed = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&private_key.signing_seed_base64url)
            .context("decode fixture volume signing seed")?;
        let signing_key = Ed25519SigningKey::from_bytes(
            &seed
                .try_into()
                .map_err(|_| anyhow::anyhow!("fixture seed length"))?,
        );
        let release_sha256 =
            jobs::managed_cloud_release_memo_sha256(&handoff.managed_cloud_release)
                .context("managed release memo checksum")?;
        let payload_sha256 = runner_volume_execution_lease_claim_payload_sha256(
            &RunnerVolumeExecutionLeaseClaimPayload {
                worker_id,
                account_id: &account_id,
                application_id: &application_id,
                run_id: &run_id,
                browser_profile_id: &browser_profile_id,
                owner_id: worker_id,
                volume_id: &volume.volume_id,
                enrollment_epoch: volume.enrollment_epoch,
                process_instance_id: &volume.process_instance_id,
                runtime_grant_id: &volume.runtime_grant_id,
                runtime_sha256: &volume.runtime_sha256,
                managed_cloud: Some(RunnerVolumeManagedExecutionLeaseClaimPayload {
                    workflow_request_id: &request_id,
                    managed_cloud_release_sha256: &release_sha256,
                    runtime_instance_id: &managed_runtime.instance.runtime_instance_id,
                    runtime_instance_epoch: managed_runtime.instance.instance_epoch,
                }),
            },
        );
        let make_body = |proof_request_id: String, digest: String| {
            let proof = jobs::sign_runner_volume_authority_proof(
                &signing_key,
                jobs::NewRunnerVolumeAuthorityProof {
                    operation: "execution_lease_claim".to_string(),
                    request_id: proof_request_id,
                    volume_id: volume.volume_id.clone(),
                    enrollment_epoch: volume.enrollment_epoch,
                    process_instance_id: volume.process_instance_id.clone(),
                    issued_at_ms: jobs::now_ms(),
                    payload_sha256: digest,
                },
            )?;
            Ok::<_, anyhow::Error>(json!({
                "account_id": account_id, "application_id": application_id,
                "run_id": run_id, "browser_profile_id": browser_profile_id,
                "owner_id": worker_id, "volume_id": volume.volume_id,
                "enrollment_epoch": volume.enrollment_epoch,
                "process_instance_id": volume.process_instance_id,
                "runtime_grant_id": volume.runtime_grant_id,
                "runtime_sha256": volume.runtime_sha256,
                "workflow_request_id": request_id,
                "managed_cloud_release": handoff.managed_cloud_release,
                "managed_cloud_release_sha256": release_sha256,
                "managed_cloud_runtime_instance_id": managed_runtime.instance.runtime_instance_id,
                "managed_cloud_runtime_instance_epoch": managed_runtime.instance.instance_epoch,
                "volume_proof": proof,
            }))
        };
        // Separate loopback object storage: workflow-gateway count stays exactly one.
        let object_store = MockServer::start().await;
        let receipt_observer = Arc::new(ReceiptObserver::default());
        type Objects = std::collections::BTreeMap<String, (String, Vec<u8>)>;
        let stored_objects = std::sync::Arc::new(std::sync::Mutex::new(Objects::new()));
        let prefix = format!("/synthetic-irreversible/phase657-synthetic/accounts/{account_id}/");
        let document_prefix = format!("{prefix}context/jobs/{application_id}/receipts/");
        let bundle_prefix = format!("{prefix}jobs/applications/{}/receipts/",
            hex::encode(Sha256::digest(application_id.as_bytes())));
        let put_objects = std::sync::Arc::clone(&stored_objects);
        let put_observer = Arc::clone(&receipt_observer);
        Mock::given(method("PUT")).respond_with(move |request: &wiremock::Request| {
            ReceiptObserver::observe(&put_observer.puts);
            if !(request.url.path().starts_with(&document_prefix) || request.url.path().starts_with(&bundle_prefix)) || request.body.is_empty()
                || request.body.len() > 1024 * 1024 {
                return ResponseTemplate::new(400);
            }
            let content_type = request.headers.get("content-type")
                .and_then(|value| value.to_str().ok()).unwrap_or("").to_string();
            if !matches!(content_type.as_str(), "application/pdf" | "image/png" | "application/json") {
                return ResponseTemplate::new(400);
            }
            let Ok(mut objects) = put_objects.lock() else { return ResponseTemplate::new(500); };
            if objects.contains_key(request.url.path()) || objects.len() >= 3 {
                return ResponseTemplate::new(409);
            }
            objects.insert(request.url.path().to_string(), (content_type, request.body.clone()));
            ReceiptObserver::observe(&put_observer.accepted_puts);
            ResponseTemplate::new(200)
        }).mount(&object_store).await;
        let get_objects = std::sync::Arc::clone(&stored_objects);
        let get_observer = Arc::clone(&receipt_observer);
        Mock::given(method("GET")).respond_with(move |request: &wiremock::Request| {
            ReceiptObserver::observe(&get_observer.gets);
            let Ok(objects) = get_objects.lock() else { return ResponseTemplate::new(500); };
            match objects.get(request.url.path()) {
                Some((media_type, bytes)) => {
                    ReceiptObserver::observe(&get_observer.accepted_gets);
                    ResponseTemplate::new(200).set_body_raw(bytes.clone(), media_type.as_str())
                }
                None => ResponseTemplate::new(404),
            }
        }).mount(&object_store).await;
        let object_store_origin = object_store.uri();
        journey_store = Some(object_store);
        if followup.needs_interview_prep() {
            let provider = MockServer::start().await;
            anyhow::ensure!(provider.address().ip().is_loopback(), "warmup model must be loopback");
            warmup_model = Some(provider);
        }
        let config = crate::config::Config {
            port: 0,
            db_path: std::path::PathBuf::from(":memory:"),
            db_backend: crate::config::ServerDbBackend::Postgres,
            database_url: Some(database_url.clone()),
            jwt_secret: "managed-claim-test-secret-at-least-32-bytes".to_string(),
            public_url: "http://localhost".to_string(),
            stripe_secret_key: None,
            stripe_webhook_secret: None,
            upstream: crate::config::UpstreamKeys {
                openai_api_key: warmup_model.as_ref().map(|_| "synthetic-warmup-provider".to_string()),
                ..Default::default()
            },
            // Only the private loopback warmup mock receives finite provider
            // authority. Every non-warmup receipt journey remains fail-closed.
            upstream_spend_guard: followup.needs_interview_prep().then_some(
                crate::config::UpstreamSpendGuard {
                    limit_cents: 100,
                    window_hours: 24,
                },
            ),
            smtp: None,
            admin_emails: vec![],
            trial_abuse: crate::config::TrialAbuseConfig::default(),
            turnstile_site_key: None,
            turnstile_secret_key: None,
            require_turnstile: false,
            object_storage: Some(crate::config::ObjectStorageConfig {
                endpoint_url: object_store_origin,
                bucket: "synthetic-irreversible".to_string(),
                access_key_id: "fixture-only".to_string(),
                secret_access_key: "fixture-only".to_string(),
                region: "auto".to_string(),
                key_prefix: "phase657-synthetic".to_string(),
                retention_days: 1,
                max_object_bytes: 64 * 1024 * 1024,
            }),
            log_storage: None,
        };
        const CLAIM_WORKER_KEY: &str = "managed-claim-worker-signing-key-at-least-32-bytes";
        let environment = EnvironmentRestore::install(&[
            (
                "BLUEY_JOBS_WORKER_SIGNING_KEY",
                Some(CLAIM_WORKER_KEY.to_string()),
            ),
            ("BLUEY_JOBS_WORKER_SIGNING_KEY_PREVIOUS", None),
            ("BLUEY_UPLOAD_ACCOUNT_QUOTA_BYTES", Some("268435456".to_string())),
            ("BLUEY_UPLOAD_DAILY_QUOTA_BYTES", Some("268435456".to_string())),
            ("BLUEY_UPLOAD_ACCOUNT_MAX_OBJECTS", Some("128".to_string())),
        ]);
        let _warmup_environment = warmup_model.as_ref().map(|provider| {
            EnvironmentRestore::install(&[
                ("BLUEY_TEST_OPENAI_URL", Some(provider.uri())),
                ("BLUEY_ROUTE_POLICY", Some("quality_first".to_string())),
                ("BLUEY_ANSWER_PLAN_ROUTING", Some("0".to_string())),
            ])
        });
        #[cfg(feature = "jobs-submission-observation")]
        let submission_observation_storage = config
            .object_storage
            .clone()
            .context("synthetic receipt object storage for submission observation")?;
        let router = crate::api::build_jobs_router(pool.clone(), config);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let jobs_address = listener.local_addr()?;
        let origin = format!("http://{jobs_address}");
        let receipt_path = format!("/api/jobs/internal/applications/{application_id}/receipt");
        let status_observer = Arc::clone(&receipt_observer);
        let served_router = router.clone().layer(middleware::from_fn(
            move |request: Request<Body>, next: Next| {
                let observe = request.method() == Method::POST
                    && request.uri().path() == receipt_path.as_str();
                let observer = Arc::clone(&status_observer);
                async move {
                    let response = next.run(request).await;
                    if observe {
                        observer.observe_status(response.status().as_u16());
                    }
                    response
                }
            },
        ));
        journey_api = Some(tokio::spawn(async move { axum::serve(listener, served_router).await }));
        let tampered_proof_id = format!("managed-claim-tampered-{}", uuid::Uuid::new_v4());
        let claim_proof_id = format!("managed-claim-{}", uuid::Uuid::new_v4());
        let protected_state = || -> Result<_> {
            db::run_blocking_db(|| -> Result<(i64, i64, i64, i64, String, String)> {
                let mut connection = pool.get_pg()?;
                let row = connection.query_one(
                    "SELECT
                       (SELECT count(*) FROM jobs_execution_leases WHERE run_id=$1),
                       (SELECT count(*) FROM jobs_execution_lease_volume_bindings WHERE run_id=$1),
                       (SELECT count(*) FROM jobs_execution_lease_process_runtime_bindings
                         WHERE run_id=$1),
                       (SELECT count(*) FROM jobs_runner_volume_authority_uses
                         WHERE volume_id=$2 AND enrollment_epoch=$3
                           AND request_id IN ($6,$7) AND operation='execution_lease_claim'),
                       state,application_json
                     FROM jobs_applications WHERE id=$4 AND account_id=$5",
                    &[
                        &run_id,
                        &volume.volume_id,
                        &volume.enrollment_epoch,
                        &application_id,
                        &account_id,
                        &tampered_proof_id,
                        &claim_proof_id,
                    ],
                )?;
                Ok((
                    row.get(0),
                    row.get(1),
                    row.get(2),
                    row.get(3),
                    row.get(4),
                    row.get(5),
                ))
            })
        };
        let before = protected_state()?;
        anyhow::ensure!(
            (before.0, before.1, before.2, before.3) == (0, 0, 0, 0) && before.4 == "queued",
            "ordinary request-start must leave no preclaim or consumed claim proofs"
        );
        let mut tampered = make_body(tampered_proof_id.clone(), payload_sha256.clone())?;
        tampered["managed_cloud_release_sha256"] = json!("0".repeat(64));
        let worker_timestamp = u64::try_from(chrono::Utc::now().timestamp())
            .context("worker timestamp must be Unix seconds")?;
        let tampered_response = router
            .clone()
            .oneshot(signed_journey_worker_request(
                "/api/jobs/internal/execution-leases/claim",
                "execution",
                &tampered,
                worker_id,
                worker_timestamp,
                &format!("nonce-{}", uuid::Uuid::new_v4()),
                CLAIM_WORKER_KEY,
            ))
            .await?;
        anyhow::ensure!(
            tampered_response.status() == StatusCode::BAD_REQUEST,
            "tampered managed release digest must reach managed-authority validation"
        );
        let tampered_bytes = to_bytes(tampered_response.into_body(), 4096).await?;
        anyhow::ensure!(
            tampered_bytes.as_ref() == b"Invalid managed-cloud execution authority.",
            "tamper must fail the managed-authority checksum guard, not another 400 guard"
        );
        let after_tampered = protected_state()?;
        anyhow::ensure!(
            before == after_tampered,
            "tampered claim changed protected execution state"
        );
        // The normal managed workflow calls recordState("running") before the
        // runner request. Exercise the same scoped HMAC API; never manufacture
        // application, attempt, or browser-session state in the database.
        let state_path = format!("/api/jobs/internal/applications/{application_id}/state");
        let state_response = router
            .clone()
            .oneshot(signed_journey_worker_request(
                &state_path,
                "application-state",
                &json!({"account_id": account_id, "state": "running"}),
                worker_id,
                worker_timestamp,
                &format!("nonce-{}", uuid::Uuid::new_v4()),
                CLAIM_WORKER_KEY,
            ))
            .await?;
        anyhow::ensure!(state_response.status() == StatusCode::OK,
            "normal authenticated workflow Running transition must succeed");
        let state_body: serde_json::Value =
            serde_json::from_slice(&to_bytes(state_response.into_body(), 512 * 1024).await?)?;
        let running = protected_state()?;
        anyhow::ensure!(state_body["id"] == application_id && state_body["state"] == "running"
            && (running.0, running.1, running.2, running.3, running.4.as_str())
                == (0, 0, 0, 0, "running"),
            "workflow Running transition cannot preclaim or consume claim proofs");
        db::run_blocking_db(|| -> Result<()> {
            let mut connection = pool.get_pg()?;
            let attempt = connection.query_one(
                "SELECT status FROM jobs_attempt_reservations WHERE account_id=$1 AND application_id=$2",
                &[&account_id, &application_id])?;
            anyhow::ensure!(attempt.try_get::<_, String>(0)? == "running",
                "normal workflow state must mark the reserved attempt running");
            Ok(())
        })?;
        let claim_body = make_body(claim_proof_id.clone(), payload_sha256.clone())?;
        let command = jobs::get_jobs_workflow_command(&pool, &account_id, &command_id)?
            .context("retained normal Start for connected result")?;
        let material = match &command.envelope.payload {
            jobs::JobsWorkflowCommandPayload::Start(material) => material,
            _ => anyhow::bail!("connected journey requires the immutable Start material"),
        };
        anyhow::ensure!(material.browser_session_id == browser_session_id
            && browser_session_id == format!("cloud-{application_id}") && browser_session_id != run_id,
            "connected journey must not create a run-id browser-session alias");
        let packet = material.workflow_input.get("packet").cloned().context("frozen Start packet")?;
        let job = material.workflow_input.get("job").cloned().context("frozen Start job")?;
        let application_before_claim = jobs::get_application(&pool, &account_id, &application_id)?
            .context("connected application before claim")?;
        let final_submit_proof_input = db::run_blocking_db(|| {
            synthetic_final_submit.proof_for_application(&pool, &account_id, &application_before_claim)
        })?;
        let identity = packet.get("applicationIdentityId").and_then(serde_json::Value::as_str)
            .context("frozen connected identity")?;
        let resume_version = packet.get("resumeVersionId").and_then(serde_json::Value::as_str)
            .context("frozen connected resume version")?;
        let resume_key = format!("qa/{application_id}/resume.pdf");
        let screenshot_key = format!("qa/{application_id}/confirmation.png");
        let generated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let receipt_id = format!("synthetic-qa-receipt-{}", uuid::Uuid::new_v4());
        let confirmation_url = format!("{}/confirmation",
            final_submit_proof_input.job.approved_canonical_url.trim_end_matches('/'));
        let receipt_input = json!({
            "receiptId": receipt_id, "accountId": account_id, "runId": run_id, "runner": "cloud",
            "applicationIdentityId": identity, "browserProfileId": browser_profile_id,
            "adapter": final_submit_proof_input.adapter,
            "adapterVersion": final_submit_proof_input.adapter_version,
            "job": job, "packet": packet,
            "documents": [{"kind":"resume", "versionId":resume_version, "storageKey":resume_key,
                "sha256":source_resume_sha256, "fileName":source_resume.file_name, "mediaType":"application/pdf"}],
            "events": [{"id":"synthetic-qa-only", "occurredAt":generated_at, "type":"greenhouse_state_transition",
                "detail":{"state":"receipt","status":"submitted","capability":"beta_review",
                    "synthetic":true,"browserExecuted":false,"providerExecuted":false}}],
            "result":{"status":"submitted","submittedAt":generated_at,
                "confirmationText":"SYNTHETIC QA ONLY: application received. Component fixture; no employer action.",
                "confirmationUrl":confirmation_url,
                "submitHttpStatus":200,"issues":[]},
            "generatedAt":generated_at, "finalUrl":confirmation_url,
            "screenshotKeys":[screenshot_key],
        });
        anyhow::ensure!(
            jobs::lookup_runner_account_purge_subject(&pool, &account_id)?.is_none(),
            "fresh connected fixture must not have a purge subject before normal preparation"
        );
        let purge_subject = jobs::ensure_runner_account_purge_subject(
            &pool,
            &account_id,
            false,
            jobs::now_ms(),
        )?;
        let purge_subject_readback =
            jobs::lookup_runner_account_purge_subject(&pool, &account_id)?
                .context("normally prepared fixture purge subject")?;
        let purge_subject_replay = jobs::ensure_runner_account_purge_subject(
            &pool,
            &account_id,
            false,
            jobs::now_ms(),
        )?;
        let mut expected_purge_subject_readback = purge_subject.clone();
        expected_purge_subject_readback.disposition = jobs::RunnerVolumeWriteDisposition::Replay;
        anyhow::ensure!(
            purge_subject.account_id == account_id
                && !purge_subject.legacy_unresolved
                && purge_subject.disposition == jobs::RunnerVolumeWriteDisposition::Applied
                && purge_subject_readback == expected_purge_subject_readback
                && purge_subject_replay == expected_purge_subject_readback,
            "normal guarded fixture purge subject preparation, lookup and replay must match"
        );
        let subject_sha256 = jobs::runner_purge_subject_sha256(&purge_subject.purge_subject)?;
        let profile_scope = hex::encode(Sha256::digest(format!("{account_id}\0{identity}").as_bytes()))[..40].to_string();
        let nonce = format!("journey-{}", uuid::Uuid::new_v4());
        let mut initial = json!({
            "schema":"phase658_managed_receipt_journey_node_input_v1", "nonce":nonce, "origin":origin,
            "worker":{"ownerId":worker_id,"signingKey":CLAIM_WORKER_KEY},
            "claim":{"accountId":account_id,"applicationId":application_id,"runId":run_id,
                "browserProfileId":browser_profile_id,"workflowRequestId":request_id,
                "managedCloudRelease":handoff.managed_cloud_release},
            "runnerVolume":{"volumeId":volume.volume_id,"enrollmentEpoch":volume.enrollment_epoch,
                "processInstanceId":volume.process_instance_id,"keyFingerprint":volume.key_fingerprint,
                "runtimeGrantId":volume.runtime_grant_id,"runtimeSha256":volume.runtime_sha256,
                "expectedClaimBinding":{"accountId":account_id,"applicationId":application_id,
                    "runId":run_id,"browserProfileId":browser_profile_id,"ownerId":worker_id,
                    "workflowRequestId":request_id,"managedCloudRelease":handoff.managed_cloud_release,
                    "managedCloudReleaseSha256":release_sha256,
                    "managedCloudRuntimeInstanceId":managed_runtime.instance.runtime_instance_id,
                    "managedCloudRuntimeInstanceEpoch":managed_runtime.instance.instance_epoch},
                "volumeProof":claim_body["volume_proof"]},
            "managedRuntime":managed_runtime.instance,"finalSubmitProof":final_submit_proof_input,
            "result":{"context":{"requestId":material.result_request_id,"profileScope":profile_scope},
                "subjectSha256":subject_sha256,"browserSessionId":browser_session_id,
                "encryptionKeyBase64":base64::engine::general_purpose::STANDARD.encode([37_u8;32]),
                "receiptInput":receipt_input,
                "evidenceObjects":[
                    {"original_key":resume_key,"kind":"resume","media_type":"application/pdf",
                     "sha256":source_resume_sha256,"bytes_base64":base64::engine::general_purpose::STANDARD.encode(&source_resume_bytes)},
                    {"original_key":screenshot_key,"kind":"screenshot","media_type":"image/png",
                     "sha256":confirmation_sha256,"bytes_base64":base64::engine::general_purpose::STANDARD.encode(&confirmation_png)}
                ]},
            "receipt":{"applicationId":application_id,"deferTerminalUpdate":defer_terminal_update},
        });
        if use_workflow_activity {
            initial["receipt"]["useWorkflowActivity"] = json!(true);
        }
        journey_node = Some(JourneyNode::spawn(&initial, nonce).await?);
        drop(initial);
        drop(claim_body);
        let node = journey_node.as_mut().context("owned private driver")?;
        let response = node.phase("claimed", &["leaseTokenSha256", "tokenCanonical32Bytes",
            "fence", "leaseExpiresAtMs", "claimResponseProjectionSha256"]).await?;
        anyhow::ensure!(response["tokenCanonical32Bytes"] == true,
            "claim token must be normal canonical 32-byte base64url");
        let lease_token_sha256 = journey_digest(&response, "leaseTokenSha256")?;
        let claim_response_sha256 = journey_digest(&response, "claimResponseProjectionSha256")?;
        let claim_fence = response["fence"].as_i64().filter(|value| *value == 1).context("normal claim fence")?;
        let claim_expiry = response["leaseExpiresAtMs"].as_i64().context("normal claim expiry")?;
        let mut claim_gateway_readback = None;
        db::run_blocking_db(|| -> Result<()> {
            let mut connection = pool.get_pg()?;
            let mut tx = connection
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead)
                .read_only(true)
                .start()?;
            let lease = tx.query_one(
                "SELECT * FROM jobs_execution_leases
                  WHERE account_id=$1 AND application_id=$2 AND run_id=$3",
                &[&account_id, &application_id, &run_id],
            )?;
            let fence: i64 = lease.try_get("fence")?;
            let expires_at_ms: i64 = lease.try_get("lease_expires_at_ms")?;
            let created_at_ms: i64 = lease.try_get("created_at_ms")?;
            let db_now_ms: i64 = tx
                .query_one(
                    "SELECT (extract(epoch FROM clock_timestamp()) * 1000)::bigint",
                    &[],
                )?
                .get(0);
            anyhow::ensure!(
                lease.try_get::<_, String>("account_id")? == account_id
                    && lease.try_get::<_, String>("application_id")? == application_id
                    && lease.try_get::<_, String>("run_id")? == run_id
                    && lease.try_get::<_, String>("browser_profile_id")? == browser_profile_id
                    && lease.try_get::<_, String>("owner_id")? == *worker_id
                    && lease.try_get::<_, String>("lease_token_sha256")? == lease_token_sha256
                    && fence == claim_fence && expires_at_ms == claim_expiry
                    && lease.try_get::<_, String>("phase")? == "prepared"
                    && expires_at_ms > db_now_ms
                    && expires_at_ms > created_at_ms
                    && lease.try_get::<_, Option<i64>>("finished_at_ms")?.is_none(),
                "durable first prepared lease identity, owner, token hash, fence and expiry"
            );
            let binding = tx.query_one(
                "SELECT * FROM jobs_execution_lease_volume_bindings WHERE run_id=$1",
                &[&run_id],
            )?;
            let runtime = tx.query_one(
                "SELECT * FROM jobs_execution_lease_process_runtime_bindings
                  WHERE run_id=$1 AND fence=$2",
                &[&run_id, &fence],
            )?;
            let subject: String = tx
                .query_one(
                    "SELECT purge_subject FROM jobs_runner_account_subjects WHERE account_id=$1",
                    &[&account_id],
                )?
                .get(0);
            anyhow::ensure!(
                subject == purge_subject.purge_subject,
                "normal claim must preserve the prepared account purge subject"
            );
            let subject_sha256 = jobs::runner_purge_subject_sha256(&subject)?;
            let residency = tx.query_one(
                "SELECT * FROM jobs_runner_volume_residencies
                  WHERE purge_subject=$1 AND volume_id=$2 AND volume_epoch=$3",
                &[&subject, &volume.volume_id, &volume.enrollment_epoch],
            )?;
            let key = tx.query_one(
                "SELECT public_key_base64url,key_fingerprint,retired_at_ms
                   FROM jobs_runner_volume_keys WHERE volume_id=$1 AND enrollment_epoch=$2",
                &[&volume.volume_id, &volume.enrollment_epoch],
            )?;
            let fingerprint: String = key.try_get("key_fingerprint")?;
            anyhow::ensure!(
                key.try_get::<_, String>("public_key_base64url")?
                    == private_key.public_key_base64url
                    && base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode(signing_key.verifying_key().to_bytes())
                        == private_key.public_key_base64url
                    && fingerprint == volume.key_fingerprint
                    && key.try_get::<_, Option<i64>>("retired_at_ms")?.is_none()
                    && binding.try_get::<_, String>("run_id")? == run_id
                    && binding.try_get::<_, String>("volume_id")? == volume.volume_id
                    && binding.try_get::<_, i64>("volume_epoch")? == volume.enrollment_epoch
                    && binding.try_get::<_, String>("process_instance_id")?
                        == volume.process_instance_id
                    && binding.try_get::<_, String>("purge_subject_sha256")? == subject_sha256
                    && binding.try_get::<_, i64>("bound_at_ms")? == created_at_ms
                    && runtime.try_get::<_, String>("run_id")? == run_id
                    && runtime.try_get::<_, i64>("fence")? == fence
                    && runtime.try_get::<_, String>("worker_id")? == *worker_id
                    && runtime.try_get::<_, String>("volume_id")? == volume.volume_id
                    && runtime.try_get::<_, i64>("enrollment_epoch")? == volume.enrollment_epoch
                    && runtime.try_get::<_, String>("process_instance_id")?
                        == volume.process_instance_id
                    && runtime.try_get::<_, String>("runtime_grant_id")? == volume.runtime_grant_id
                    && runtime.try_get::<_, String>("runtime_sha256")? == volume.runtime_sha256
                    && runtime.try_get::<_, i64>("bound_at_ms")? == created_at_ms
                    && residency.try_get::<_, String>("purge_subject")? == subject
                    && residency.try_get::<_, String>("volume_id")? == volume.volume_id
                    && residency.try_get::<_, i64>("volume_epoch")? == volume.enrollment_epoch
                    && residency.try_get::<_, String>("state")? == "resident"
                    && residency.try_get::<_, i64>("purge_generation")? == 0,
                "actual lease volume/process/runtime/residency rows must bind the fixture"
            );
            let release_base64url: String =
                lease.try_get("managed_cloud_release_memo_base64url")?;
            let release_bytes =
                base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&release_base64url)?;
            anyhow::ensure!(
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&release_bytes)
                    == release_base64url
                    && jobs::parse_managed_cloud_release_memo_bytes(&release_bytes)?
                        == handoff.managed_cloud_release
                    && hex::encode(Sha256::digest(&release_bytes)) == release_sha256,
                "frozen lease memo bytes must equal legitimate post-queue authority"
            );
            let gateway_base64url: String =
                lease.try_get("managed_cloud_gateway_authority_base64url")?;
            let gateway_bytes =
                base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&gateway_base64url)?;
            let gateway: ManagedCloudGatewayAuthority = serde_json::from_slice(&gateway_bytes)?;
            let gateway_sha256 = hex::encode(Sha256::digest(&gateway_bytes));
            anyhow::ensure!(
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&gateway_bytes)
                    == gateway_base64url
                    && gateway.version == 1
                    && gateway.execution == handoff.managed_cloud_release.execution
                    && gateway.execution == outbound_authority.execution
                    && gateway.authorization.authorized_at_ms == created_at_ms
                    && gateway.authorization.current_head_revision
                        == managed_runtime.instance.head_revision
                    && gateway.authorization.current_transition_sha256
                        == managed_runtime.instance.transition_sha256
                    && gateway.authorization.current_activation_sha256
                        == managed_runtime.instance.activation_sha256
                    && gateway.authorization.current_manifest_sha256
                        == managed_runtime.instance.manifest_sha256
                    && gateway.authorization.current_activation_expires_at_ms
                        == managed_runtime.instance.activation_expires_at_ms
                    && gateway.authorization.current_task_queue_sha256
                        == managed_runtime.instance.task_queue_sha256
                    && gateway.authorization.current_failure_converter_sha256
                        == managed_runtime.instance.failure_converter_sha256
                    && !gateway.authorization.recovery_accepted,
                "frozen lease gateway must bind original execution and claim-time authorization"
            );
            // Read all twelve non-null frozen managed columns, then use the
            // existing pure canonical binder to verify the final digest.
            let authority = jobs::ManagedCloudExecutionLeaseAuthority {
                managed_cloud: gateway,
                managed_cloud_workflow_request_id: lease
                    .try_get("managed_cloud_workflow_request_id")?,
                request_command_id: lease.try_get("managed_cloud_request_command_id")?,
                execution_command_id: lease.try_get("managed_cloud_execution_command_id")?,
                binding_sha256: lease.try_get("managed_cloud_binding_sha256")?,
                release_memo_base64url: release_base64url,
                release_sha256: lease.try_get("managed_cloud_release_sha256")?,
                managed_cloud_runtime_instance_id: lease
                    .try_get("managed_cloud_runtime_instance_id")?,
                managed_cloud_runtime_instance_epoch: lease
                    .try_get("managed_cloud_runtime_instance_epoch")?,
                managed_cloud_worker_id: lease.try_get("managed_cloud_worker_id")?,
                gateway_authority_base64url: gateway_base64url,
                gateway_authority_sha256: lease
                    .try_get("managed_cloud_gateway_authority_sha256")?,
            };
            anyhow::ensure!(
                authority.managed_cloud_workflow_request_id == request_id
                    && authority.request_command_id == command_id
                    && authority.execution_command_id == handoff.command_id
                    && authority.binding_sha256
                        == handoff.managed_cloud_release.execution.binding_sha256
                    && authority.release_sha256 == release_sha256
                    && authority.managed_cloud_runtime_instance_id
                        == managed_runtime.instance.runtime_instance_id
                    && authority.managed_cloud_runtime_instance_epoch
                        == managed_runtime.instance.instance_epoch
                    && authority.managed_cloud_worker_id == *worker_id
                    && authority.gateway_authority_sha256 == gateway_sha256,
                "all frozen managed lease identities must bind request-start and runtime authority"
            );
            let bound = jobs::bind_managed_cloud_execution_lease_authority(
                authority,
                &run_id,
                fence,
                &lease_token_sha256,
            )?;
            anyhow::ensure!(
                lease.try_get::<_, String>("managed_cloud_lease_authority_sha256")?
                    == bound.lease_authority_sha256,
                "canonical managed lease authority digest must bind token hash and fence"
            );
            let proof = tx.query_one(
                "SELECT * FROM jobs_runner_volume_authority_uses
                  WHERE volume_id=$1 AND enrollment_epoch=$2 AND request_id=$3",
                &[&volume.volume_id, &volume.enrollment_epoch, &claim_proof_id],
            )?;
            anyhow::ensure!(
                proof.try_get::<_, String>("operation")? == "execution_lease_claim"
                    && proof.try_get::<_, String>("process_instance_id")?
                        == volume.process_instance_id
                    && proof.try_get::<_, String>("payload_sha256")? == payload_sha256
                    && proof.try_get::<_, i64>("consumed_at_ms")? == created_at_ms,
                "successful claim must consume exactly its own canonical signed volume proof"
            );
            let expected = jobs::AuthorizedRunnerVolumeExecutionLeaseGrant {
                grant: jobs::RunnerVolumeExecutionLeaseGrant {
                    lease: jobs::ExecutionLeaseGrant {
                        run_id: run_id.clone(),
                        lease_token: lease_token_sha256.clone(),
                        fence,
                        lease_expires_at_ms: expires_at_ms,
                        phase: "prepared".to_string(),
                    },
                    purge_subject: subject,
                    volume_id: volume.volume_id.clone(),
                    enrollment_epoch: volume.enrollment_epoch,
                    process_instance_id: volume.process_instance_id.clone(),
                    volume_key_fingerprint: fingerprint,
                    runtime_grant_id: volume.runtime_grant_id.clone(),
                    runtime_sha256: volume.runtime_sha256.clone(),
                },
                managed_cloud: Some(bound.authority),
            };
            claim_gateway_readback = expected.managed_cloud.as_ref().map(|authority| authority.managed_cloud.clone());
            anyhow::ensure!(claim_response_sha256 == journey_sha(&serde_json::to_value(expected)?)?,
                "actual claim projection must equal the full durable typed authority");
            tx.commit()?;
            Ok(())
        })?;
        let after_claim = protected_state()?;
        anyhow::ensure!(
            (after_claim.0, after_claim.1, after_claim.2, after_claim.3) == (1, 1, 1, 1)
                && after_claim.4 == "running",
            "one normal claim, no rejected-proof consumption, application remains running"
        );
        // Extend only after every V2 request-start and claim assertion has passed.
        anyhow::ensure!(
            managed_authority_test_canonical_bytes(&json!({"b": [true, "x"], "a": 1}))?
                == b"{\"a\":1,\"b\":[true,\"x\"]}\n",
            "test canonical bytes must match lexical compact JSON plus LF"
        );
        anyhow::ensure!(
            managed_authority_test_canonical_bytes(&json!(1.5)).is_err()
                && managed_authority_test_canonical_bytes(&json!(9_007_199_254_740_992_u64),)
                    .is_err(),
            "test canonical digest must reject floats and unsafe integers"
        );
        let claim_gateway = claim_gateway_readback.context("durable claim gateway")?;
        anyhow::ensure!(
            managed_recovery_test_digest(&claim_gateway.execution, &claim_gateway.authorization)?
                == claim_gateway.authorization.recovery_authorization_sha256,
            "test recovery digest must reproduce the actual normal claim authority"
        );
        let phase_a_request = RunnerControlPlaneInspectAutoSubmitClaimRequest {
            version: 1,
            operation: "inspect_auto_submit_claim".to_string(),
            account_id: account_id.clone(),
            application_id: application_id.clone(),
            run_id: run_id.clone(),
            browser_session_id: browser_session_id.clone(),
            browser_profile_id: browser_profile_id.clone(),
            lease_token_sha256: lease_token_sha256.clone(),
            lease_fence: claim_fence,
        };
        let inspect_phase_a = || -> Result<()> {
            db::run_blocking_db(|| {
                jobs::integration_test_runner_control_plane::inspect_running_auto_submit_claim(
                    &pool,
                    &phase_a_request,
                    &bootstrap,
                    &handoff,
                    &private_key,
                )
                .map(|_| ())
            })
        };
        inspect_phase_a()?;
        let effect_snapshot = || {
            managed_effect_protected_state(
                &pool,
                &account_id,
                &application_id,
                &run_id,
                &volume.volume_id,
                volume.enrollment_epoch,
            )
        };
        let before_effect = effect_snapshot()?;
        anyhow::ensure!(
            before_effect
                .iter()
                .filter(|row| row.starts_with("lease:"))
                .count()
                == 1
                && before_effect
                    .iter()
                    .filter(|row| row.starts_with("ats:"))
                    .count()
                    == 1
                && before_effect
                    .iter()
                    .all(|row| !row.starts_with("irreversible:")),
            "effect baseline must contain one prepared lease/Phase A and no irreversible receipt"
        );
        node.proceed("claimed").await?;
        let denied = node.phase("effect_checksum_rejected", &["status"]).await?;
        anyhow::ensure!(denied["status"] == 400, "negative effect checksum status");
        anyhow::ensure!(
            effect_snapshot()? == before_effect,
            "negative effect changed protected claim/application/Phase A/proof state"
        );
        inspect_phase_a()?;
        let readiness_query = jobs::ManagedCloudReadinessQuery {
            scope: bootstrap.scope.clone(),
            account_id: Some(account_id.clone()),
        };
        let readiness_before = jobs::resolve_managed_cloud_readiness(&pool, &readiness_query)?;
        node.proceed("effect_checksum_rejected").await?;
        let effect_value = node.phase("effect_authorized",
            &["status", "privateNoStore", "nosniff", "authorizedAtMs", "responseSha256"]).await?;
        anyhow::ensure!(effect_value["status"] == 200 && effect_value["privateNoStore"] == true
            && effect_value["nosniff"] == true, "normal private managed effect response");
        let authorized_at = effect_value["authorizedAtMs"].as_i64().context("actual effect authorization time")?;
        let effect_response_sha256 = journey_digest(&effect_value, "responseSha256")?;
        let readiness_after = jobs::resolve_managed_cloud_readiness(&pool, &readiness_query)?;
        // These are stable bracketing observations, not an atomicity proof.
        anyhow::ensure!(
            readiness_before.status == readiness_after.status
                && readiness_before.missing_roles == readiness_after.missing_roles
                && readiness_before.stale_roles == readiness_after.stale_roles
                && readiness_before.cohort_eligible == readiness_after.cohort_eligible
                && readiness_before.evaluated_at_ms <= authorized_at
                && authorized_at <= readiness_after.evaluated_at_ms,
            "effect authority time must lie within stable actual readiness observations"
        );
        anyhow::ensure!(
            managed_readiness_test_digest_at(&readiness_before, readiness_before.evaluated_at_ms)?
                == readiness_before.readiness_sha256
                && managed_readiness_test_digest_at(
                    &readiness_after,
                    readiness_after.evaluated_at_ms,
                )? == readiness_after.readiness_sha256,
            "test readiness digest must reproduce both real public canonical results"
        );
        let expected_gateway = managed_effect_test_gateway(
            &handoff.managed_cloud_release.execution,
            &readiness_before,
            authorized_at,
        )?;

        let fresh_gateway_bytes =
            managed_authority_test_canonical_bytes(&serde_json::to_value(&expected_gateway)?)?;
        db::run_blocking_db(|| -> Result<()> {
            let mut connection = pool.get_pg()?;
            let mut tx = connection
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead)
                .read_only(true)
                .start()?;
            let lease = tx.query_one(
                "SELECT * FROM jobs_execution_leases
                  WHERE account_id=$1 AND application_id=$2 AND run_id=$3",
                &[&account_id, &application_id, &run_id],
            )?;
            anyhow::ensure!(
                lease.try_get::<_, String>("lease_token_sha256")? == lease_token_sha256
                    && lease.try_get::<_, i64>("fence")? == claim_fence
                    && lease.try_get::<_, String>("phase")? == "prepared"
                    && lease.try_get::<_, i64>("lease_expires_at_ms")? == claim_expiry,
                "effect must retain the actual claim token hash/fence/prepared phase/expiry"
            );
            // Hidden execution fields come from the verified immutable claim.
            // Fresh gateway bytes/digest match the real effect constructor; no
            // claim-time gateway/readiness digest is substituted for this response.
            let expected_authority = jobs::ManagedCloudExecutionLeaseAuthority {
                managed_cloud: expected_gateway,
                managed_cloud_workflow_request_id: lease
                    .try_get("managed_cloud_workflow_request_id")?,
                request_command_id: lease.try_get("managed_cloud_request_command_id")?,
                execution_command_id: lease.try_get("managed_cloud_execution_command_id")?,
                binding_sha256: lease.try_get("managed_cloud_binding_sha256")?,
                release_memo_base64url: lease.try_get("managed_cloud_release_memo_base64url")?,
                release_sha256: lease.try_get("managed_cloud_release_sha256")?,
                managed_cloud_runtime_instance_id: lease
                    .try_get("managed_cloud_runtime_instance_id")?,
                managed_cloud_runtime_instance_epoch: lease
                    .try_get("managed_cloud_runtime_instance_epoch")?,
                managed_cloud_worker_id: lease.try_get("managed_cloud_worker_id")?,
                gateway_authority_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(&fresh_gateway_bytes),
                gateway_authority_sha256: hex::encode(Sha256::digest(&fresh_gateway_bytes)),
            };
            let expected_record = jobs::AuthorizedExecutionLeaseRecord {
                lease: jobs::ExecutionLeaseRecord {
                    run_id: run_id.clone(),
                    fence: claim_fence,
                    lease_expires_at_ms: claim_expiry,
                    phase: "prepared".to_string(),
                },
                managed_cloud: Some(expected_authority),
            };
            anyhow::ensure!(
                effect_response_sha256 == journey_sha(&serde_json::to_value(expected_record)?)?,
                "closed effect record must equal the normal serializer and actual authority"
            );
            tx.commit()?;
            Ok(())
        })?;
        anyhow::ensure!(
            effect_snapshot()? == before_effect && protected_state()? == after_claim,
            "effect changed frozen managed columns, domain rows, application or proof uses"
        );
        inspect_phase_a()?;
        // Continue only through controlled synthetic component operations; never a provider/browser.
        let auxiliary_snapshot = || -> Result<Vec<String>> {
            db::run_blocking_db(|| {
                let mut connection = pool.get_pg()?;
                let mut tx = connection.build_transaction()
                    .isolation_level(postgres::IsolationLevel::RepeatableRead)
                    .read_only(true).start()?;
                let rows = tx.query(
                    "SELECT 'capacity:' || row_to_json(capacity)::text
                       FROM jobs_submission_evidence_capacity capacity
                      WHERE account_id=$1 AND application_id=$2 AND run_id=$3
                     ORDER BY 1", &[&account_id, &application_id, &run_id])?
                    .into_iter().map(|row| row.get::<_, String>(0)).collect();
                tx.commit()?;
                Ok(rows)
            })
        };
        let before_irreversible = effect_snapshot()?;
        let before_capacity = auxiliary_snapshot()?;
        anyhow::ensure!(before_capacity.is_empty(), "no evidence capacity before irreversible gate");
        let application_before: jobs::JobApplication = db::run_blocking_db(|| -> Result<_> {
            let mut connection = pool.get_pg()?;
            let row = connection.query_one(
                "SELECT application_json FROM jobs_applications WHERE account_id=$1 AND id=$2",
                &[&account_id, &application_id])?;
            let raw: String = row.get(0);
            let plain = jobs::decrypt_payload_for_test(&raw)
                .context("decrypt pre-irreversible application snapshot")?;
            serde_json::from_str(&plain)
                .context("parse pre-irreversible application snapshot")
        })?;
        let final_submit_proof = db::run_blocking_db(|| {
            synthetic_final_submit.proof_for_application(
                &pool, &account_id, &application_before)
        })?;
        anyhow::ensure!(
            final_submit_proof.documents.len() == 1
                && final_submit_proof.documents[0].sha256 == source_resume_sha256
                && application_before.state == "running"
                && serde_json::to_value(&final_submit_proof)? == serde_json::to_value(&final_submit_proof_input)?,
            "only pre-frozen synthetic PDF facts, never browser/provider evidence");
        node.proceed("effect_authorized").await?;
        for (phase, expected_status) in [
            ("irreversible_checksum_rejected", 400),
            ("irreversible_fence_rejected", 404),
        ] {
            let denied = node.phase(phase, &["status"]).await?;
            anyhow::ensure!(denied["status"] == expected_status,
                "irreversible negative must reach the exact checksum/fence guard");
            anyhow::ensure!(effect_snapshot()? == before_irreversible
                && auxiliary_snapshot()? == before_capacity,
                "pre-Phase-B rejection changed protected durable state");
            inspect_phase_a()?;
            if phase == "irreversible_checksum_rejected" { node.proceed(phase).await?; }
        }
        // The only positive irreversible call uses normal worker HMAC and the real frozen proof.
        let readiness_before_irreversible = jobs::resolve_managed_cloud_readiness(&pool, &readiness_query)?;
        node.proceed("irreversible_fence_rejected").await?;
        let irreversible_value = node.phase("irreversible_authorized",
            &["status", "privateNoStore", "nosniff", "authorizedAtMs",
              "atsCertifiedReceiptAuthority", "responseSha256"]).await?;
        anyhow::ensure!(irreversible_value["status"] == 200
            && irreversible_value["privateNoStore"] == true && irreversible_value["nosniff"] == true,
            "normal irreversible response must be private and non-sniffable");
        let irreversible_at = irreversible_value["authorizedAtMs"].as_i64()
            .context("actual irreversible authorization time")?;
        let irreversible_response_sha256 = journey_digest(&irreversible_value, "responseSha256")?;
        let presented_ats: jobs::AtsCertifiedReceiptAuthority = serde_json::from_value(
            irreversible_value["atsCertifiedReceiptAuthority"].clone())?;
        let readiness_after_irreversible = jobs::resolve_managed_cloud_readiness(&pool, &readiness_query)?;
        // Bracketing stable status observations do not establish transaction atomicity.
        anyhow::ensure!(readiness_before_irreversible.status == readiness_after_irreversible.status
            && readiness_before_irreversible.missing_roles == readiness_after_irreversible.missing_roles
            && readiness_before_irreversible.stale_roles == readiness_after_irreversible.stale_roles
            && readiness_before_irreversible.cohort_eligible == readiness_after_irreversible.cohort_eligible
            && readiness_before_irreversible.evaluated_at_ms <= irreversible_at
            && irreversible_at <= readiness_after_irreversible.evaluated_at_ms,
            "irreversible authority must be bracketed by stable real readiness");
        let expected_irreversible_gateway = managed_effect_test_gateway(
            &handoff.managed_cloud_release.execution, &readiness_before_irreversible, irreversible_at)?;
        let irreversible_input = jobs::ManagedCloudExecutionLeaseClaimInput {
            workflow_request_id: request_id.clone(),
            managed_cloud_release: handoff.managed_cloud_release.clone(),
            managed_cloud_release_sha256: release_sha256.clone(),
            managed_cloud_runtime_instance_id: managed_runtime.instance.runtime_instance_id.clone(),
            managed_cloud_runtime_instance_epoch: managed_runtime.instance.instance_epoch,
        };
        let internal_authority = db::run_blocking_db(|| {
            jobs::integration_test_runner_control_plane::inspect_synthetic_irreversible_receipt(
                &pool, &phase_a_request, &irreversible_input, worker_id)
        })?;
        anyhow::ensure!(internal_authority.managed_cloud == expected_irreversible_gateway,
            "durable internal receipt must validate with the normal canonical/HMAC authority validator");
        db::run_blocking_db(|| -> Result<()> {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead).read_only(true).start()?;
            let lease = tx.query_one(
                "SELECT * FROM jobs_execution_leases WHERE account_id=$1 AND application_id=$2 AND run_id=$3",
                &[&account_id, &application_id, &run_id])?;
            anyhow::ensure!(lease.try_get::<_, String>("lease_token_sha256")? == lease_token_sha256
                && lease.try_get::<_, i64>("fence")? == claim_fence
                && lease.try_get::<_, i64>("lease_expires_at_ms")? == claim_expiry
                && lease.try_get::<_, String>("phase")? == "click_started"
                && lease.try_get::<_, Option<i64>>("finished_at_ms")?.is_none(),
                "durable checkpoint phase begins irreversible authority, not completion");
            anyhow::ensure!(internal_authority.managed_cloud_workflow_request_id
                    == lease.try_get::<_, String>("managed_cloud_workflow_request_id")?
                && internal_authority.request_command_id == lease.try_get::<_, String>("managed_cloud_request_command_id")?
                && internal_authority.execution_command_id == lease.try_get::<_, String>("managed_cloud_execution_command_id")?
                && internal_authority.binding_sha256 == lease.try_get::<_, String>("managed_cloud_binding_sha256")?
                && internal_authority.release_memo_base64url == lease.try_get::<_, String>("managed_cloud_release_memo_base64url")?
                && internal_authority.release_sha256 == lease.try_get::<_, String>("managed_cloud_release_sha256")?
                && internal_authority.managed_cloud_runtime_instance_id == lease.try_get::<_, String>("managed_cloud_runtime_instance_id")?
                && internal_authority.managed_cloud_runtime_instance_epoch == lease.try_get::<_, i64>("managed_cloud_runtime_instance_epoch")?
                && internal_authority.managed_cloud_worker_id == lease.try_get::<_, String>("managed_cloud_worker_id")?,
                "internal receipt must preserve every frozen execution identity");
            let verified_ats = jobs::lookup_ats_certification_terminal_receipt_authority_postgres_tx(
                &mut tx, &account_id, &application_id, &run_id, &presented_ats)?;
            anyhow::ensure!(verified_ats.binding_fence == 1
                && verified_ats.binding_consumed_at_ms >= readiness_before_irreversible.evaluated_at_ms
                && verified_ats.binding_consumed_at_ms <= readiness_after_irreversible.evaluated_at_ms
                && verified_ats.rollout_channel == "general"
                && verified_ats.observed_surface_sha256
                    == final_submit_proof.observed_surface.as_ref().context("constructed surface")?.surface_sha256,
                "real Phase B must consume exactly the frozen constructed surface once");
            let phase_b = tx.query_one(
                "SELECT phase,fence,invalidated_at_ms FROM jobs_application_ats_certification_bindings
                  WHERE account_id=$1 AND application_id=$2 AND run_id=$3",
                &[&account_id, &application_id, &run_id])?;
            anyhow::ensure!(phase_b.get::<_, String>(0) == "consumed"
                && phase_b.get::<_, i64>(1) == 1
                && phase_b.get::<_, Option<i64>>(2).is_none(),
                "Phase B is consumed, never quarantined or side-effect-unknown");
            let receipt = tx.query_one(
                "SELECT committed_at_ms FROM jobs_managed_cloud_irreversible_effect_receipts
                  WHERE account_id=$1 AND application_id=$2 AND run_id=$3 AND fence=$4",
                &[&account_id, &application_id, &run_id, &claim_fence])?;
            anyhow::ensure!(receipt.get::<_, i64>(0) >= readiness_before_irreversible.evaluated_at_ms
                && receipt.get::<_, i64>(0) <= readiness_after_irreversible.evaluated_at_ms,
                "internal receipt is committed inside the observed irreversible gate interval");
            let expected = jobs::AuthorizedIrreversibleExecutionLeaseRecord {
                record: jobs::IrreversibleExecutionLeaseRecord {
                    lease: jobs::ExecutionLeaseRecord {
                        run_id: run_id.clone(), fence: claim_fence,
                        lease_expires_at_ms: claim_expiry, phase: "click_started".to_string(),
                    }, ats_certified_receipt_authority: Some(verified_ats),
                }, managed_cloud: Some(internal_authority),
            };
            anyhow::ensure!(irreversible_response_sha256 == journey_sha(&serde_json::to_value(expected)?)?,
                "closed irreversible response must equal actual durable typed authority");
            let application = tx.query_one(
                "SELECT state,application_json FROM jobs_applications WHERE account_id=$1 AND id=$2",
                &[&account_id, &application_id])?;
            let raw: String = application.get(1);
            let plain = jobs::decrypt_payload_for_test(&raw)
                .context("decrypt post-irreversible application snapshot")?;
            let application_after: jobs::JobApplication = serde_json::from_str(&plain)
                .context("parse post-irreversible application snapshot")?;
            let mut expected_application = application_before.clone();
            expected_application.receipt[jobs::FINAL_SUBMIT_PROOF_KEY] = serde_json::to_value(&final_submit_proof)?;
            anyhow::ensure!(application.get::<_, String>(0) == "running"
                && jobs::stored_final_submit_proof(&application_after)? == final_submit_proof
                && serde_json::to_value(application_after)? == serde_json::to_value(expected_application)?,
                "normal gate binds only the submitted proof; no application completion/runner result/receipt");
            let capacity = tx.query_one(
                "SELECT * FROM jobs_submission_evidence_capacity
                  WHERE account_id=$1 AND application_id=$2 AND run_id=$3",
                &[&account_id, &application_id, &run_id])?;
            anyhow::ensure!(capacity.try_get::<_, String>("runner")? == "cloud"
                && capacity.try_get::<_, i64>("reserved_bytes")? == 48 * 1024 * 1024
                && capacity.try_get::<_, i64>("reserved_objects")? == 13
                && capacity.try_get::<_, i64>("consumed_bytes")? == 0
                && capacity.try_get::<_, i64>("consumed_objects")? == 0
                && capacity.try_get::<_, String>("state")? == "active"
                && capacity.try_get::<_, i64>("expires_at_ms")?
                    == capacity.try_get::<_, i64>("created_at_ms")? + jobs::SUBMISSION_RECONCILIATION_GRACE_MS
                && capacity.try_get::<_, i64>("created_at_ms")? == capacity.try_get::<_, i64>("updated_at_ms")?
                && capacity.try_get::<_, i64>("created_at_ms")? >= readiness_before_irreversible.evaluated_at_ms
                && capacity.try_get::<_, i64>("created_at_ms")? <= readiness_after_irreversible.evaluated_at_ms,
                "gate reserves bounded synthetic capacity but performs no evidence upload");
            tx.commit()?;
            Ok(())
        })?;
        let after_irreversible = effect_snapshot()?;
        // Freeze every lease field except the two intentional gate updates. This
        // includes all twelve managed columns, the claim gateway digest and token hash.
        let stable_rows = |rows: &[String]| -> Result<Vec<String>> {
            rows.iter().filter(|row| !row.starts_with("application:")
                && !row.starts_with("ats:") && !row.starts_with("irreversible:"))
                .map(|row| {
                    if let Some(value) = row.strip_prefix("lease:") {
                        let mut value: serde_json::Value = serde_json::from_str(value)?;
                        let object = value.as_object_mut().context("lease row object")?;
                        object.remove("phase"); object.remove("updated_at_ms");
                        Ok(format!("lease:{}", serde_json::to_string(&value)?))
                    } else { Ok(row.clone()) }
                }).collect()
        };
        anyhow::ensure!(stable_rows(&before_irreversible)? == stable_rows(&after_irreversible)?
            && after_irreversible.iter().filter(|row| row.starts_with("ats:")).count() == 1
            && after_irreversible.iter().filter(|row| row.starts_with("irreversible:")).count() == 1,
            "irreversible gate must not mutate volume/runtime/subject/residency/proof/session/frozen lease rows");
        anyhow::ensure!(gateway.received_requests().await.context("owned gateway request capture")?.len() == 1,
            "connected component must never retry/activate the workflow gateway");
        // The Node memory capability exercised real encrypted staging. Its finish
        // completes before commit and before any public receipt/evidence upload.
        node.proceed("irreversible_authorized").await?;
        let finish = node.phase("finish_before_commit", &["finishOutcome"]).await?;
        anyhow::ensure!(finish["finishOutcome"] == "submitted", "normal finalize submitted finish");
        db::run_blocking_db(|| -> Result<()> {
            let mut connection = pool.get_pg()?;
            let row = connection.query_one(
                "SELECT phase,finished_at_ms FROM jobs_execution_leases
                  WHERE account_id=$1 AND application_id=$2 AND run_id=$3",
                &[&account_id, &application_id, &run_id])?;
            anyhow::ensure!(row.get::<_, String>(0) == "submitted"
                && row.get::<_, Option<i64>>(1).is_some(), "normal finish durable submitted lease");
            Ok(())
        })?;
        let application_before_receipt = jobs::get_application(&pool, &account_id, &application_id)?
            .context("application before public receipt")?;
        anyhow::ensure!(application_before_receipt.state == "running"
            && application_before_receipt.submitted_at_ms.is_none()
            && jobs::list_application_evidence(&pool, &account_id, Some(&application_id))?.is_empty()
            && journey_store.as_ref().context("owned object store")?.received_requests().await
                .context("object-store capture")?.is_empty(),
            "finish must not manufacture application receipt/evidence or upload");
        let before_commit = journey_durable_snapshot(&pool, &account_id)?;
        node.proceed("finish_before_commit").await?;
        let committed = node.phase("result_committed", &["resultSha256"]).await?;
        let committed_sha256 = journey_digest(&committed, "resultSha256")?;
        anyhow::ensure!(receipt_observer.statuses_match(&[]),
            "no receipt request may precede the committed result");
        anyhow::ensure!(journey_durable_snapshot(&pool, &account_id)? == before_commit,
            "encrypted memory result commit must not bypass public receipt persistence");

        let before_terminalization = if defer_terminal_update {
            let workflow_before = journey_workflow_execution_snapshot(&pool, &account_id, &workflow_id)?;
            install_receipt_terminal_failure(&pool, &account_id, &workflow_id, &command_id)?;
            terminal_trigger_installed = true;
            Some(workflow_before)
        } else {
            None
        };
        node.proceed("result_committed").await?;
        let deferred_snapshot = if let Some(workflow_before) = before_terminalization {
            let deferred = node.phase("receipt_terminalization_deferred", &["status"]).await
                .with_context(|| receipt_observer.diagnosis())?;
            anyhow::ensure!(deferred["status"] == 503
                && receipt_observer.statuses_match(&[503])
                && journey_workflow_execution_snapshot(&pool, &account_id, &workflow_id)? == workflow_before,
                "receipt finalization failure must leave the exact workflow execution nonterminal");
            let application = jobs::get_application(&pool, &account_id, &application_id)?
                .context("application committed before workflow terminalization")?;
            anyhow::ensure!(application.state == "submitted" && application.submitted_at_ms.is_some()
                && application.receipt["receiptId"] == receipt_id
                && jobs::list_application_evidence(&pool, &account_id, Some(&application_id))?.len() == 3,
                "503 must follow a durable submitted application and three evidence rows");
            let rows = without_workflow_execution(journey_durable_snapshot(&pool, &account_id)?);
            let objects = stored_objects.lock().map_err(|_| anyhow::anyhow!("object store poisoned"))?.clone();
            anyhow::ensure!(objects.len() == 3, "503 must retain all three evidence objects");
            let requests = journey_store.as_ref().context("owned object store")?
                .received_requests().await.context("object-store capture before terminal replay")?.len();
            remove_receipt_terminal_failure(&pool)?;
            terminal_trigger_installed = false;
            node.proceed("receipt_terminalization_deferred").await?;
            Some((rows, objects, requests))
        } else {
            None
        };
        let posted_fields: &[&str] = if use_workflow_activity {
            &["status", "applicationState", "workflowActivity", "acknowledgementWithheld"]
        } else {
            &["status", "applicationState"]
        };
        let posted = node.phase("receipt_posted", posted_fields).await
            .with_context(|| receipt_observer.diagnosis())?;
        anyhow::ensure!(
            !use_workflow_activity
                || (posted["workflowActivity"] == true && posted["acknowledgementWithheld"] == true),
            "workflow must withhold the first actual committed receipt response"
        );
        let posted_statuses: &[u16] = if defer_terminal_update { &[503, 200] } else { &[200] };
        anyhow::ensure!(posted["status"] == 200 && posted["applicationState"] == "submitted"
            && receipt_observer.statuses_match(posted_statuses),
            "normal HMAC public receipt must complete the application");
        if let Some((rows, objects, requests)) = deferred_snapshot {
            anyhow::ensure!(
                without_workflow_execution(journey_durable_snapshot(&pool, &account_id)?) == rows
                    && *stored_objects.lock().map_err(|_| anyhow::anyhow!("object store poisoned"))? == objects
                    && journey_store.as_ref().context("owned object store")?.received_requests().await
                        .context("object-store capture after terminal replay")?.len() == requests,
                "terminal recovery may update the workflow but must not reupload or mutate receipt evidence"
            );
        }
        anyhow::ensure!(
            journey_case_accepted_attempt_snapshot(&pool, case, &account_id, &command_id)?
                == accepted_attempt_after_handoff,
            "first public receipt must preserve accepted attempt/event authority"
        );
        let application = jobs::get_application(&pool, &account_id, &application_id)?
            .context("submitted durable application")?;
        let frozen_lease = |rows: &[String]| -> Result<serde_json::Value> {
            let mut leases = rows.iter().filter_map(|row| row.strip_prefix("lease:"));
            let mut value: serde_json::Value = serde_json::from_str(leases.next().context("one durable lease")?)?;
            anyhow::ensure!(leases.next().is_none(), "one exact lease, never a replacement");
            let object = value.as_object_mut().context("durable lease object")?;
            for mutable in ["phase", "updated_at_ms", "finished_at_ms"] { object.remove(mutable); }
            Ok(value)
        };
        anyhow::ensure!(frozen_lease(&before_effect)? == frozen_lease(&effect_snapshot()?)?,
            "finish/receipt must preserve token hash/fence, expiry and all twelve frozen managed columns");
        let _receipt_fingerprint = journey_digest(&application.receipt,
            "_bluey_server_submission_fingerprint_v1")?;
        anyhow::ensure!(application.state == "submitted" && application.submitted_at_ms.is_some()
            && application.resume_version_id == application_before_receipt.resume_version_id
            && jobs::stored_final_submit_proof(&application)? == final_submit_proof
            && application.receipt["accountId"] == account_id && application.receipt["runId"] == run_id
            && application.receipt["receiptId"] == receipt_id
            && application.receipt["schemaVersion"] == 2
            && application.receipt["atsCertifiedReceiptAuthority"] == serde_json::to_value(&presented_ats)?,
            "receipt must preserve frozen identity/resume/proof and real Phase-B authority");
        let workflow_execution_after_receipt =
            journey_workflow_execution_snapshot(&pool, &account_id, &workflow_id)?;
        let retained_command = jobs::get_jobs_workflow_command(&pool, &account_id, &command_id)?
            .context("original immutable workflow command")?;
        match case {
            ManagedReceiptJourneyCase::DeliveryUnknown => {
                // Preserve the baseline random workflow/DeliveryUnknown fixture. The public
                // receipt's derived workflow-id lookup has no command, so its Ok(false)
                // branch is not a Temporal/workflow-terminal completion proof.
                anyhow::ensure!(
                    workflow_id != format!("bluey-jobs-v2-{run_id}")
                        && retained_command.state == JobsWorkflowCommandState::DeliveryUnknown
                        && workflow_execution_after_receipt.is_none(),
                    "receipt component must not rewrite the original delivery-unknown workflow"
                );
            }
            ManagedReceiptJourneyCase::Accepted => {
                let execution: serde_json::Value = serde_json::from_str(
                    workflow_execution_after_receipt
                        .as_deref()
                        .context("canonical accepted workflow execution")?,
                )?;
                anyhow::ensure!(
                    workflow_id == format!("bluey-jobs-v2-{run_id}")
                        && retained_command.state == JobsWorkflowCommandState::Accepted
                        && retained_command.temporal_run_id.as_deref()
                            == Some(ACCEPTED_TEMPORAL_RUN_ID)
                        && execution["account_id"] == account_id
                        && execution["workflow_id"] == workflow_id
                        && execution["start_command_id"] == command_id
                        && execution["first_execution_run_id"] == ACCEPTED_TEMPORAL_RUN_ID
                        && execution["lifecycle_state"] == "terminal"
                        && execution["cleanup_state"] == "required"
                        && execution["terminal_at_ms"].as_i64().is_some(),
                    "canonical accepted Start must become the exact terminal execution"
                );
            }
        }
        db::run_blocking_db(|| -> Result<()> {
            let mut connection = pool.get_pg()?;
            let attempt = connection.query_one(
                "SELECT status,runner FROM jobs_attempt_reservations WHERE account_id=$1 AND application_id=$2",
                &[&account_id, &application_id])?;
            let lease = connection.query_one(
                "SELECT phase,finished_at_ms,fence,lease_token_sha256 FROM jobs_execution_leases
                  WHERE account_id=$1 AND application_id=$2 AND run_id=$3",
                &[&account_id, &application_id, &run_id])?;
            anyhow::ensure!(attempt.get::<_, String>(0) == "submitted"
                && attempt.get::<_, String>(1) == "cloud" && lease.get::<_, String>(0) == "submitted"
                && lease.get::<_, Option<i64>>(1).is_some() && lease.get::<_, i64>(2) == claim_fence
                && lease.get::<_, String>(3) == lease_token_sha256,
                "normal receipt must submit the actual attempt and retain the exact finished lease");
            let canonical_count: i64 = connection.query_one(
                "SELECT count(*) FROM jobs_workflow_commands WHERE account_id=$1 AND workflow_id=$2",
                &[&account_id, &format!("bluey-jobs-v2-{run_id}")])?.get(0);
            anyhow::ensure!(
                canonical_count
                    == match case {
                        ManagedReceiptJourneyCase::DeliveryUnknown => 0,
                        ManagedReceiptJourneyCase::Accepted => 1,
                    },
                "canonical workflow command count must match the selected journey"
            );
            Ok(())
        })?;
        let sessions = jobs::list_browser_sessions(&pool, &account_id)?;
        anyhow::ensure!(sessions.len() == 1 && sessions[0].id == browser_session_id
            && sessions[0].id != run_id && sessions[0].runner == "cloud"
            && sessions[0].application_id.as_deref() == Some(application_id.as_str())
            && sessions[0].status == "complete" && sessions[0].takeover_url.is_none(),
            "exact authorized cloud session must complete without any run-id alias");
        let evidence = jobs::list_application_evidence(&pool, &account_id, Some(&application_id))?;
        anyhow::ensure!(evidence.len() == 3
            && evidence.iter().filter(|item| item.kind == "resume").count() == 1
            && evidence.iter().filter(|item| item.kind == "application_receipt").count() == 1
            && evidence.iter().filter(|item| item.kind == "submission_confirmation").count() == 1
            && evidence.iter().filter(|item| item.kind == "submission_confirmation")
                .all(|item| item.sha256 == confirmation_sha256)
            && evidence.iter().filter(|item| item.kind == "resume")
                .all(|item| item.sha256 == source_resume_sha256
                    && item.resume_version_id == application.resume_version_id),
            "normal public receipt must atomically create bound evidence");
        let (status, bytes) = if let Some(candidate) = signed_up.as_ref() {
            journey_bearer_get(&router, &candidate.access_token, "/api/jobs/applications", 1024 * 1024).await?
        } else {
            journey_account_get(&router, JOURNEY_JWT_SECRET, &account_id,
                "/api/jobs/applications", 1024 * 1024).await?
        };
        let tracker: Vec<jobs::JobApplication> = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(status == StatusCode::OK && tracker.len() == 1
            && serde_json::to_value(&tracker[0])? == serde_json::to_value(&application)?,
            "account tracker HTTP readback must equal the actual durable Submitted application");
        let (status, bytes) = journey_account_get(&router, JOURNEY_JWT_SECRET, &account_id,
            "/api/jobs/browser-sessions", 1024 * 1024).await?;
        anyhow::ensure!(status == StatusCode::OK
            && serde_json::from_slice::<serde_json::Value>(&bytes)? == serde_json::to_value(&sessions)?,
            "account session HTTP readback must equal actual authorized durable session");
        let evidence_path = format!("/api/jobs/applications/{application_id}/evidence");
        let (status, bytes) = journey_account_get(&router, JOURNEY_JWT_SECRET, &account_id,
            &evidence_path, 1024 * 1024).await?;
        anyhow::ensure!(status == StatusCode::OK
            && serde_json::from_slice::<serde_json::Value>(&bytes)? == serde_json::to_value(&evidence)?,
            "public evidence list must equal the actual durable evidence");
        for item in &evidence {
            let path = format!("/api/jobs/applications/{application_id}/evidence/{}/download", item.id);
            let (status, bytes) = journey_account_get(&router, JOURNEY_JWT_SECRET, &account_id,
                &path, 1024 * 1024).await?;
            anyhow::ensure!(status == StatusCode::OK
                && hex::encode(Sha256::digest(&bytes)) == item.sha256,
                "public evidence download must return exact persisted object bytes");
            match item.kind.as_str() {
                "resume" => anyhow::ensure!(bytes == source_resume_bytes,
                    "public PDF exact synthetic bytes"),
                "submission_confirmation" => anyhow::ensure!(bytes == confirmation_png,
                    "public confirmation exact synthetic PNG bytes"),
                "application_receipt" => {
                    let bundle: serde_json::Value = serde_json::from_slice(&bytes)?;
                    anyhow::ensure!(bundle["receiptId"] == receipt_id
                        && bundle["receipt"]["atsCertifiedReceiptAuthority"] == serde_json::to_value(&presented_ats)?,
                        "downloaded bundle must retain the actual certified receipt");
                }
                _ => anyhow::bail!("unexpected public evidence kind"),
            }
        }
        #[cfg(feature = "jobs-submission-observation")]
        {
            use crate::api::jobs::submission_observation::{
                verify_stored_submissions, SubmissionObservationBudget, SubmissionObservationInput,
                SubmissionVerdict,
            };
            let (submitted_at_ms, updated_at_ms, stored_application_sha256) =
                db::run_blocking_db(|| -> Result<_> {
                    let mut connection = pool.get_pg()?;
                    let row = connection.query_one(
                        "SELECT submitted_at_ms,updated_at_ms,application_json
                           FROM jobs_applications
                          WHERE account_id=$1 AND id=$2",
                        &[&account_id, &application_id],
                    )?;
                    let raw: String = row.get(2);
                    Ok((
                        row.get::<_, i64>(0),
                        row.get::<_, i64>(1),
                        hex::encode(Sha256::digest(raw.as_bytes())),
                    ))
                })?;
            let storage = crate::object_storage::ObjectStorage::new(submission_observation_storage);
            let object_reads_before = journey_store
                .as_ref()
                .context("owned object store")?
                .received_requests()
                .await
                .context("submission observation object-store capture before verification")?
                .len();
            let observation = SubmissionObservationInput {
                account_id: &account_id,
                application_id: &application_id,
                submitted_at_ms,
                updated_at_ms,
                stored_application_sha256: &stored_application_sha256,
            };
            macro_rules! observe_one {
                ($input:expr, $budget:expr) => {{
                    let mut verdicts =
                        verify_stored_submissions(&pool, &storage, &[$input], $budget).await?;
                    anyhow::ensure!(verdicts.len() == 1, "one input must have one receipt verdict");
                    verdicts.remove(0)
                }};
            }
            let short_bytes = SubmissionObservationBudget::new(
                2,
                1,
                std::time::Duration::from_secs(10),
            )?;
            let unavailable = observe_one!(observation, short_bytes);
            anyhow::ensure!(
                unavailable.verdict == SubmissionVerdict::Unavailable
                    && unavailable.stored_application_sha256 == stored_application_sha256,
                "a receipt plan over the report byte budget must be unavailable"
            );
            anyhow::ensure!(
                journey_store
                    .as_ref()
                    .context("owned object store")?
                    .received_requests()
                    .await
                    .context("submission observation capture after byte refusal")?
                    .len()
                    == object_reads_before,
                "an over-budget receipt plan must perform no partial object reads"
            );
            let full_budget = || {
                SubmissionObservationBudget::new(
                    2,
                    48 * 1024 * 1024,
                    std::time::Duration::from_secs(10),
                )
            };
            let changed_digest = format!(
                "{}{}",
                if stored_application_sha256.starts_with('0') {
                    '1'
                } else {
                    '0'
                },
                &stored_application_sha256[1..]
            );
            let unavailable = observe_one!(
                SubmissionObservationInput {
                    stored_application_sha256: &changed_digest,
                    ..observation
                },
                full_budget()?
            );
            anyhow::ensure!(
                unavailable.verdict == SubmissionVerdict::Unavailable
                    && unavailable.stored_application_sha256 == changed_digest,
                "changed stored receipt digest must be unavailable"
            );
            anyhow::ensure!(
                journey_store
                    .as_ref()
                    .context("owned object store")?
                    .received_requests()
                    .await
                    .context("submission observation capture after changed digest")?
                    .len()
                    == object_reads_before,
                "a changed digest must not initiate an evidence read"
            );
            let unknown = SubmissionObservationInput {
                application_id: "receipt-budget-fixture-missing",
                ..observation
            };
            let deferred =
                verify_stored_submissions(&pool, &storage, &[observation, unknown], full_budget()?)
                    .await?;
            anyhow::ensure!(
                deferred.len() == 2
                    && deferred.iter().all(|result| result.verdict == SubmissionVerdict::Unavailable),
                "an incomplete cohort must not start an otherwise valid receipt plan"
            );
            anyhow::ensure!(
                journey_store
                    .as_ref()
                    .context("owned object store")?
                    .received_requests()
                    .await
                    .context("submission observation capture after cohort refusal")?
                    .len()
                    == object_reads_before,
                "an incomplete cohort must initiate no evidence reads"
            );
            // A 206 can contain exactly the expected bytes. Refuse the upstream
            // representation and do not read a second object from that plan.
            let first = evidence
                .iter()
                .filter(|item| {
                    matches!(item.kind.as_str(), "application_receipt" | "submission_confirmation")
                })
                .min_by(|a, b| a.id.cmp(&b.id))
                .context("first ordered receipt object")?;
            let first_path = format!("/synthetic-irreversible/{}", first.storage_key);
            let kept_objects = stored_objects
                .lock()
                .map_err(|_| anyhow::anyhow!("object store poisoned"))?
                .clone();
            let (media_type, bytes) = kept_objects.get(&first_path).context("first receipt bytes")?;
            let last = bytes.len().checked_sub(1).context("nonempty receipt bytes")?;
            let kept_rows = journey_durable_snapshot(&pool, &account_id)?;
            let store = journey_store.as_ref().context("owned object store")?;
            // Bound the response count directly; priority sorting can move a
            // mounted mock away from its scoped guard's registration index.
            Mock::given(method("GET"))
                .and(wiremock::matchers::path(first_path.as_str()))
                .respond_with(
                    ResponseTemplate::new(206)
                        .insert_header("content-range", format!("bytes 0-{last}/{}", bytes.len()))
                        .set_body_raw(bytes.clone(), media_type.as_str()),
                )
                .with_priority(1)
                .up_to_n_times(1)
                .mount(store)
                .await;
            let partial_result = observe_one!(observation, full_budget()?);
            let requests_after_partial = store
                .received_requests()
                .await
                .context("submission observation capture after partial response")?;
            let attempted = requests_after_partial
                .get(object_reads_before..)
                .context("observation must retain earlier request history")?;
            anyhow::ensure!(
                partial_result.verdict == SubmissionVerdict::Unavailable
                    && partial_result.stored_application_sha256 == stored_application_sha256
                    && attempted.len() == 1
                    && attempted[0].method.as_str() == "GET"
                    && attempted[0].url.path() == first_path,
                "an unsuitable first response must stop the receipt read plan"
            );
            anyhow::ensure!(
                journey_durable_snapshot(&pool, &account_id)? == kept_rows
                    && *stored_objects.lock().map_err(|_| anyhow::anyhow!("object store poisoned"))?
                        == kept_objects,
                "refused readback must not mutate the durable application or evidence"
            );
            let object_reads_before = requests_after_partial.len();
            let observed = observe_one!(observation, full_budget()?);
            anyhow::ensure!(
                observed.verdict == SubmissionVerdict::Verified
                    && observed.stored_application_sha256 == stored_application_sha256,
                "submission observation must verify the exact stored receipt version"
            );
            let object_reads_after_verified = journey_store
                .as_ref()
                .context("owned object store")?
                .received_requests()
                .await
                .context("submission observation object-store capture after verification")?
                .len();
            anyhow::ensure!(
                object_reads_after_verified == object_reads_before + 2,
                "verified submission observation must read exactly the receipt and confirmation objects"
            );
            if case == ManagedReceiptJourneyCase::Accepted {
                println!("owned_jobs_strict_receipt_partial_stopped_then_verified=true");
            }
            let short_count = SubmissionObservationBudget::new(
                1,
                48 * 1024 * 1024,
                std::time::Duration::from_secs(10),
            )?;
            let unavailable = observe_one!(observation, short_count);
            anyhow::ensure!(
                unavailable.verdict == SubmissionVerdict::Unavailable
                    && unavailable.stored_application_sha256 == stored_application_sha256,
                "a two-object receipt must be unavailable under a one-object cap"
            );
            anyhow::ensure!(
                journey_store
                    .as_ref()
                    .context("owned object store")?
                    .received_requests()
                    .await
                    .context("submission observation capture after exhausted budget")?
                    .len()
                    == object_reads_after_verified,
                "an exhausted budget must not read another receipt object"
            );
        }
        let other = db::run_blocking_db(|| -> Result<_> {
            let account = db::accounts::Account::create_with_admin(&pool,
                &format!("receipt-journey-other-{}@example.test", uuid::Uuid::new_v4()), "fixture-only", false)?;
            anyhow::ensure!(db::accounts::Account::mark_email_verified(&pool, &account.id)? == 1,
                "normal isolated other account verification");
            Ok(account)
        })?;
        // Prove the second account first fails the ordinary cohort gate. Then
        // admit it normally so the following empty/404 assertions exercise
        // tenant isolation instead of accidentally testing the beta gate.
        let (unadmitted_status, _) = journey_account_get(&router, JOURNEY_JWT_SECRET, &other.id,
            "/api/jobs/applications", 4096).await?;
        anyhow::ensure!(unadmitted_status == StatusCode::FORBIDDEN,
            "full one-seat test cohort must deny the second account; status={unadmitted_status}");
        db::run_blocking_db(|| -> Result<()> {
            let cohort = jobs_beta_access::get_public_beta_cohort(&pool)?;
            anyhow::ensure!(cohort.state == PublicBetaCohortState::Open
                && cohort.hard_cap == 1 && cohort.assigned_count == 1,
                "only the first synthetic account has a cohort slot before the cross-tenant test");
            let expanded = jobs_beta_access::update_public_beta_cohort_audited(
                &pool,
                PublicBetaCohortUpdate {
                    expected_revision: cohort.revision,
                    state: PublicBetaCohortState::Open,
                    opens_at_ms: cohort.opens_at_ms,
                    closes_at_ms: cohort.closes_at_ms,
                    hard_cap: 2,
                },
                &beta_admin_id,
            )?;
            anyhow::ensure!(expanded.hard_cap == 2 && expanded.assigned_count == 1,
                "normal audited expansion adds exactly one available synthetic seat");
            let admission = jobs_beta_access::evaluate_or_enroll_public_beta(&pool, &other.id)?;
            anyhow::ensure!(admission.reason == PublicBetaAccessReason::Admitted,
                "second verified nonadmin account must be normally admitted");
            let snapshot = jobs_beta_access::public_beta_aggregate_snapshot(&pool)?;
            anyhow::ensure!(snapshot.hard_cap == 2 && snapshot.assigned_count == 2
                && snapshot.public_enrollment_count == 2 && snapshot.admin_enrollment_count == 0,
                "both synthetic accounts use ordinary cohort admission");
            Ok(())
        })?;
        let other_before = journey_durable_snapshot(&pool, &other.id)?;
        for path in ["/api/jobs/applications".to_string(), "/api/jobs/browser-sessions".to_string()] {
            let (status, bytes) = journey_account_get(&router, JOURNEY_JWT_SECRET, &other.id,
                &path, 1024 * 1024).await?;
            anyhow::ensure!(status == StatusCode::OK
                && serde_json::from_slice::<serde_json::Value>(&bytes)? == json!([]),
                "another normally admitted account cannot list target tracker/session; status={status}");
        }
        let (status, _) = journey_account_get(&router, JOURNEY_JWT_SECRET, &other.id,
            &evidence_path, 4096).await?;
        anyhow::ensure!(status == StatusCode::NOT_FOUND, "other tenant evidence list must be absent");
        for item in &evidence {
            let path = format!("/api/jobs/applications/{application_id}/evidence/{}/download", item.id);
            let (status, _) = journey_account_get(&router, JOURNEY_JWT_SECRET, &other.id, &path, 4096).await?;
            anyhow::ensure!(status == StatusCode::NOT_FOUND, "other tenant object download must be absent");
        }
        let posted_snapshot = journey_durable_snapshot(&pool, &account_id)?;
        let protected_rows_after_post = effect_snapshot()?;
        let objects_after_post = stored_objects.lock().map_err(|_| anyhow::anyhow!("object store poisoned"))?.clone();
        anyhow::ensure!(objects_after_post.len() == 3
            && objects_after_post.values().filter(|(media, bytes)| media == "application/pdf" && *bytes == source_resume_bytes).count() == 1
            && objects_after_post.values().filter(|(media, bytes)| media == "image/png" && *bytes == confirmation_png).count() == 1,
            "exact PDF/PNG/bundle objects must originate from the receipt API");
        db::run_blocking_db(|| -> Result<()> {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead).read_only(true).start()?;
            let capacity = tx.query_one(
                "SELECT state,consumed_bytes,consumed_objects,reserved_bytes,reserved_objects
                 FROM jobs_submission_evidence_capacity WHERE account_id=$1 AND application_id=$2 AND run_id=$3",
                &[&account_id, &application_id, &run_id])?;
            let actual_bytes = i64::try_from(objects_after_post.values().map(|(_, bytes)| bytes.len()).sum::<usize>())?;
            anyhow::ensure!(capacity.get::<_, String>(0) == "committed"
                && capacity.get::<_, i64>(1) == actual_bytes && capacity.get::<_, i64>(2) == 3
                && capacity.get::<_, i64>(3) == 48 * 1024 * 1024 && capacity.get::<_, i64>(4) == 13,
                "normal receipt commits exactly the three measured evidence objects against protected capacity");
            let uploads = tx.query("SELECT object_key,sha256,size_bytes,state FROM object_uploads
                WHERE account_id=$1 ORDER BY object_key", &[&account_id])?;
            anyhow::ensure!(uploads.len() == 3 + usize::from(signed_up.is_some()), "exact evidence and source upload ledger");
            for upload in uploads {
                let key: String = upload.get(0);
                if signed_up.is_some() && key == source_resume.storage_key {
                    anyhow::ensure!(upload.get::<_, String>(1) == source_resume.sha256
                        && upload.get::<_, i64>(2) == source_resume.size_bytes
                        && upload.get::<_, String>(3) == "ready",
                        "authored source must remain ready, independent of receipt artifacts");
                    continue;
                }
                let (_, bytes) = objects_after_post.get(&format!("/synthetic-irreversible/{key}"))
                    .context("actual upload object-key readback")?;
                anyhow::ensure!(upload.get::<_, String>(1) == hex::encode(Sha256::digest(bytes))
                    && upload.get::<_, i64>(2) == i64::try_from(bytes.len())?
                    && upload.get::<_, String>(3) == "ready",
                    "every committed upload row must bind measured immutable object bytes");
            }
            tx.commit()?;
            Ok(())
        })?;
        let request_count = journey_store.as_ref().context("owned object store")?
            .received_requests().await.context("object-store capture")?.len();

        node.proceed("receipt_posted").await?;
        let replay_fields: &[&str] = if use_workflow_activity {
            &["status", "applicationState", "workflowActivity", "resultReads"]
        } else {
            &["status", "applicationState"]
        };
        let replay = node.phase("receipt_replayed", replay_fields).await?;
        anyhow::ensure!(
            !use_workflow_activity || (replay["workflowActivity"] == true && replay["resultReads"] == 2),
            "workflow retry must reload the exact committed submission result"
        );
        let replayed_statuses: &[u16] = if defer_terminal_update { &[503, 200, 200] } else { &[200, 200] };
        anyhow::ensure!(
            journey_case_accepted_attempt_snapshot(&pool, case, &account_id, &command_id)?
                == accepted_attempt_after_handoff,
            "identical public receipt replay must preserve accepted attempt/event authority"
        );
        anyhow::ensure!(replay["status"] == 200 && replay["applicationState"] == "submitted"
            && receipt_observer.statuses_match(replayed_statuses)
            && journey_durable_snapshot(&pool, &account_id)? == posted_snapshot
            && journey_durable_snapshot(&pool, &other.id)? == other_before
            && effect_snapshot()? == protected_rows_after_post
            && journey_workflow_execution_snapshot(&pool, &account_id, &workflow_id)?
                == workflow_execution_after_receipt
            && *stored_objects.lock().map_err(|_| anyhow::anyhow!("object store poisoned"))?
                == objects_after_post
            && journey_store.as_ref().context("owned object store")?.received_requests().await
                .context("object-store capture")?.len() == request_count
            && gateway.received_requests().await.context("gateway capture")?.len() == 1,
            "identical public receipt replay must preserve terminal execution, rows, objects and gateway count");
        node.proceed("receipt_replayed").await?;
        let changed = node.phase("fingerprint_rejected", &["status"]).await?;
        anyhow::ensure!(
            journey_case_accepted_attempt_snapshot(&pool, case, &account_id, &command_id)?
                == accepted_attempt_after_handoff,
            "changed fingerprint must preserve accepted attempt/event authority"
        );
        anyhow::ensure!(changed["status"] == 409
            && journey_durable_snapshot(&pool, &account_id)? == posted_snapshot
            && journey_durable_snapshot(&pool, &other.id)? == other_before
            && effect_snapshot()? == protected_rows_after_post
            && journey_workflow_execution_snapshot(&pool, &account_id, &workflow_id)?
                == workflow_execution_after_receipt
            && *stored_objects.lock().map_err(|_| anyhow::anyhow!("object store poisoned"))? == objects_after_post
            && journey_store.as_ref().context("owned object store")?.received_requests().await
                .context("object-store capture")?.len() == request_count
            && gateway.received_requests().await.context("gateway capture")?.len() == 1,
            "changed fingerprint must fail without new objects/rows, gateway work or cross-tenant changes");
        node.proceed("fingerprint_rejected").await?;
        let result = node.finish().await?;
        let final_statuses: &[u16] = if defer_terminal_update {
            &[503, 200, 200, 409]
        } else {
            &[200, 200, 409]
        };
        anyhow::ensure!(
            receipt_observer.statuses_match(final_statuses),
            "unexpected mounted receipt response sequence: {}",
            receipt_observer.diagnosis()
        );
        anyhow::ensure!(result["applicationId"] == application_id && result["runId"] == run_id
            && result["resultSha256"] == committed_sha256 && result["receiptStatus"] == 200
            && result["replayStatus"] == 200 && result["fingerprintStatus"] == 409,
            "private final result must join the actual completed phase observations");
        anyhow::ensure!(gateway.received_requests().await.context("gateway capture")?.len() == 1,
            "connected journey cannot cause another workflow/provider call");
        if let Some(provider) = warmup_model.as_ref() {
            let browser = (followup == ReceiptFollowup::InterviewPrepBrowser)
                .then_some((&*node, jobs_address.port()));
            verify_postgres_interview_prep_from_receipt(
                &pool,
                &router,
                provider,
                JOURNEY_JWT_SECRET,
                InterviewPrepActors {
                    account_id: &account_id,
                    other_id: &other.id,
                },
                &application,
                browser,
            )
            .await?;
            anyhow::ensure!(journey_durable_snapshot(&pool, &account_id)? == posted_snapshot
                && journey_durable_snapshot(&pool, &other.id)? == other_before
                && *stored_objects.lock().map_err(|_| anyhow::anyhow!("object store poisoned"))? == objects_after_post,
                "warmup must not change receipt, application, workflow, evidence, or the other tenant");
        }
        drop(environment);
        Ok(())
    })
    .catch_unwind()
    .await;

    // Stop/join private children before dropping servers or restoring outer state,
    // including on an assertion error or unwind. Canonical launcher owns signals.
    let node_cleanup = match journey_node.as_mut() {
        Some(node) => node.stop().await,
        None => Ok(()),
    };
    drop(journey_node);
    let api_cleanup = match journey_api.take() {
        Some(handle) => {
            handle.abort();
            match handle.await {
                Err(error) if error.is_cancelled() => Ok(()),
                Err(error) => Err(anyhow::anyhow!("private journey API join failed: {error}")),
                Ok(_) => Err(anyhow::anyhow!("private journey API returned unexpectedly")),
            }
        }
        None => Ok(()),
    };
    let dispatcher_cleanup = match dispatcher.take() {
        Some(handle) => abort_dispatcher(handle).await,
        None => Ok(()),
    };
    let fault_cleanup = if terminal_trigger_installed {
        remove_receipt_terminal_failure(&pool)
    } else {
        Ok(())
    };
    let cleanup = node_cleanup
        .and(api_cleanup)
        .and(dispatcher_cleanup)
        .and(fault_cleanup);
    let store_drop = journey_store.take().map(drop_gateway).unwrap_or(Ok(()));
    let warmup_drop = warmup_model.take().map(drop_gateway).unwrap_or(Ok(()));
    drop(environment);
    let gateway_drop = drop_gateway(gateway);
    let mock_drop = store_drop.and(gateway_drop).and(warmup_drop);
    match outcome {
        Ok(Ok(())) => {
            cleanup?;
            if let Err(payload) = mock_drop {
                std::panic::resume_unwind(payload);
            }
            println!("phase657_owned_postgres_managed_request_start_dispatch_complete");
            println!("phase657_owned_postgres_managed_claim_complete");
            println!("phase657_owned_postgres_managed_effect_authorized");
            println!("phase657_owned_postgres_managed_irreversible_authorized");
            println!("phase658_owned_postgres_managed_connected_receipt_complete");
            if defer_terminal_update {
                println!("phase660_owned_postgres_managed_receipt_terminal_update_recovery_complete=true");
            } else if followup == ReceiptFollowup::InterviewPrep {
                println!("phase660_owned_postgres_managed_receipt_interview_prep_complete=true");
            } else if followup == ReceiptFollowup::InterviewPrepBrowser {
                println!(
                    "phase660_owned_postgres_managed_receipt_interview_prep_browser_complete=true"
                );
            } else if followup == ReceiptFollowup::VerifiedSignup {
                println!("phase660_owned_postgres_managed_receipt_verified_signup_complete=true");
            } else if case == ManagedReceiptJourneyCase::Accepted {
                println!(
                    "phase660_owned_postgres_managed_start_accepted_receipt_replay_complete=true"
                );
            }
            Ok(())
        }
        Ok(Err(primary)) => match cleanup {
            Ok(()) => match mock_drop {
                Ok(()) => Err(primary),
                Err(_) => Err(primary.context("gateway mock verification panicked")),
            },
            Err(cleanup) => Err(primary.context(format!("dispatcher cleanup failed: {cleanup}"))),
        },
        Err(payload) => {
            if let Err(cleanup) = cleanup {
                eprintln!("dispatcher cleanup failed while unwinding test panic: {cleanup}");
            }
            if mock_drop.is_err() {
                eprintln!("gateway mock verification panicked while unwinding test panic");
            }
            std::panic::resume_unwind(payload)
        }
    }
}
