// Fixed synthetic V4 receipt journey for the Darwin owner.  This module owns
// neither a listener nor credentials: its parent supplies a candidate-bound,
// route-closed transport and the two already-admitted fixture operations.
import { randomUUID } from "node:crypto";

import {
  LOCAL_QA_SAMPLE_RESUME as sample,
  createLocalQaSampleResume,
} from "./local-qa-browser-sample-resume.mjs";
import {
  createLocalQaImportedResumeMatch,
  requireImportedQaPrepared,
  requireImportedQaProfile,
} from "./local-qa-imported-resume-prepare.mjs";

const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/;
const ID = /^[A-Za-z0-9_-]{1,160}$/;
const SHA256 = /^[a-f0-9]{64}$/;
const DRAFT_KEYS = "accountId,applicationId,jobId,resumeId,sourceAssetId,sourceSha256";
const RESULT_KEYS = "accountId,applicationId,jobId,receiptObjectSha256,resumeId,runId,sourceAssetId,sourceSha256,submissionFingerprintSha256";
const PROFILE_KEYS = [
  "email",
  "full_name",
  "headline",
  "source_resume_asset_id",
  "source_resume_media_type",
  "source_resume_name",
  "source_resume_sha256",
  "source_resume_template_status",
];
const RECEIPT_EVIDENCE = new WeakMap();

const plain = value => value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype;
const exact = (value, keys) => plain(value) && Object.keys(value).sort().join(",") === keys;
const match = (regex, value) => typeof value === "string" && regex.exec(value)?.[0] === value;
const fail = () => { throw new Error("local_qa_darwin_mock_receipt_journey_refused"); };
const need = value => { if (!value) fail(); };

function assertUpload(value) {
  need(plain(value) && plain(value.asset) && plain(value.profile));
  const { asset, profile } = value;
  need(match(UUID, asset.id) && asset.file_name === sample.fileName && asset.media_type === sample.mimeType
    && asset.file_type === "txt" && asset.template_status === "text_only" && asset.sha256 === sample.sha256
    && asset.size_bytes === createLocalQaSampleResume().length
    && PROFILE_KEYS.every(key => Object.hasOwn(profile, key))
    && Object.entries(sample.expectedProfile).every(([key, expected]) => profile[key] === expected)
    && profile.source_resume_asset_id === asset.id);
  return Object.freeze({ assetId: asset.id, sha256: asset.sha256 });
}

function bindDraft(value) {
  need(exact(value, DRAFT_KEYS) && [value.accountId, value.applicationId, value.resumeId, value.sourceAssetId]
    .every(id => match(UUID, id)) && match(ID, value.jobId) && value.sourceSha256 === sample.sha256);
  return Object.freeze({ ...value });
}

function bindResult(value, draft) {
  need(exact(value, RESULT_KEYS) && Object.entries(draft).every(([key, expected]) => value[key] === expected)
    && match(ID, value.runId) && match(SHA256, value.receiptObjectSha256)
    && match(SHA256, value.submissionFingerprintSha256));
  return Object.freeze({ ...value });
}

function assertCensus(value, phase) {
  need(plain(value) && value.accounts === 1 && value.profiles === 1 && value.applications === 1
    && value.resumes === 1 && value.auto_submit_authorizations === 0 && value.communication_actions === 0);
  if (phase === "prepared") {
    need(value.prepared === 1 && value.factual_draft_links === 1 && value.submitted === 0
      && value.attempts === 0 && value.browser_sessions === 0 && value.execution_leases === 0
      && value.metering === 0);
  } else {
    need(value.prepared === 0 && value.submitted === 1 && value.attempts === 1
      && value.browser_sessions === 1 && value.execution_leases === 1 && value.metering === 1
      && value.factual_draft_links === 0);
  }
}

function requireEvidence(evidence) {
  need(plain(evidence) && RECEIPT_EVIDENCE.has(evidence));
  return RECEIPT_EVIDENCE.get(evidence);
}

