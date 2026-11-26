import assert from "node:assert/strict";
import { EventEmitter, once } from "node:events";
import { Readable } from "node:stream";
import { test } from "node:test";
import { assertInterviewPrepPracticeExport, assertInterviewPrepWitnessExport, observeReceiptInterviewPrepInPortal,
  observeSignedInInterviewPrep, practiceBlobProbe } from
  "../portal/nonprod/interview-prep-witness.mjs";

const portalOrigin = "http://127.0.0.1:5189";
const expected = { applicationId: "fixture-app", receiptId: "fixture-receipt",
  resumeVersionId: "fixture-resume", resumeChecksum: "a".repeat(64),
  receiptFingerprint: "d".repeat(64),
  jobTitle: "Synthetic data engineer", resumeVersionNo: 1,
  resumeDocumentSha256: "b".repeat(64), company: "Fixture employer", provider: "openai",
  model: "fixture-model", content: "Explain why the business deadline matters.\n\nThen test your assumption." };
const response = { schema_version: 1, id: `prep-${"c".repeat(24)}`, application_id: expected.applicationId,
  content: expected.content, generated_at_ms: 1234, provider: expected.provider, model: expected.model,
  cost_cents: 0, cost_label: null, confidence: null, balance_cents_after: 5, trial_seconds_remaining: 8,
  grounding: { receipt_id: expected.receiptId, receipt_fingerprint: "d".repeat(64),
    resume_version_id: expected.resumeVersionId, resume_checksum: expected.resumeChecksum,
    resume_document_sha256: expected.resumeDocumentSha256, answer_keys_used: [], answer_keys_omitted: [] } };
const workspace = { applications: [{
  id: expected.applicationId, state: "submitted", submitted_at_ms: 100,
  resume_version_id: expected.resumeVersionId, receipt: { receiptId: expected.receiptId,
    job: { title: expected.jobTitle, company: expected.company },
    documents: [{ kind: "resume", versionId: expected.resumeVersionId, sha256: expected.resumeDocumentSha256 }] },
}], interview_prep_receipts: { [expected.applicationId]: expected.receiptFingerprint },
application_evidence: [{ application_id: expected.applicationId, kind: "resume",
  resume_version_id: expected.resumeVersionId, sha256: expected.resumeDocumentSha256 }] };

const draftFor = (reviewed) => ({ schema_version: 1, status: "draft", application_id: expected.applicationId,
  prep_id: response.id, generated_at_ms: response.generated_at_ms, content: expected.content,
  grounding: { ...response.grounding }, generation: { provider: expected.provider, model: expected.model,
    cost_cents: 0, cost_label: null, confidence: null }, ...(reviewed ? {
    candidate_review: { kind: "candidate_self_review", facts_checked: true, reasoning_checked: true },
  } : {}) });

const practiceFor = (reviewed) => Buffer.from([
  "Bluey Jobs | Interview practice draft",
  reviewed ? "You marked the facts and reasoning checked. Bluey has not verified this draft."
    : "Check the facts and reasoning before rehearsing. Bluey has not verified this draft.",
  "", "Rehearse in your own words. Use only examples you can explain and substantiate.", "", expected.content, "",
].join("\n"));

