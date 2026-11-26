// Positive Playwright witness for a dedicated synthetic, signed-in controller.
// The controller owns browser, database, receipt, local provider and network
// policy. This module neither creates those authorities nor launches anything.
import { isDeepStrictEqual } from "node:util";

const maxDraftBytes = 128 * 1024;
const maxWorkspaceBytes = 4 * 1024 * 1024;
const timeout = 10_000;
const prepPath = /^\/api\/jobs\/applications\/[^/]+\/interview-prep\/?$/;
const sha256 = /^[a-f0-9]{64}$/;
const selfReview = { kind: "candidate_self_review", facts_checked: true, reasoning_checked: true };
const fail = (code) => { throw new Error(code); };
const require = (value, code) => { if (!value) fail(code); };
const hasBearer = (value) => typeof value === "string" && /^Bearer \S+$/.test(value);

function parseJson(bytes, failure) {
  try { return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)); }
  catch { return fail(failure); }
}

function verifyGrounding(response, expected) {
  const g = response?.grounding;
  require(response?.schema_version === 1 && typeof response.id === "string"
    && /^prep-[a-f0-9]{24}$/.test(response.id), "prep_response_identity");
  require(response.application_id === expected.applicationId && response.content === expected.content
    && response.provider === expected.provider && response.model === expected.model,
  "prep_response_fixture_binding");
  require(g?.receipt_id === expected.receiptId && g.resume_version_id === expected.resumeVersionId
    && g.resume_checksum === expected.resumeChecksum && g.resume_document_sha256 === expected.resumeDocumentSha256
    && g.receipt_fingerprint === expected.receiptFingerprint, "prep_response_receipt_binding");
}

export function assertInterviewPrepWitnessExport(draft, response, expected, reviewed) {
  verifyGrounding(response, expected);
  const projection = {
    schema_version: 1, status: "draft", application_id: response.application_id,
    prep_id: response.id, generated_at_ms: response.generated_at_ms, content: response.content,
    grounding: {
      receipt_id: response.grounding.receipt_id,
      receipt_fingerprint: response.grounding.receipt_fingerprint,
      resume_version_id: response.grounding.resume_version_id,
      resume_checksum: response.grounding.resume_checksum,
      resume_document_sha256: response.grounding.resume_document_sha256,
      answer_keys_used: response.grounding.answer_keys_used,
      answer_keys_omitted: response.grounding.answer_keys_omitted,
    },
    generation: { provider: response.provider, model: response.model, cost_cents: response.cost_cents,
      cost_label: response.cost_label, confidence: response.confidence },
    ...(reviewed ? { candidate_review: selfReview } : {}),
  };
  // Do not surface private draft text or IDs in assertion output.
  require(typeof reviewed === "boolean", "prep_download_review_input");
  require(isDeepStrictEqual(draft, projection), "prep_download_projection");
}

async function readDownload(page, dialog, response, format) {
  const name = format === "txt" ? "Download practice draft (.txt)" : "Download structured draft (.json)";
  const [download] = await Promise.all([
    page.waitForEvent("download", { timeout }),
    dialog.getByRole("button", { name, exact: true }).click({ timeout }),
  ]);
  require(download.suggestedFilename() === `bluey-interview-prep-${response.id.slice(5)}.${format}`,
    format === "txt" ? "prep_text_download_filename" : "prep_download_filename");
  const stream = await download.createReadStream();
  require(stream, "prep_download_stream");
  const chunks = [];
  let size = 0;
  for await (const chunk of stream) {
    size += chunk.byteLength;
    require(size <= maxDraftBytes, "prep_download_size");
    chunks.push(chunk);
  }
  return { bytes: Buffer.concat(chunks), download };
}

async function readDraft(page, dialog, response) {
  return parseJson((await readDownload(page, dialog, response, "json")).bytes, "prep_download_json");
}

// Serialized by Playwright into the disposable fixture's own page. This
// observes the native Blob at creation, before the app synchronously
// revokes its object URL. It never changes the Blob or keeps its content.
export function practiceBlobProbe(action) {
  const key = Symbol.for("bluey.jobs.fixture.practiceBlobProbe");
  if (action === "open") {
    if (globalThis[key] !== undefined) throw new Error("prep_text_probe_already_active");
    const original = Object.getOwnPropertyDescriptor(URL, "createObjectURL");
    if (typeof original?.value !== "function" || !original.configurable) {
      throw new Error("prep_text_probe_unavailable");
    }
    const state = { original, count: 0, first: null, shim: null };
    state.shim = function (blob) {
      const url = Reflect.apply(original.value, this, arguments);
      state.count += 1;
      if (state.count === 1 && blob instanceof Blob) {
        state.first = { url, type: blob.type, size: blob.size };
      }
      return url;
    };
    Object.defineProperty(URL, "createObjectURL", { ...original, value: state.shim });
    globalThis[key] = state;
    return null;
  }
  const state = globalThis[key];
  if (!state || URL.createObjectURL !== state.shim) throw new Error("prep_text_probe_changed");
  if (action === "read") return { count: state.count, first: state.first };
  if (action !== "close") throw new Error("prep_text_probe_action");
  Object.defineProperty(URL, "createObjectURL", state.original);
  delete globalThis[key];
  return null;
}

