//! Internal reader for the provider-yield report. No router or credential
//! acquisition is registered here. A host must already own the scoped reader.

use anyhow::{ensure, Result};
use serde::Serialize;
use std::{collections::HashSet, sync::Mutex, time::Duration};
use tokio::time::{timeout_at, Instant};

use super::{
    read_verified_application_evidence, valid_approved_execution_identifier,
    valid_approved_execution_sha256, validate_evidence_download_binding,
    validate_receipt_ats_certification_authority, validate_receipt_final_submit_proof, ApiError,
    ApplicationEvidence, JobApplication, ObjectStorage, MAX_RECEIPT_BUNDLE_BYTES,
    MAX_RECEIPT_EVIDENCE_BYTES, MAX_RECEIPT_SCREENSHOTS,
};
use crate::db::{
    jobs::submission_observation::{read_submission_observation, StoredSubmissionObservation},
    DbPool,
};

const MAX_EXACT_INTEGER: i64 = 9_007_199_254_740_991;
const MAX_REPORT_CANDIDATES: usize = 32;
const MAX_REPORT_EVIDENCE_OBJECTS: usize = MAX_REPORT_CANDIDATES * (MAX_RECEIPT_SCREENSHOTS + 1);
const MAX_REPORT_EVIDENCE_BYTES: usize = 128 * 1024 * 1024;
const MAX_REPORT_EVIDENCE_TIME: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionVerdict {
    Verified,
    Unverified,
    Unavailable,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionObservationVerdict {
    pub stored_application_sha256: String,
    pub verdict: SubmissionVerdict,
}

#[derive(Clone, Copy)]
pub struct SubmissionObservationInput<'a> {
    pub account_id: &'a str,
    pub application_id: &'a str,
    pub submitted_at_ms: i64,
    pub updated_at_ms: i64,
    pub stored_application_sha256: &'a str,
}

struct RemainingReadBudget {
    objects: usize,
    bytes: usize,
}

/// One budget for an entire report cohort, consumed on any verification call.
///
/// The bytes are reserved bounded object bodies, not measured network traffic.
/// The caller authorizes one budget per report; do not reinitialize it per candidate.
pub struct SubmissionObservationBudget {
    remaining: Mutex<RemainingReadBudget>,
    deadline: Instant,
}

impl SubmissionObservationBudget {
    pub fn new(max_objects: usize, max_bytes: usize, read_time: Duration) -> Result<Self> {
        ensure!(
            (1..=MAX_REPORT_EVIDENCE_OBJECTS).contains(&max_objects)
                && (1..=MAX_REPORT_EVIDENCE_BYTES).contains(&max_bytes)
                && !read_time.is_zero()
                && read_time <= MAX_REPORT_EVIDENCE_TIME,
            "invalid Jobs submission observation budget"
        );
        Ok(Self {
            remaining: Mutex::new(RemainingReadBudget {
                objects: max_objects,
                bytes: max_bytes,
            }),
            deadline: Instant::now() + read_time,
        })
    }

    fn reserve(&self, objects: usize, bytes: usize) -> bool {
        let Ok(mut remaining) = self.remaining.lock() else {
            return false;
        };
        if Instant::now() >= self.deadline
            || objects == 0
            || bytes == 0
            || objects > remaining.objects
            || bytes > remaining.bytes
        {
            return false;
        }
        // Commit the entire cohort before any read. Failed, interrupted, and
        // timed-out reads do not refund this report's conservative reservation.
        remaining.objects -= objects;
        remaining.bytes -= bytes;
        true
    }

    fn reserve_cohort(&self, mut sizes: impl Iterator<Item = usize>) -> bool {
        let Some((objects, bytes)) = sizes
            .try_fold((0_usize, 0_usize), |(objects, bytes), size| {
                Some((objects.checked_add(1)?, bytes.checked_add(size)?))
            })
        else {
            return false;
        };
        objects == 0 || self.reserve(objects, bytes)
    }
}

fn valid_request(
    account: &str,
    application: &str,
    submitted: i64,
    updated: i64,
    digest: &str,
) -> bool {
    valid_approved_execution_identifier(account)
        && valid_approved_execution_identifier(application)
        && (1..=MAX_EXACT_INTEGER).contains(&submitted)
        && (submitted..=MAX_EXACT_INTEGER).contains(&updated)
        && valid_approved_execution_sha256(digest)
}