// An event-driven browser double checks contract/control flow only, not real
// Chromium, authority, provider, billing or native download behavior.
function fakePage(overrides = {}) {
  const page = new EventEmitter();
  const endpoint = `${portalOrigin}/api/jobs/applications/${expected.applicationId}/interview-prep`;
  const req = { url: () => endpoint, method: () => "POST",
    postData: () => overrides.body ?? JSON.stringify({ prepare: true, receipt_fingerprint: expected.receiptFingerprint }),
    headerValue: async (name) => name === "authorization" && !overrides.unauthenticated
      ? overrides.prepAuthorization ?? "Bearer synthetic" : null };
  const res = { url: req.url, request: () => req, status: () => 200,
    body: async () => Buffer.from(JSON.stringify(overrides.response ?? response)) };
  const review = { facts: false, reasoning: false };
  let probe = null;
  let checked = false, visible = false, selected = false;
  const push = () => page.emit("request", req);
  const openButton = { count: async () => 1, click: async () => {
    visible = true;
    if (overrides.eager === "open") push();
  }, waitFor: async () => assert(selected) };
  const locators = {
    consent: { isChecked: async () => checked, check: async () => {
      checked = true;
      if (overrides.eager === "toggle") push();
    } },
    generate: { isEnabled: async () => checked, click: async () => {
      push();
      if (overrides.double) push();
      page.emit("response", res);
    } },
    heading: { waitFor: async () => assert(visible) },
    facts: { isChecked: async () => review.facts, check: async () => { review.facts = true; } },
    reasoning: { isChecked: async () => review.reasoning, check: async () => { review.reasoning = true; } },
    download: { click: async () => {
      const payload = draftFor(review.facts && review.reasoning);
      if (overrides.leak) payload.balance_cents_after = response.balance_cents_after;
      const bytes = overrides.largeDownload ? Buffer.alloc(128 * 1024 + 1)
        : Buffer.from(JSON.stringify(payload));
      page.emit("download", { suggestedFilename: () => `bluey-interview-prep-${response.id.slice(5)}.json`,
        createReadStream: async () => Readable.from([bytes]) });
    } },
    practice: { click: async () => {
      assert(probe, "practice download must have a Blob probe");
      const reviewed = review.facts && review.reasoning && !overrides.wrongTextReview;
      let bytes = practiceFor(reviewed);
      if (overrides.textLeak) bytes = Buffer.concat([bytes, Buffer.from(response.grounding.receipt_fingerprint)]);
      if (overrides.textOversize) bytes = Buffer.alloc(128 * 1024 + 1);
      const url = `blob:${portalOrigin}/controlled-fixture`;
      probe = { count: overrides.textObjectCount ?? 1, first: {
        url: overrides.textProbeUrl ?? url, type: overrides.textMime ?? "text/plain;charset=utf-8",
        size: bytes.length + (overrides.textSizeDrift ? 1 : 0),
      } };
      page.emit("download", {
        url: () => url,
        suggestedFilename: () => overrides.textFilename ?? `bluey-interview-prep-${response.id.slice(5)}.txt`,
        createReadStream: async () => Readable.from([bytes]),
      });
    } },
  };
  page.url = () => overrides.url ?? `${portalOrigin}/jobs/applications`;
  page.goto = async (url) => {
    assert.equal(url, `${portalOrigin}/jobs/applications`);
    if (overrides.eager === "navigate") push();
    const workspaceRequest = { url: () => `${portalOrigin}/api/jobs/workspace`, method: () => "GET",
      headerValue: async (name) => name === "authorization" && !overrides.workspaceAnonymous
        ? "Bearer synthetic" : null };
    page.emit("response", { url: workspaceRequest.url, request: () => workspaceRequest,
      status: () => overrides.workspaceStatus ?? 200,
      body: async () => Buffer.from(JSON.stringify(overrides.workspace ?? workspace)) });
  };
  page.locator = (selector) => {
    assert.equal(selector, ".application-row");
    return { waitFor: async () => {}, count: async () => overrides.rowCount ?? 1,
      click: async () => { selected = true; if (overrides.eager === "select") push(); } };
  };
  page.getByRole = (role, { name }) => {
    if (role === "button" && name === "Prepare interview") return openButton;
    assert.equal(role, "dialog");
    assert.equal(name, `Interview prep: ${expected.company}`);
    return {
      waitFor: locators.heading.waitFor,
      getByText: (value, options) => ({ waitFor: async () => {
        assert(options.exact);
        if (value !== (overrides.description ?? `${expected.jobTitle} · Submitted resume v${expected.resumeVersionNo}`)) {
          throw new Error("fixture_dialog_description_refused");
        }
      } }),
      locator: (selector) => {
        assert.equal(selector, "article");
        return { textContent: async () => overrides.visibleText ?? expected.content };
      },
      getByRole: (_role, options) => {
        const names = { "/^Prepare for this interview/": "consent", "Generate with Bluey": "generate",
          "Draft for review": "heading", "/^I checked that personal claims/": "facts",
          "/^I can explain the decisions/": "reasoning", "Download structured draft (.json)": "download",
          "Download practice draft (.txt)": "practice" };
        const locator = locators[names[String(options.name)]];
        assert(locator);
        return locator;
      },
    };
  };
  page.waitForResponse = async (accept) => {
    const [value] = await once(page, "response");
    assert(accept(value));
    return value;
  };
  page.waitForEvent = async (event) => (await once(page, event))[0];
  page.evaluate = async (fn, action) => {
    assert.equal(fn, practiceBlobProbe);
    if (action === "open") {
      assert.equal(probe, null);
      probe = { count: 0, first: null };
    } else if (action === "read") {
      assert(probe);
      return probe;
    } else {
      assert.equal(action, "close");
      assert(probe);
      probe = null;
    }
  };
  return { page, openButton, portalOrigin, expected, probeClosed: () => probe === null };
}