export function assertInterviewPrepPracticeExport(bytes, response, expected, reviewed) {
  verifyGrounding(response, expected);
  require(typeof reviewed === "boolean", "prep_download_review_input");
  const note = reviewed
    ? "You marked the facts and reasoning checked. Bluey has not verified this draft."
    : "Check the facts and reasoning before rehearsing. Bluey has not verified this draft.";
  const exact = ["Bluey Jobs | Interview practice draft", note, "",
    "Rehearse in your own words. Use only examples you can explain and substantiate.", "",
    expected.content, ""].join("\n");
  // Exact bytes exclude JSON, billing, IDs and provider metadata, without
  // including the observed private payload in any error.
  require(Buffer.isBuffer(bytes) && bytes.equals(Buffer.from(exact, "utf8")), "prep_text_download_projection");
}

async function checkPracticeDownload(page, dialog, response, expected, reviewed, portalOrigin) {
  await page.evaluate(practiceBlobProbe, "open");
  try {
    const { bytes, download } = await readDownload(page, dialog, response, "txt");
    const metadata = await page.evaluate(practiceBlobProbe, "read");
    const url = download.url();
    require(typeof url === "string" && url.startsWith(`blob:${portalOrigin}/`)
      && metadata?.count === 1 && metadata.first?.url === url
      && metadata.first?.type === "text/plain;charset=utf-8" && metadata.first?.size === bytes.length,
    "prep_text_blob_binding");
    assertInterviewPrepPracticeExport(bytes, response, expected, reviewed);
  } finally {
    await page.evaluate(practiceBlobProbe, "close");
  }
}

function ownedOrigin(portalOrigin) {
  const origin = new URL(portalOrigin);
  require(origin.protocol === "http:" && origin.hostname === "127.0.0.1" && origin.port
    && origin.origin === portalOrigin, "prep_owned_portal");
  return origin.origin;
}

function assertExpected(expected) {
  require(["applicationId", "receiptId", "resumeVersionId", "resumeChecksum", "company", "provider", "model", "content"]
    .every((field) => typeof expected?.[field] === "string" && expected[field].length > 0 && expected[field].length <= 4096)
    && sha256.test(expected.resumeDocumentSha256) && sha256.test(expected.receiptFingerprint), "prep_expected_scope");
}

export async function observeSignedInInterviewPrep({ page, portalOrigin, openButton, expected,
  workspaceAuthorization }) {
  ownedOrigin(portalOrigin);
  const current = new URL(page.url());
  require(current.origin === portalOrigin
    && current.pathname.startsWith("/jobs/") && !current.searchParams.has("preview"), "prep_owned_portal");
  assertExpected(expected);
  const requestUrl = `${portalOrigin}/api/jobs/applications/${encodeURIComponent(expected.applicationId)}/interview-prep`;
  const requestBody = JSON.stringify({ prepare: true, receipt_fingerprint: expected.receiptFingerprint });
  const requests = [];
  let generate = false;
  const onRequest = (request) => {
    if (prepPath.test(new URL(request.url()).pathname)) {
      requests.push(generate && request.url() === requestUrl && request.method() === "POST"
        && request.postData() === requestBody);
    }
  };
  page.on("request", onRequest);
  try {
    require(await openButton.count() === 1, "prep_selected_open_button");
    await openButton.click({ timeout });
    const dialog = page.getByRole("dialog", { name: `Interview prep: ${expected.company}`, exact: true });
    await dialog.waitFor({ state: "visible", timeout });
    if (workspaceAuthorization !== undefined) {
      await dialog.getByText(`${expected.jobTitle} · Submitted resume v${expected.resumeVersionNo}`,
        { exact: true }).waitFor({ state: "visible", timeout });
    }
    const consent = dialog.getByRole("checkbox", { name: /^Prepare for this interview/ });
    const button = dialog.getByRole("button", { name: "Generate with Bluey", exact: true });
    require(!await consent.isChecked({ timeout }) && !await button.isEnabled({ timeout })
      && requests.length === 0, "prep_initial_consent");
    await consent.check({ timeout });
    require(await button.isEnabled({ timeout }) && requests.length === 0, "prep_toggle_no_request");
    generate = true;
    const [networkResponse] = await Promise.all([
      page.waitForResponse((value) => value.url() === requestUrl && value.request().method() === "POST", { timeout }),
      button.click({ timeout }),
    ]);
    generate = false;
    const authorization = await networkResponse.request().headerValue("authorization");
    require(hasBearer(authorization) && (workspaceAuthorization === undefined
      || authorization === workspaceAuthorization), "prep_authenticated_request");
    require(networkResponse.status() === 200, "prep_response_status");
    const responseBytes = await networkResponse.body();
    require(responseBytes.byteLength <= maxDraftBytes, "prep_response_size");
    const response = parseJson(responseBytes, "prep_response_json");
    verifyGrounding(response, expected);
    await dialog.getByRole("heading", { name: "Draft for review", exact: true }).waitFor({ state: "visible", timeout });
    require(await dialog.locator("article").textContent({ timeout }) === expected.content, "prep_visible_content");
    const facts = dialog.getByRole("checkbox", { name: /^I checked that personal claims/ });
    const reasoning = dialog.getByRole("checkbox", { name: /^I can explain the decisions/ });
    require(!await facts.isChecked({ timeout }) && !await reasoning.isChecked({ timeout }), "prep_initial_review");
    assertInterviewPrepWitnessExport(await readDraft(page, dialog, response), response, expected, false);
    await checkPracticeDownload(page, dialog, response, expected, false, portalOrigin);
    await facts.check({ timeout });
    await reasoning.check({ timeout });
    assertInterviewPrepWitnessExport(await readDraft(page, dialog, response), response, expected, true);
    await checkPracticeDownload(page, dialog, response, expected, true, portalOrigin);
    require(requests.length === 1 && requests[0] === true, "prep_exact_request_count");
    return { exactGenerateRequests: 1, structuredDraftDownloads: 2, textDraftDownloads: 2, scoped: true };
  } finally {
    page.off("request", onRequest);
  }
}

