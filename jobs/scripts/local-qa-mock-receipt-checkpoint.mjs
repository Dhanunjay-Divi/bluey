// Same-session mock-receipt checkpoint for an already owned, disposable Jobs
// browser/API/Postgres fixture. None of these operations provisions a worker.
import { LOCAL_QA_SAMPLE_RESUME as sample } from "./local-qa-browser-sample-resume.mjs";
import { observeLocalQaBrowserMockReceipt } from "./local-qa-browser-mock-receipt.mjs";
import { requireImportedQaProfile } from "./local-qa-imported-resume-prepare.mjs";

const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/;
const ID = /^[A-Za-z0-9_-]{1,160}$/;
const SHA = /^[a-f0-9]{64}$/;
const DRAFT_KEYS = "accountId,applicationId,jobId,resumeId,sourceAssetId,sourceSha256";
const RESULT_KEYS = "accountId,applicationId,jobId,receiptObjectSha256,resumeId,runId,sourceAssetId,sourceSha256,submissionFingerprintSha256";
const EVIDENCE_KEYS = "applicationCount,mockReceiptObserved,submittedCount";
const FINGERPRINT = "_bluey_server_submission_fingerprint_v1";

const plain = value => value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype;
const exact = (value, keys) => plain(value) && Object.keys(value).sort().join(",") === keys;
const matches = (re, value) => typeof value === "string" && re.exec(value)?.[0] === value;
const empty = value => Array.isArray(value) && value.length === 0;

function need(condition, stage) {
  if (!condition) throw new Error(`local_qa_mock_receipt_checkpoint_refused:${stage}`);
}

function bindDraft(value) {
  need(exact(value, DRAFT_KEYS) && [value.accountId, value.applicationId, value.resumeId, value.sourceAssetId]
    .every(id => matches(UUID, id)) && matches(ID, value.jobId) && value.sourceSha256 === sample.sha256, "input");
  return Object.freeze({ ...value });
}

function requireOwnedReadback(snapshot, expected) {
  need(exact(snapshot, "account,census,resume,workspace"), "owned_readback");
  const { account, workspace, resume, census } = snapshot;
  need(account?.id === expected.accountId && account.email === sample.candidateEmail
    && account.is_admin === false && account.is_temporary === false, "candidate_readback");
  try { requireImportedQaProfile(workspace?.profile, expected.sourceAssetId); }
  catch { need(false, "source_profile"); }
  const row = workspace?.applications?.[0];
  const source = resume?.content?.provenance?.career_track_policy_authority;
  need(Array.isArray(workspace?.applications) && workspace.applications.length === 1
    && empty(workspace.auto_submit_authorizations)
    && row?.id === expected.applicationId && row.job_id === expected.jobId
    && row.resume_version_id === expected.resumeId && row.submission_mode === "review_first"
    && resume?.id === expected.resumeId && resume.job_id === expected.jobId && resume.mode === "factual"
    && resume.content?.provenance?.generated_for_job_id === expected.jobId
    && source?.source_resume_asset_id === expected.sourceAssetId && source.source_resume_sha256 === expected.sourceSha256
    && census?.accounts === 1 && census.profiles === 1 && census.applications === 1 && census.resumes === 1
    && census.auto_submit_authorizations === 0 && census.communication_actions === 0, "owned_readback");
  return row;
}

/**
 * Run the one-shot handoff while the owner still holds the same browser and
 * database. All callbacks are supplied by the isolated test owner, never by
 * the browser. The worker only receives the measured identity tuple. The
 * caller is responsible for restricting that worker to the owned mock lane.
 */