function requireCompletedReadback(metadata, accountId, workspace) {
  need(match(UUID, accountId) && accountId === metadata.accountId && plain(workspace));
  try {
    requireImportedQaProfile(workspace.profile, metadata.sourceAssetId);
  } catch {
    fail();
  }
  if (Object.hasOwn(workspace, "matches_page")) {
    need(plain(workspace.matches_page) && workspace.matches_page.next_cursor === null);
  }
  if (Object.hasOwn(workspace, "applications_page")) {
    need(plain(workspace.applications_page) && workspace.applications_page.next_cursor === null);
  }
  const applications = workspace.applications;
  const application = Array.isArray(applications) && applications.length === 1 ? applications[0] : null;
  need(application?.id === metadata.applicationId && application.job_id === metadata.jobId
    && application.resume_version_id === metadata.resumeId && application.state === "submitted"
    && application.submission_mode === "review_first" && application.run_id === metadata.runId
    && application.submitted_at_ms === metadata.submittedAtMs
    && application.receipt?.accountId === metadata.accountId
    && application.receipt?.applicationId === metadata.applicationId
    && application.receipt?.result?.status === "submitted"
    && application.receipt?.receiptObject?.sha256 === metadata.receiptObjectSha256
    && application.receipt?._bluey_server_submission_fingerprint_v1 === metadata.submissionFingerprintSha256);
}

/** Refuses fabricated/public-only values without exposing the private binding. */
export function requireLocalQaMockReceiptEvidence(evidence) {
  requireEvidence(evidence);
  return evidence;
}

/** Bind a fresh candidate workspace to one completed, branded synthetic receipt. */
export function bindLocalQaMockReceiptReadback(evidence, accountId, workspace) {
  const metadata = requireEvidence(evidence);
  requireCompletedReadback(metadata, accountId, workspace);
  return Object.freeze({ accountId, resumeId: metadata.resumeId });
}

/** Verify the factual resume preserves the completed receipt's immutable source provenance. */
export function assertLocalQaMockReceiptResume(evidence, resume) {
  const metadata = requireEvidence(evidence);
  const provenance = resume?.content?.provenance;
  const source = provenance?.career_track_policy_authority;
  need(resume?.id === metadata.resumeId && resume.job_id === metadata.jobId && resume.mode === "factual"
    && provenance?.generated_for_job_id === metadata.jobId
    && provenance?.job_snapshot_fingerprint === metadata.jobSnapshotFingerprint
    && source?.source_resume_asset_id === metadata.sourceAssetId
    && source?.source_resume_sha256 === metadata.sourceSha256);
}

/**
 * Exercise the exact synthetic review-first receipt lane without a browser.
 * `call` is already candidate-bound and must reject every route outside this
 * closed sequence; no bearer is accepted or returned here.
 */