fn error_verdict(error: ApiError) -> SubmissionVerdict {
    if error.0.is_server_error() {
        SubmissionVerdict::Unavailable
    } else {
        SubmissionVerdict::Unverified
    }
}

type ReceiptObjectRead = (usize, &'static str, usize);

struct ReceiptPlan {
    snapshot: StoredSubmissionObservation,
    objects: Vec<ReceiptObjectRead>,
}

fn object_plan(
    account_id: &str,
    application: &JobApplication,
    evidence: &[ApplicationEvidence],
) -> Option<Vec<ReceiptObjectRead>> {
    if !(2..=1 + MAX_RECEIPT_SCREENSHOTS).contains(&evidence.len()) {
        return None;
    }
    let mut result = Vec::with_capacity(evidence.len());
    let mut screenshot_bytes = 0_usize;
    for (index, item) in evidence.iter().enumerate() {
        let media_type = match item.kind.as_str() {
            "application_receipt" => "application/json",
            "submission_confirmation" => "image/png",
            _ => return None,
        };
        let size =
            validate_evidence_download_binding(account_id, application, item, evidence, media_type)
                .ok()?;
        if item.kind == "application_receipt" {
            if size > MAX_RECEIPT_BUNDLE_BYTES {
                return None;
            }
        } else {
            screenshot_bytes = screenshot_bytes.checked_add(size)?;
            if screenshot_bytes > MAX_RECEIPT_EVIDENCE_BYTES {
                return None;
            }
        }
        result.push((index, media_type, size));
    }
    Some(result)
}

impl SubmissionObservationInput<'_> {
    fn read(self, pool: &DbPool) -> Result<Option<StoredSubmissionObservation>> {
        read_submission_observation(
            pool,
            self.account_id,
            self.application_id,
            self.submitted_at_ms,
            self.updated_at_ms,
            self.stored_application_sha256,
        )
    }

    fn observed(self, verdict: SubmissionVerdict) -> SubmissionObservationVerdict {
        SubmissionObservationVerdict {
            stored_application_sha256: self.stored_application_sha256.to_owned(),
            verdict,
        }
    }
}

fn plan_submission(
    pool: &DbPool,
    storage: &ObjectStorage,
    input: SubmissionObservationInput<'_>,
) -> Result<ReceiptPlan, SubmissionVerdict> {
    let account_id = input.account_id;
    let Ok(Some(snapshot)) = input.read(pool) else {
        return Err(SubmissionVerdict::Unavailable);
    };
    let application = &snapshot.application;
    let receipt = &application.receipt;
    if receipt
        .get("schemaVersion")
        .and_then(|value| value.as_i64())
        != Some(2)
        || crate::db::jobs::validated_submitted_pre_submission_application(account_id, application)
            .is_err()
    {
        return Err(SubmissionVerdict::Unverified);
    }
    let Some(run_id) = application.run_id.as_deref() else {
        return Err(SubmissionVerdict::Unverified);
    };
    let Some(runner @ ("local" | "cloud")) = receipt.get("runner").and_then(|value| value.as_str())
    else {
        return Err(SubmissionVerdict::Unverified);
    };
    if let Err(error) = validate_receipt_final_submit_proof(application, receipt) {
        return Err(error_verdict(error));
    }
    if let Err(error) = validate_receipt_ats_certification_authority(
        pool,
        application,
        receipt,
        account_id,
        input.application_id,
        run_id,
        runner,
    ) {
        return Err(error_verdict(error));
    }
    if snapshot.evidence.len() < 2 {
        return Err(SubmissionVerdict::Unavailable);
    }
    let Some(objects) = object_plan(account_id, application, &snapshot.evidence) else {
        return Err(SubmissionVerdict::Unverified);
    };
    if objects.iter().any(|(index, _, size)| {
        let item = &snapshot.evidence[*index];
        item.storage_key.len() > 4_096
            || !storage.key_belongs_to_account(&item.storage_key, account_id)
            || *size > storage.max_object_bytes()
    }) {
        return Err(SubmissionVerdict::Unverified);
    }
    Ok(ReceiptPlan { snapshot, objects })
}