export function createOwnedQaMockReceiptCheckpoint({ assertOwned, readOwnedState, runMockWorker, observeInBrowser }) {
  need([assertOwned, readOwnedState, runMockWorker, observeInBrowser].every(fn => typeof fn === "function"), "operations");
  let attempted = false;
  const call = async (operation, value, stage) => {
    try { return await operation(value); }
    catch { need(false, stage); }
  };
  const current = async () => need(await call(assertOwned, undefined, "ownership") === true, "ownership");

  return async input => {
    need(!attempted, "already_attempted");
    attempted = true;
    const draft = bindDraft(input);
    await current();
    const before = await call(readOwnedState, draft, "read_draft");
    await current();
    const pending = requireOwnedReadback(before, draft);
    need(pending.state === "awaiting_review" && pending.run_id === null && pending.submitted_at_ms === null
      && plain(pending.receipt) && pending.receipt.confirmation === null && !Object.hasOwn(pending.receipt, "approved_execution")
      && pending.receipt.resume_version_id === draft.resumeId && matches(SHA, pending.receipt.job_snapshot_fingerprint)
      && pending.receipt.job_snapshot_fingerprint === before.resume.content.provenance.job_snapshot_fingerprint
      && before.census.prepared === 1 && before.census.factual_draft_links === 1
      && empty(before.workspace.browser_sessions) && ["submitted", "attempts", "commands", "browser_sessions",
        "execution_leases", "irreversible_receipts", "workflow_executions", "metering"].every(key => before.census[key] === 0),
    "unapproved_draft");

    const jobFingerprint = pending.receipt.job_snapshot_fingerprint;
    const raw = await call(runMockWorker, draft, "worker_failed");
    await current();
    need(exact(raw, RESULT_KEYS) && Object.entries(draft).every(([key, value]) => raw[key] === value)
      && matches(ID, raw.runId) && matches(SHA, raw.receiptObjectSha256)
      && matches(SHA, raw.submissionFingerprintSha256), "worker_result");
    const accepted = Object.freeze({ ...raw });
    const after = await call(readOwnedState, draft, "read_receipt");
    await current();
    const submitted = requireOwnedReadback(after, accepted);
    const confirmation = submitted.receipt?.result;
    need(submitted.state === "submitted" && submitted.run_id === accepted.runId
      && Number.isSafeInteger(submitted.submitted_at_ms) && submitted.submitted_at_ms > 0
      && after.resume.content.provenance.job_snapshot_fingerprint === jobFingerprint
      && plain(submitted.receipt) && submitted.receipt.applicationId === draft.applicationId
      && submitted.receipt.accountId === draft.accountId && confirmation?.status === "submitted"
      && [confirmation.confirmationText, confirmation.confirmationUrl].some(value => typeof value === "string" && value.trim())
      && submitted.receipt.receiptObject?.sha256 === accepted.receiptObjectSha256
      && submitted.receipt[FINGERPRINT] === accepted.submissionFingerprintSha256
      && after.census.submitted === 1 && after.census.prepared === 0 && after.census.attempts === 1, "committed_receipt");

    const browser = await call(observeInBrowser, accepted, "browser_receipt");
    await current();
    need(exact(browser, EVIDENCE_KEYS) && browser.applicationCount === 1 && browser.submittedCount === 1
      && browser.mockReceiptObserved === true, "browser_receipt");
    return Object.freeze({ applicationCount: 1, submittedCount: 1, mockReceiptObserved: true });
  };
}

/** Bind the last observation to the held signed-in page without handing it to the worker. */
export function createOwnedQaBrowserMockReceiptCheckpoint({ page, origin, assertOwned, readOwnedState, runMockWorker }) {
  const port = typeof origin === "string" ? /^http:\/\/127\.0\.0\.1:([1-9][0-9]{3,4})$/.exec(origin) : null;
  need(page && [page.url, page.reload, page.waitForResponse, page.locator, assertOwned].every(fn => typeof fn === "function")
    && port !== null && port[0] === origin
    && Number(port[1]) > 1024 && Number(port[1]) <= 65535 && Number(port[1]) !== 5187, "browser_binding");
  return createOwnedQaMockReceiptCheckpoint({
    assertOwned: async () => await assertOwned() === true && page.url() === `${origin}/jobs/applications`,
    readOwnedState, runMockWorker,
    observeInBrowser: expected => observeLocalQaBrowserMockReceipt({ page, origin, expected }),
  });
}
