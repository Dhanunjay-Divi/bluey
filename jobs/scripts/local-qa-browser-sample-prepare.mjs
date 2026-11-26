import { LOCAL_QA_SAMPLE_RESUME } from "./local-qa-browser-sample-resume.mjs";

const TIMEOUT_MS = 8_000;
const REFUSAL = Symbol("localQaBrowserSamplePrepareRefusal");
const STAGES = new Set([
  "input", "origin", "jobs_page", "match_row", "match_dialog", "mode", "review_first",
  "prepare_response", "prepare_submit", "prepare_status", "prepare_body", "application_page",
  "application_state",
]);
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const SHA256 = /^[0-9a-f]{64}$/i;
const TITLE = "Data Engineer";
const COMPANY = "Synthetic Qualification Employer";

function refusal(stage) {
  const safe = STAGES.has(stage) ? stage : "internal";
  const error = new Error(`local_qa_browser_sample_prepare_refused:${safe}`);
  Object.defineProperty(error, REFUSAL, { value: true });
  return error;
}

async function guarded(stage, operation) {
  try {
    return await operation();
  } catch (error) {
    if (error?.[REFUSAL] === true) throw error;
    throw refusal(stage);
  }
}

function localOrigin(value) {
  let url;
  try { url = new URL(value); } catch { throw refusal("origin"); }
  const port = Number(url.port);
  if (url.protocol !== "http:" || url.hostname !== "127.0.0.1" || !Number.isInteger(port)
    || port <= 1024 || port > 65535 || port === 5187 || url.username || url.password
    || url.pathname !== "/" || url.search || url.hash || url.origin !== value) throw refusal("origin");
  return url.origin;
}

function plain(value) {
  return value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype;
}

async function visible(locator, stage) {
  await guarded(stage, () => locator.waitFor({ state: "visible", timeout: TIMEOUT_MS }));
}

async function exactUrl(page, expected, stage) {
  const actual = await guarded(stage, () => page.url());
  if (actual !== expected) throw refusal(stage);
}

function exactPrepareResponse(origin, jobId, response) {
  try {
    if (response.url() !== `${origin}/api/jobs/applications` || response.request().method() !== "POST") return false;
    const raw = response.request().postData();
    return raw === JSON.stringify({ job_id: jobId, mode: "factual", submission_mode: "review_first" });
  } catch {
    return false;
  }
}

function requirePrepareEnvelope(body, jobId, assetId) {
  if (!plain(body) || Object.keys(body).sort().join(",") !== "application,resume_version") throw refusal("prepare_body");
  const application = body.application;
  const resume = body.resume_version;
  if (!plain(application) || !plain(resume) || !UUID.test(application.id) || !UUID.test(resume.id)
    || application.job_id !== jobId || application.resume_version_id !== resume.id
    || application.state !== "awaiting_review" || application.submission_mode !== "review_first"
    || application.run_id !== null || application.submitted_at_ms !== null
    || !plain(application.receipt) || Object.hasOwn(application.receipt, "approved_execution")
    || resume.job_id !== jobId || resume.mode !== "factual" || !plain(resume.content)
    || !plain(resume.content.provenance)) throw refusal("prepare_body");

  const authority = resume.content.provenance.career_track_policy_authority;
  if (!plain(authority) || authority.source_resume_asset_id !== assetId
    || authority.source_resume_sha256 !== LOCAL_QA_SAMPLE_RESUME.sha256
    || !SHA256.test(authority.source_resume_sha256)) throw refusal("prepare_body");
  return { applicationId: application.id, resumeId: resume.id };
}

/**
 * Performs exactly the visible, review-first Prepare action for the one seeded
 * local-QA Data Engineer match. It does not create a browser/context, seed data,
 * approve a packet, or expose credentials/tokens to its caller.
 */
export async function completeLocalQaBrowserSamplePrepare({ page, origin, jobId, assetId }) {
  if (!page || !UUID.test(jobId || "") || !UUID.test(assetId || "")) throw refusal("input");
  const local = localOrigin(origin);
  const jobsUrl = `${local}/jobs/matches`;
  const applicationsUrl = `${local}/jobs/applications`;
  await exactUrl(page, jobsUrl, "jobs_page");

  const rows = page.locator('section[aria-label="Job matches"] button.job-row')
    .filter({ hasText: TITLE }).filter({ hasText: COMPANY });
  await visible(rows, "match_row");
  if (await guarded("match_row", () => rows.count()) !== 1) throw refusal("match_row");
  await guarded("match_row", () => rows.click({ timeout: TIMEOUT_MS }));

  const dialog = page.getByRole("dialog", { name: `${TITLE} at ${COMPANY}`, exact: true });
  await visible(dialog, "match_dialog");
  const factual = dialog.getByRole("button", { name: "Factual", exact: true });
  const reviewFirst = dialog.getByRole("button", { name: "Review first", exact: true });
  const prepare = dialog.getByRole("button", { name: "Prepare application", exact: true });
  await visible(factual, "mode");
  await guarded("mode", () => factual.click({ timeout: TIMEOUT_MS }));
  await visible(reviewFirst, "review_first");
  await guarded("review_first", () => reviewFirst.click({ timeout: TIMEOUT_MS }));
  await visible(prepare, "prepare_submit");

  const responseWait = guarded("prepare_response", () =>
    page.waitForResponse((response) => exactPrepareResponse(local, jobId, response), { timeout: TIMEOUT_MS }));
  const navigationWait = guarded("application_page", () => page.waitForURL(applicationsUrl, { timeout: TIMEOUT_MS }));
  const [response] = await Promise.all([
    responseWait,
    navigationWait,
    guarded("prepare_submit", () => prepare.click({ timeout: TIMEOUT_MS })),
  ]);
  if (await guarded("prepare_status", () => response.status()) !== 200) throw refusal("prepare_status");
  const result = requirePrepareEnvelope(await guarded("prepare_body", () => response.json()), jobId, assetId);
  await exactUrl(page, applicationsUrl, "application_page");
  await visible(page.getByRole("heading", { name: "Applications", exact: true }), "application_page");
  const applications = page.locator("section.application-list button.application-row")
    .filter({ hasText: TITLE }).filter({ hasText: COMPANY });
  await visible(applications, "application_state");
  if (await guarded("application_state", () => applications.count()) !== 1) throw refusal("application_state");
  await visible(applications.getByText("Awaiting Review", { exact: true }), "application_state");
  return Object.freeze(result);
}