export async function runLocalQaDarwinMockReceiptJourney({ accountId, call, seedPosting, publishMock, readCensus }) {
  let stage = "input";
  try {
    need(match(UUID, accountId) && [call, seedPosting, publishMock, readCensus].every(fn => typeof fn === "function"));
    stage = "upload";
    // Same reviewed candidate projection as the in-process verified
    // signup fixture; these facts come from the exact synthetic TXT.
    const profile = {
      full_name: sample.name, email: sample.candidateEmail, headline: sample.headline,
      summary: "Data Engineer building reliable, observable data pipelines for synthetic QA workloads.",
      skills: ["Python", "SQL", "PostgreSQL", "Airflow"],
      employment: [{ id: randomUUID(), company: "Bluey QA Systems", title: "Data Engineer",
        start_date: "2023-01", current: true, highlights: ["Built synthetic batch pipelines with Python and SQL.",
          "Improved data-quality checks for repeatable test fixtures."] }],
    };
    const upload = await call("/api/jobs/resume-source", {
      method: "POST",
      body: Object.freeze({
        request_id: randomUUID(),
        file_name: sample.fileName,
        media_type: sample.mimeType,
        bytes_base64: createLocalQaSampleResume().toString("base64"),
        profile,
      }),
    });
    const source = assertUpload(upload);
    stage = "onboarding_and_seed";
    const matchResult = await createLocalQaImportedResumeMatch({
      mockReceipt: true,
      readWorkspace: () => call("/api/jobs/workspace"),
      readTaxonomy: () => call("/api/jobs/taxonomy"),
      saveOnboarding: (body, headers) => call("/api/jobs/onboarding/complete", {
        method: "POST", body, headers,
      }),
      seedPosting,
    })({ accountId, assetId: source.assetId });
    stage = "prepare";
    const prepared = await call("/api/jobs/applications", {
      method: "POST",
      body: Object.freeze({ job_id: matchResult.jobId, mode: "factual", submission_mode: "review_first" }),
    });
    need(plain(prepared?.application) && plain(prepared?.resume_version)
      && match(UUID, prepared.application.id) && match(UUID, prepared.resume_version.id)
      && prepared.application.resume_version_id === prepared.resume_version.id);
    const draft = bindDraft({
      accountId,
      applicationId: prepared.application.id,
      jobId: matchResult.jobId,
      resumeId: prepared.resume_version.id,
      sourceAssetId: source.assetId,
      sourceSha256: source.sha256,
    });
    const beforeWorkspace = await call("/api/jobs/workspace");
    const beforeResume = await call(`/api/jobs/resume-versions/${draft.resumeId}`);
    requireImportedQaPrepared({
      mockReceipt: true,
      workspace: beforeWorkspace,
      resume: beforeResume,
      jobId: draft.jobId,
      assetId: draft.sourceAssetId,
      applicationId: draft.applicationId,
      resumeId: draft.resumeId,
    });
    const fingerprint = beforeResume.content.provenance.job_snapshot_fingerprint;
    stage = "prepared_census";
    assertCensus(await readCensus(draft, "prepared", null), "prepared");
    stage = "publish";
    const result = bindResult(await publishMock(draft), draft);
    stage = "receipt_readback";
    const afterWorkspace = await call("/api/jobs/workspace");
    const afterResume = await call(`/api/jobs/resume-versions/${draft.resumeId}`);
    requireImportedQaProfile(afterWorkspace?.profile, draft.sourceAssetId);
    const applications = afterWorkspace.applications;
    const provenance = afterResume?.content?.provenance;
    const application = Array.isArray(applications) && applications.length === 1 ? applications[0] : null;
    need(application?.id === draft.applicationId && application.job_id === draft.jobId
      && application.resume_version_id === draft.resumeId && application.state === "submitted"
      && application.submission_mode === "review_first" && application.run_id === result.runId
      && Number.isSafeInteger(application.submitted_at_ms) && application.submitted_at_ms > 0
      && afterResume?.id === draft.resumeId && afterResume.job_id === draft.jobId && afterResume.mode === "factual"
      && provenance?.generated_for_job_id === draft.jobId && provenance.job_snapshot_fingerprint === fingerprint
      && provenance.career_track_policy_authority?.source_resume_asset_id === draft.sourceAssetId
      && provenance.career_track_policy_authority?.source_resume_sha256 === draft.sourceSha256
      && application.receipt?.accountId === draft.accountId && application.receipt.applicationId === draft.applicationId
      && application.receipt.result?.status === "submitted"
      && application.receipt?.receiptObject?.sha256 === result.receiptObjectSha256
      && application.receipt?._bluey_server_submission_fingerprint_v1 === result.submissionFingerprintSha256);
    stage = "submitted_census";
    assertCensus(await readCensus(draft, "submitted", result.runId), "submitted");
    const evidence = Object.freeze({ sampleResumeImported: true, reviewFirstPrepared: true, mockReceiptPublished: true,
      applicationCount: 1, submittedCount: 1 });
    RECEIPT_EVIDENCE.set(evidence, Object.freeze({
      accountId: draft.accountId,
      applicationId: draft.applicationId,
      jobId: draft.jobId,
      resumeId: draft.resumeId,
      runId: result.runId,
      receiptObjectSha256: result.receiptObjectSha256,
      submissionFingerprintSha256: result.submissionFingerprintSha256,
      sourceAssetId: draft.sourceAssetId,
      sourceSha256: draft.sourceSha256,
      jobSnapshotFingerprint: fingerprint,
      submittedAtMs: application.submitted_at_ms,
    }));
    return evidence;
  } catch (error) {
    if (stage === "onboarding_and_seed") {
      const substage = /^local_qa_imported_resume_prepare_refused:(candidate|current_import|not_fresh|taxonomy|onboarding_binding|preexisting_match|match_binding)$/
        .exec(error?.message ?? "")?.[1];
      if (substage !== undefined) stage = `seed_${substage}`;
    }
    throw new Error(`local_qa_darwin_mock_receipt_journey_refused:${stage}`);
  }
}