test("positive witness observes one consented request and both formats before and after review", async () => {
  const run = fakePage();
  assert.deepEqual(await observeSignedInInterviewPrep(run),
    { exactGenerateRequests: 1, structuredDraftDownloads: 2, textDraftDownloads: 2, scoped: true });
  assert.equal(run.page.listenerCount("request"), 0);
  assert(run.probeClosed());
});

test("portal witness joins the authenticated workspace receipt to the prep and export", async () => {
  const run = fakePage();
  assert.deepEqual(await observeReceiptInterviewPrepInPortal(run), {
    exactGenerateRequests: 1, structuredDraftDownloads: 2, textDraftDownloads: 2, scoped: true,
    committedWorkspaceReceiptObserved: true, loginFormObserved: false,
  });
  assert.equal(run.page.listenerCount("request"), 0);
});

for (const [title, overrides, code] of [
  ["anonymous workspace", { workspaceAnonymous: true }, "prep_workspace_authenticated"],
  ["non-200 workspace", { workspaceStatus: 403 }, "prep_workspace_authenticated"],
  ["ambiguous applications", { workspace: { ...workspace,
    applications: [...workspace.applications, ...workspace.applications] } }, "prep_workspace_single_application"],
  ["uncommitted application", { workspace: { ...workspace,
    applications: [{ ...workspace.applications[0], submitted_at_ms: null }] } }, "prep_workspace_receipt_binding"],
  ["different finalized receipt", { workspace: { ...workspace,
    applications: [{ ...workspace.applications[0], receipt: { receiptId: "other" } }] } },
  "prep_workspace_receipt_binding"],
  ["drifted frozen job", { workspace: { ...workspace, applications: [{ ...workspace.applications[0],
    receipt: { ...workspace.applications[0].receipt, job: { title: expected.jobTitle, company: "other" } },
  }] } }, "prep_workspace_frozen_job"],
  ["drifted receipt document", { workspace: { ...workspace, applications: [{ ...workspace.applications[0],
    receipt: { ...workspace.applications[0].receipt, documents: [{ kind: "resume",
      versionId: expected.resumeVersionId, sha256: "0".repeat(64) }] },
  }] } }, "prep_workspace_receipt_resume"],
  ["stale workspace fingerprint", { workspace: { ...workspace,
    interview_prep_receipts: { [expected.applicationId]: "0".repeat(64) } } }, "prep_workspace_receipt_binding"],
  ["evidence from a different resume", { workspace: { ...workspace,
    application_evidence: [{ ...workspace.application_evidence[0], resume_version_id: "other" }] } },
  "prep_workspace_resume_evidence"],
  ["ambiguous visible rows", { rowCount: 2 }, "prep_workspace_selection"],
  ["generation on navigation", { eager: "navigate" }, "prep_workspace_selection"],
  ["generation on selecting application", { eager: "select" }, "prep_workspace_no_eager_request"],
  ["unrelated displayed role", { description: "Other role · Submitted resume v1" }, "fixture_dialog_description_refused"],
  ["generation under a different session", { prepAuthorization: "Bearer other" }, "prep_authenticated_request"],
]) {
  test(`portal witness refuses ${title}`, async () => {
    const run = fakePage(overrides);
    await assert.rejects(observeReceiptInterviewPrepInPortal(run), { message: code });
    assert.equal(run.page.listenerCount("request"), 0);
  });
}

