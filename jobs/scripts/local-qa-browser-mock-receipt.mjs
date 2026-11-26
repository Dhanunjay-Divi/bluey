import { LOCAL_QA_SAMPLE_RESUME } from "./local-qa-browser-sample-resume.mjs";
import { requireImportedQaProfile } from "./local-qa-imported-resume-prepare.mjs";

const TIMEOUT_MS = 8_000;
const REFUSAL = Symbol("localQaBrowserMockReceiptRefusal");
const STAGES = new Set([
  "input", "origin", "applications_page", "workspace_response", "workspace_body", "application_row",
  "resume_response", "resume_body", "submitted_state",
]);
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const SHA256 = /^[0-9a-f]{64}$/i;
const whole = (regex, value) => typeof value === "string" && regex.exec(value)?.[0] === value;

function refusal(stage) {
  const safe = STAGES.has(stage) ? stage : "internal";
  const error = new Error(`local_qa_browser_mock_receipt_refused:${safe}`);
  Object.defineProperty(error, REFUSAL, { value: true });
  return error;
}

async function guarded(stage, action) {
  try { return await action(); } catch (error) {
    if (error?.[REFUSAL] === true) throw error;
    throw refusal(stage);
  }
}

function plain(value) {
  return value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype;
}

function origin(value) {
  let parsed;
  try { parsed = new URL(value); } catch { throw refusal("origin"); }
  const port = Number(parsed.port);
  if (parsed.protocol !== "http:" || parsed.hostname !== "127.0.0.1" || !Number.isInteger(port)
    || port <= 1024 || port > 65535 || port === 5187 || parsed.username || parsed.password
    || parsed.pathname !== "/" || parsed.search || parsed.hash || parsed.origin !== value) throw refusal("origin");
  return parsed.origin;
}

function expectedFrame(value) {
  const names = "accountId,applicationId,jobId,receiptObjectSha256,resumeId,runId,sourceAssetId,sourceSha256,submissionFingerprintSha256";
  if (!plain(value) || Object.keys(value).sort().join(",") !== names
    || ![value.accountId, value.applicationId, value.resumeId, value.sourceAssetId].every(item => whole(UUID, item))
    || ![value.jobId, value.runId].every(item => whole(/^[A-Za-z0-9_-]{1,160}$/, item))
    || ![value.sourceSha256, value.receiptObjectSha256, value.submissionFingerprintSha256].every(item => whole(SHA256, item))
    || value.sourceSha256 !== LOCAL_QA_SAMPLE_RESUME.sha256) {
    throw refusal("input");
  }
  return Object.freeze({ ...value });
}

async function exactUrl(page, value, stage) {
  if (await guarded(stage, () => page.url()) !== value) throw refusal(stage);
}

async function visible(locator, stage) {
  await guarded(stage, () => locator.waitFor({ state: "visible", timeout: TIMEOUT_MS }));
}

function matchesWorkspaceResponse(base, response) {
  try { return response.url() === `${base}/api/jobs/workspace` && response.request().method() === "GET"; }
  catch { return false; }
}

function matchesResumeResponse(base, resumeId, response) {
  try {
    return response.url() === `${base}/api/jobs/resume-versions/${encodeURIComponent(resumeId)}`
      && response.request().method() === "GET";
  } catch { return false; }
}

function requireSubmittedWorkspace(workspace, expected) {
  if (!plain(workspace) || !plain(workspace.profile) || !Array.isArray(workspace.applications)
    || workspace.applications.length !== 1) throw refusal("workspace_body");
  try { requireImportedQaProfile(workspace.profile, expected.sourceAssetId); }
  catch { throw refusal("workspace_body"); }
  if (workspace.profile.source_resume_sha256 !== expected.sourceSha256) throw refusal("workspace_body");
  const application = workspace.applications[0];
  if (!plain(application) || application.id !== expected.applicationId || application.job_id !== expected.jobId
    || application.resume_version_id !== expected.resumeId || application.state !== "submitted"
    || application.submission_mode !== "review_first"
    || application.run_id !== expected.runId || !Number.isSafeInteger(application.submitted_at_ms)
    || application.submitted_at_ms <= 0 || !plain(application.receipt)
    || application.receipt.accountId !== expected.accountId || application.receipt.applicationId !== expected.applicationId
    || application.receipt._bluey_server_submission_fingerprint_v1 !== expected.submissionFingerprintSha256
    || !plain(application.receipt.receiptObject)
    || application.receipt.receiptObject.sha256 !== expected.receiptObjectSha256
    || !plain(application.receipt.result) || application.receipt.result.status !== "submitted") throw refusal("workspace_body");
  const confirmationText = application.receipt.result.confirmationText;
  const confirmationUrl = application.receipt.result.confirmationUrl;
  if (!((typeof confirmationText === "string" && confirmationText.trim())
    || (typeof confirmationUrl === "string" && confirmationUrl.trim()))) throw refusal("workspace_body");
}

function requireResume(resume, expected) {
  const source = resume?.content?.provenance?.career_track_policy_authority;
  if (!plain(resume) || resume.id !== expected.resumeId || resume.job_id !== expected.jobId || resume.mode !== "factual"
    || !plain(source) || source.source_resume_asset_id !== expected.sourceAssetId
    || source.source_resume_sha256 !== expected.sourceSha256) throw refusal("resume_body");
}

/**
 * Read-only browser observation of a future worker-finalized synthetic receipt.
 * The caller owns the signed-in page and supplies only fixed, non-secret IDs/hashes.
 */
export async function observeLocalQaBrowserMockReceipt({ page, origin: inputOrigin, expected }) {
  if (!page) throw refusal("input");
  const base = origin(inputOrigin);
  const frame = expectedFrame(expected);
  const applicationsUrl = `${base}/jobs/applications`;
  await exactUrl(page, applicationsUrl, "applications_page");
  const workspaceWait = guarded("workspace_response", () =>
    page.waitForResponse(response => matchesWorkspaceResponse(base, response), { timeout: TIMEOUT_MS }));
  const [workspaceResponse] = await Promise.all([
    workspaceWait,
    guarded("applications_page", () => page.reload({ timeout: TIMEOUT_MS })),
  ]);
  if (await guarded("workspace_response", () => workspaceResponse.status()) !== 200) throw refusal("workspace_response");
  const workspace = await guarded("workspace_body", () => workspaceResponse.json());
  requireSubmittedWorkspace(workspace, frame);
  await exactUrl(page, applicationsUrl, "applications_page");

  const rows = page.locator("section.application-list button.application-row");
  await visible(rows, "application_row");
  if (await guarded("application_row", () => rows.count()) !== 1) throw refusal("application_row");
  await visible(rows.locator(".application-stage").getByText("Submitted", { exact: true }), "submitted_state");
  const resumeWait = guarded("resume_response", () =>
    page.waitForResponse(response => matchesResumeResponse(base, frame.resumeId, response), { timeout: TIMEOUT_MS }));
  const [resumeResponse] = await Promise.all([
    resumeWait,
    guarded("application_row", () => rows.click({ timeout: TIMEOUT_MS })),
  ]);
  if (await guarded("resume_response", () => resumeResponse.status()) !== 200) throw refusal("resume_response");
  const resume = await guarded("resume_body", () => resumeResponse.json());
  requireResume(resume, frame);
  return Object.freeze({ applicationCount: 1, submittedCount: 1, mockReceiptObserved: true });
}
