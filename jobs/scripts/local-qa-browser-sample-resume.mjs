import { createHash } from "node:crypto";

const TIMEOUT_MS = 8_000;
const REFUSAL = Symbol("localQaBrowserSampleResumeRefusal");
const REFUSAL_STAGES = new Set([
  "input",
  "origin",
  "jobs_page",
  "onboarding",
  "file_input",
  "review_dialog",
  "review_candidate",
  "upload_response",
  "upload_submit",
  "upload_status",
  "upload_body",
  "upload_asset",
  "upload_profile",
  "baseline",
]);
const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

const TEXT = `Casey Rowan
candidate@bluey-qa.test
Data Engineer

SUMMARY
Data Engineer building reliable, observable data pipelines for synthetic QA workloads.

EXPERIENCE
Data Engineer
Bluey QA Systems
Jan 2023 - Present
• Built synthetic batch pipelines with Python and SQL.
• Improved data-quality checks for repeatable test fixtures.

SKILLS
Python, SQL, PostgreSQL, Airflow`;
const BYTES = Buffer.from(TEXT, "utf8");
const SHA256 = createHash("sha256").update(BYTES).digest("hex");

export const LOCAL_QA_SAMPLE_RESUME = Object.freeze({
  fileName: "bluey-qa-resume.txt",
  mimeType: "text/plain",
  candidateEmail: "candidate@bluey-qa.test",
  name: "Casey Rowan",
  headline: "Data Engineer",
  text: TEXT,
  sha256: SHA256,
  expectedProfile: Object.freeze({
    full_name: "Casey Rowan",
    email: "candidate@bluey-qa.test",
    headline: "Data Engineer",
    source_resume_name: "bluey-qa-resume.txt",
    source_resume_sha256: SHA256,
    source_resume_media_type: "text/plain",
    source_resume_template_status: "text_only",
  }),
});

export function createLocalQaSampleResume() {
  return Buffer.from(BYTES);
}

function refusal(stage) {
  const suffix = REFUSAL_STAGES.has(stage) ? stage : "internal";
  const error = new Error(`local_qa_browser_sample_resume_refused:${suffix}`);
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

function requireOrigin(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw refusal("origin");
  }
  const port = Number(url.port);
  if (
    url.protocol !== "http:" ||
    url.hostname !== "127.0.0.1" ||
    !Number.isInteger(port) ||
    port <= 1024 ||
    port > 65535 ||
    port === 5187 ||
    url.username ||
    url.password ||
    url.pathname !== "/" ||
    url.search ||
    url.hash ||
    url.origin !== value
  ) {
    throw refusal("origin");
  }
  return url.origin;
}

async function requireUrl(page, expected, stage) {
  const current = await guarded(stage, () => page.url());
  if (current !== expected) throw refusal(stage);
}

async function requireVisible(locator, stage) {
  await guarded(stage, () => locator.waitFor({ state: "visible", timeout: TIMEOUT_MS }));
}

function matchingUpload(origin) {
  const expected = `${origin}/api/jobs/resume-source`;
  return (response) => {
    try {
      return response.url() === expected && response.request().method() === "POST";
    } catch {
      return false;
    }
  };
}

async function requireStoredResume(response) {
  const status = await guarded("upload_status", () => response.status());
  if (status !== 200) throw refusal("upload_status");
  const body = await guarded("upload_body", () => response.json());
  const { asset, profile } = body ?? {};
  const expected = LOCAL_QA_SAMPLE_RESUME;
  if (
    !asset ||
    typeof asset.id !== "string" ||
    !UUID_V4.test(asset.id) ||
    asset.file_name !== expected.fileName ||
    asset.media_type !== expected.mimeType ||
    asset.file_type !== "txt" ||
    asset.template_status !== "text_only" ||
    asset.sha256 !== expected.sha256 ||
    asset.size_bytes !== BYTES.length
  ) {
    throw refusal("upload_asset");
  }
  if (
    !profile ||
    profile.full_name !== expected.expectedProfile.full_name ||
    profile.email !== expected.expectedProfile.email ||
    profile.headline !== expected.expectedProfile.headline ||
    profile.source_resume_asset_id !== asset.id ||
    profile.source_resume_name !== expected.expectedProfile.source_resume_name ||
    profile.source_resume_sha256 !== expected.expectedProfile.source_resume_sha256 ||
    profile.source_resume_media_type !== expected.expectedProfile.source_resume_media_type ||
    profile.source_resume_template_status !== expected.expectedProfile.source_resume_template_status
  ) {
    throw refusal("upload_profile");
  }
  return asset.id;
}

/**
 * Imports one fixed synthetic text resume into an already-authenticated local QA
 * onboarding page. It neither starts a browser nor creates any on-disk document.
 */
export async function completeLocalQaBrowserSampleResume({ page, origin }) {
  if (!page) throw refusal("input");
  const local = requireOrigin(origin);
  const jobsUrl = `${local}/jobs/`;
  await requireUrl(page, jobsUrl, "jobs_page");
  await requireVisible(
    page.getByRole("heading", { name: "Import once, then tailor every job", exact: true }),
    "onboarding",
  );

  const fileInput = page.locator('input[type="file"][accept=".pdf,.docx,.txt"]');
  await guarded("file_input", () =>
    fileInput.setInputFiles({
      name: LOCAL_QA_SAMPLE_RESUME.fileName,
      mimeType: LOCAL_QA_SAMPLE_RESUME.mimeType,
      buffer: createLocalQaSampleResume(),
    }),
  );

  const dialog = page.getByRole("dialog", { name: "Review imported resume", exact: true });
  await requireVisible(dialog, "review_dialog");
  const extractedName = dialog.getByText(LOCAL_QA_SAMPLE_RESUME.name, { exact: true });
  await requireVisible(extractedName, "review_candidate");
  const replace = dialog.getByRole("button", { name: "Replace resume facts", exact: true });
  await requireVisible(replace, "review_dialog");

  const uploadResponse = guarded("upload_response", () =>
    page.waitForResponse(matchingUpload(local), { timeout: TIMEOUT_MS }),
  );
  const [response] = await Promise.all([
    uploadResponse,
    guarded("upload_submit", () => replace.click({ timeout: TIMEOUT_MS })),
  ]);
  const assetId = await requireStoredResume(response);

  const baseline = page.getByText("Baseline extracted", { exact: true });
  await requireVisible(baseline, "baseline");
  const nameField = page.getByLabel("Full name", { exact: true });
  await requireVisible(nameField, "baseline");
  const savedName = await guarded("baseline", () => nameField.inputValue({ timeout: TIMEOUT_MS }));
  if (savedName !== LOCAL_QA_SAMPLE_RESUME.name) throw refusal("baseline");

  return { assetId };
}