// Enter through the real workspace route, not an injected workspace object.
// The owner provisions the disposable browser/session and mounts the gateway.
// Observing authenticated API requests is not proof of the login form.
export async function observeReceiptInterviewPrepInPortal({ page, portalOrigin, expected }) {
  const origin = ownedOrigin(portalOrigin);
  assertExpected(expected);
  require(typeof expected?.jobTitle === "string" && expected.jobTitle.trim().length > 0
    && expected.jobTitle.length <= 4096 && Number.isSafeInteger(expected.resumeVersionNo)
    && expected.resumeVersionNo > 0, "prep_workspace_expected_snapshot");
  const destination = `${origin}/jobs/applications`;
  const workspaceUrl = `${origin}/api/jobs/workspace`;
  let prepRequests = 0;
  const countPrep = (request) => {
    if (prepPath.test(new URL(request.url()).pathname)) prepRequests += 1;
  };
  page.on("request", countPrep);
  try {
    const [workspaceResponse] = await Promise.all([
      page.waitForResponse((response) => response.url() === workspaceUrl
        && response.request().method() === "GET", { timeout }),
      page.goto(destination, { waitUntil: "domcontentloaded", timeout }),
    ]);
    const workspaceAuthorization = await workspaceResponse.request().headerValue("authorization");
    require(page.url() === destination && workspaceResponse.status() === 200
      && hasBearer(workspaceAuthorization), "prep_workspace_authenticated");
    const bytes = await workspaceResponse.body();
    require(bytes.byteLength <= maxWorkspaceBytes, "prep_workspace_size");
    const workspace = parseJson(bytes, "prep_workspace_json");
    const applications = workspace?.applications;
    require(Array.isArray(applications) && applications.length === 1, "prep_workspace_single_application");
    const application = applications[0];
    const receipt = application?.receipt;
    require(application?.id === expected.applicationId && application.state === "submitted"
      && Number.isSafeInteger(application.submitted_at_ms) && application.submitted_at_ms > 0
      && application.resume_version_id === expected.resumeVersionId
      && receipt?.receiptId === expected.receiptId
      && workspace.interview_prep_receipts?.[expected.applicationId] === expected.receiptFingerprint,
    "prep_workspace_receipt_binding");
    require(receipt.job?.title === expected.jobTitle && receipt.job?.company === expected.company,
      "prep_workspace_frozen_job");
    const receiptResumes = Array.isArray(receipt.documents)
      && receipt.documents.filter((row) => row?.kind === "resume");
    require(receiptResumes?.length === 1 && receiptResumes[0].versionId === expected.resumeVersionId
      && receiptResumes[0].sha256 === expected.resumeDocumentSha256, "prep_workspace_receipt_resume");
    const evidence = workspace.application_evidence;
    const resume = Array.isArray(evidence) && evidence.filter((row) => row?.kind === "resume"
      && row.application_id === expected.applicationId && row.resume_version_id === expected.resumeVersionId);
    require(resume?.length === 1 && resume[0].sha256 === expected.resumeDocumentSha256,
      "prep_workspace_resume_evidence");

    const row = page.locator(".application-row");
    await row.waitFor({ state: "visible", timeout });
    require(await row.count() === 1 && prepRequests === 0, "prep_workspace_selection");
    await row.click({ timeout });
    const openButton = page.getByRole("button", { name: "Prepare interview", exact: true });
    await openButton.waitFor({ state: "visible", timeout });
    require(prepRequests === 0, "prep_workspace_no_eager_request");
    const result = await observeSignedInInterviewPrep({
      page, portalOrigin, openButton, expected, workspaceAuthorization,
    });
    require(prepRequests === 1, "prep_workspace_exact_request_count");
    return { ...result, committedWorkspaceReceiptObserved: true, loginFormObserved: false };
  } finally {
    page.off("request", countPrep);
  }
}