async fn run_plan(
    pool: &DbPool,
    storage: &ObjectStorage,
    input: SubmissionObservationInput<'_>,
    plan: ReceiptPlan,
    deadline: Instant,
) -> SubmissionVerdict {
    // Other plans may have been checked since this one was constructed.
    // Recheck the deletion fence and evidence revision before reading objects.
    if Instant::now() >= deadline {
        return SubmissionVerdict::Unavailable;
    }
    let Ok(Some(current)) = input.read(pool) else {
        return SubmissionVerdict::Unavailable;
    };
    if current.evidence_revision_sha256 != plan.snapshot.evidence_revision_sha256 {
        return SubmissionVerdict::Unavailable;
    }
    drop(current);
    let io = timeout_at(deadline, async {
        // Finish each bounded read before starting the next and drop the bytes.
        for (index, media_type, size) in plan.objects {
            if Instant::now() >= deadline {
                return Err(SubmissionVerdict::Unavailable);
            }
            read_verified_application_evidence(
                storage,
                input.account_id,
                &plan.snapshot.application,
                &plan.snapshot.evidence[index],
                media_type,
                size,
            )
            .await
            .map_err(error_verdict)?;
        }
        Ok(())
    })
    .await;
    match io {
        Ok(Ok(())) if Instant::now() < deadline => {}
        Ok(Err(verdict)) => return verdict,
        _ => return SubmissionVerdict::Unavailable,
    }
    // No database connection was held over object I/O. Reread the exact
    // ciphertext revision and ledger, also checking the deletion fence again.
    let Ok(Some(after)) = input.read(pool) else {
        return SubmissionVerdict::Unavailable;
    };
    if after.evidence_revision_sha256 != plan.snapshot.evidence_revision_sha256
        || Instant::now() >= deadline
    {
        return SubmissionVerdict::Unavailable;
    }
    SubmissionVerdict::Verified
}