for (const [title, overrides, code] of [
  ["preview", { url: `${portalOrigin}/jobs/applications?preview=1` }, "prep_owned_portal"],
  ["open issues a generation", { eager: "open" }, "prep_initial_consent"],
  ["toggle issues a generation", { eager: "toggle" }, "prep_toggle_no_request"],
  ["wrong exact opt-in body", { body: '{"prepare":false}' }, "prep_exact_request_count"],
  ["old opt-in without a receipt version", { body: '{"prepare":true}' }, "prep_exact_request_count"],
  ["unauthenticated success", { unauthenticated: true }, "prep_authenticated_request"],
  ["duplicate generation", { double: true }, "prep_exact_request_count"],
  ["stale submitted resume digest", { response: { ...response,
    grounding: { ...response.grounding, resume_document_sha256: "e".repeat(64) } } }, "prep_response_receipt_binding"],
  ["wrong visible draft", { visibleText: "unrelated scope" }, "prep_visible_content"],
  ["billing field leaked into download", { leak: true }, "prep_download_projection"],
  ["oversized download", { largeDownload: true }, "prep_download_size"],
  ["incorrect text filename", { textFilename: "out.json" }, "prep_text_download_filename"],
  ["oversized text download", { textOversize: true }, "prep_download_size"],
  ["different text Blob URL", { textProbeUrl: "blob:http://127.0.0.1:5189/other" }, "prep_text_blob_binding"],
  ["wrong text MIME", { textMime: "application/json" }, "prep_text_blob_binding"],
  ["wrong text Blob size", { textSizeDrift: true }, "prep_text_blob_binding"],
  ["extra text object URL", { textObjectCount: 2 }, "prep_text_blob_binding"],
  ["receipt metadata appended to text", { textLeak: true }, "prep_text_download_projection"],
  ["unreviewed note after both reviews", { wrongTextReview: true }, "prep_text_download_projection"],
]) {
  test(`positive witness refuses ${title}`, async () => {
    const run = fakePage(overrides);
    await assert.rejects(observeSignedInInterviewPrep(run), { message: code });
    assert.equal(run.page.listenerCount("request"), 0);
    assert(run.probeClosed());
  });
}

test("candidate self-review cannot be changed into server approval or appear by default", () => {
  const payload = draftFor(true);
  assert.throws(() => assertInterviewPrepWitnessExport(payload, response, expected, false),
    { message: "prep_download_projection" });
  payload.candidate_review.kind = "server_approved";
  assert.throws(() => assertInterviewPrepWitnessExport(payload, response, expected, true),
    { message: "prep_download_projection" });
});

test("text exact bytes rule out invalid UTF-8 and review metadata", () => {
  assert.doesNotThrow(() => assertInterviewPrepPracticeExport(practiceFor(false), response, expected, false));
  assert.throws(() => assertInterviewPrepPracticeExport(practiceFor(true), response, expected, false),
    { message: "prep_text_download_projection" });
  assert.throws(() => assertInterviewPrepPracticeExport(Buffer.from([0xff]), response, expected, false),
    { message: "prep_text_download_projection" });
});

test("Blob probe leaves creation native, records only metadata and restores the original descriptor", () => {
  const original = Object.getOwnPropertyDescriptor(URL, "createObjectURL");
  practiceBlobProbe("open");
  let url;
  try {
    const blob = new Blob(["hé"], { type: "text/plain;charset=utf-8" });
    url = URL.createObjectURL(blob);
    assert.deepEqual(practiceBlobProbe("read"), { count: 1, first: { url, type: blob.type, size: 3 } });
    assert.throws(() => practiceBlobProbe("open"), { message: "prep_text_probe_already_active" });
  } finally {
    if (url) URL.revokeObjectURL(url);
    practiceBlobProbe("close");
  }
  assert.deepEqual(Object.getOwnPropertyDescriptor(URL, "createObjectURL"), original);
});