/// Validate one authorized, single-account cohort in input order. Reserve the
/// entire remaining read plan before the first GET. If any needed plan cannot
/// be established, read no objects. This helper registers no transport, and
/// its deadline bounds object I/O, not blocking SQL.
pub async fn verify_stored_submissions(
    pool: &DbPool,
    storage: &ObjectStorage,
    inputs: &[SubmissionObservationInput<'_>],
    budget: SubmissionObservationBudget,
) -> Result<Vec<SubmissionObservationVerdict>> {
    ensure!(
        (1..=MAX_REPORT_CANDIDATES).contains(&inputs.len()),
        "invalid Jobs submission observation count"
    );
    let account_id = inputs[0].account_id;
    let mut applications = HashSet::with_capacity(inputs.len());
    ensure!(
        inputs.iter().all(|input| input.account_id == account_id
            && applications.insert(input.application_id)
            && valid_request(
                input.account_id,
                input.application_id,
                input.submitted_at_ms,
                input.updated_at_ms,
                input.stored_application_sha256
            )),
        "invalid Jobs submission observation cohort"
    );
    let unavailable = || {
        inputs
            .iter()
            .map(|i| i.observed(SubmissionVerdict::Unavailable))
            .collect()
    };
    let mut plans = Vec::with_capacity(inputs.len());
    for &input in inputs {
        if Instant::now() >= budget.deadline {
            return Ok(unavailable());
        }
        plans.push(plan_submission(pool, storage, input));
    }
    let incomplete = plans
        .iter()
        .any(|plan| matches!(plan, Err(SubmissionVerdict::Unavailable)));
    let object_sizes = plans
        .iter()
        .filter_map(|plan| plan.as_ref().ok())
        .flat_map(|plan| plan.objects.iter().map(|(_, _, size)| *size));
    let mut continue_reads = !incomplete && budget.reserve_cohort(object_sizes);
    let mut results = Vec::with_capacity(inputs.len());
    for (&input, plan) in inputs.iter().zip(plans) {
        let verdict = match plan {
            Err(verdict) => verdict,
            Ok(plan) if continue_reads => {
                run_plan(pool, storage, input, plan, budget.deadline).await
            }
            Ok(_) => SubmissionVerdict::Unavailable,
        };
        if verdict == SubmissionVerdict::Unavailable {
            continue_reads = false;
        }
        results.push(input.observed(verdict));
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn reporting_request_requires_an_exact_digest_and_bounded_version() {
        let hash = "c".repeat(64);
        assert!(valid_request("alpha", "job-1", 5, 6, &hash));
        assert!(!valid_request("another\naccount", "job-1", 5, 6, &hash));
        assert!(!valid_request("alpha", "job-1", 7, 6, &hash));
        assert!(!valid_request(
            "alpha",
            "job-1",
            1,
            MAX_EXACT_INTEGER + 1,
            &hash
        ));
        assert!(!valid_request(
            "alpha",
            "job-1",
            5,
            6,
            &hash.to_ascii_uppercase()
        ));
    }

    #[test]
    fn verdict_is_redacted_and_distinguishes_unavailable_reads() {
        let verdict = SubmissionObservationVerdict {
            stored_application_sha256: "a".repeat(64),
            verdict: error_verdict((StatusCode::BAD_GATEWAY, "private upstream error".into())),
        };
        assert_eq!(
            serde_json::to_value(verdict).unwrap(),
            serde_json::json!({
                "storedApplicationSha256": "a".repeat(64), "verdict": "unavailable"
            })
        );
        assert_eq!(
            error_verdict((StatusCode::CONFLICT, String::new())),
            SubmissionVerdict::Unverified
        );
    }

    #[test]
    fn budget_reserves_entire_plan_without_changing_balance_on_refusal() {
        let budget = SubmissionObservationBudget::new(3, 30, Duration::from_secs(5)).unwrap();
        assert!(!budget.reserve(4, 10));
        assert!(!budget.reserve(2, 31));
        assert!(budget.reserve(2, 20));
        assert!(!budget.reserve(2, 1));
        assert!(!budget.reserve(1, 11));
        assert!(budget.reserve(1, 10));
        assert!(!budget.reserve(1, 1));
    }

    #[test]
    fn combined_admission_does_not_charge_a_prefix_of_oversized_cohorts() {
        let budget =
            SubmissionObservationBudget::new(15, 128 * 1024 * 1024, Duration::from_secs(5))
                .unwrap();
        let one = [8, 10, 10, 10, 10].map(|mib| mib * 1024 * 1024);
        assert!(!budget.reserve_cohort(one.into_iter().cycle().take(15)));
        assert!(!budget.reserve_cohort([usize::MAX, 1].into_iter()));
        assert!(budget.reserve_cohort(one.into_iter().cycle().take(10)));
        assert!(!budget.reserve_cohort(one.into_iter()));
    }

    #[test]
    fn budget_rejects_configured_caps_and_expired_reports() {
        let time = Duration::from_secs(1);
        assert!(SubmissionObservationBudget::new(0, 10, time).is_err());
        assert!(SubmissionObservationBudget::new(1, 0, time).is_err());
        assert!(SubmissionObservationBudget::new(1, 10, Duration::ZERO).is_err());
        assert!(SubmissionObservationBudget::new(1, 10, MAX_REPORT_EVIDENCE_TIME + time).is_err());
        assert!(
            SubmissionObservationBudget::new(MAX_REPORT_EVIDENCE_OBJECTS + 1, 10, time).is_err()
        );
        assert!(SubmissionObservationBudget::new(1, MAX_REPORT_EVIDENCE_BYTES + 1, time).is_err());
        let mut budget = SubmissionObservationBudget::new(1, 10, time).unwrap();
        budget.deadline = Instant::now();
        assert!(!budget.reserve(1, 1));
    }

    #[test]
    fn parallel_reservations_cannot_both_consume_the_last_plan() {
        let budget = SubmissionObservationBudget::new(2, 10, Duration::from_secs(5)).unwrap();
        let start = std::sync::Barrier::new(2);
        let wins = std::thread::scope(|scope| {
            let contender = || {
                start.wait();
                budget.reserve(2, 10) as u8
            };
            let first = scope.spawn(contender);
            let second = contender();
            first.join().unwrap() + second
        });
        assert_eq!(wins, 1);
    }
}
