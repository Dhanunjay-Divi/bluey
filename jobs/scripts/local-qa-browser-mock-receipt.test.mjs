import assert from "node:assert/strict";
import test from "node:test";

import { observeLocalQaBrowserMockReceipt } from "./local-qa-browser-mock-receipt.mjs";
import { LOCAL_QA_SAMPLE_RESUME } from "./local-qa-browser-sample-resume.mjs";

const ORIGIN = "http://127.0.0.1:5191";
const expected = Object.freeze({
  accountId: "01234567-89ab-4cde-8fab-0123456789ab",
  applicationId: "11234567-89ab-4cde-8fab-0123456789ab",
  jobId: "synthetic-data-engineer-job",
  resumeId: "21234567-89ab-4cde-8fab-0123456789ab",
  runId: "synthetic-managed-run-1",
  sourceAssetId: "31234567-89ab-4cde-8fab-0123456789ab",
  sourceSha256: LOCAL_QA_SAMPLE_RESUME.sha256,
  receiptObjectSha256: "b".repeat(64),
  submissionFingerprintSha256: "c".repeat(64),
});

function workspace(extra = {}) {
  return {
    profile: { ...LOCAL_QA_SAMPLE_RESUME.expectedProfile, source_resume_asset_id: expected.sourceAssetId,
      source_resume_sha256: expected.sourceSha256, source_resume_name: LOCAL_QA_SAMPLE_RESUME.fileName,
      source_resume_media_type: LOCAL_QA_SAMPLE_RESUME.mimeType, onboarding_complete: true,
      employment: [{ id: "41234567-89ab-4cde-8fab-0123456789ab", company: "Bluey QA Systems",
        title: "Data Engineer", current: true }], skills: ["Python", "SQL"] },
    applications: [{ id: expected.applicationId, job_id: expected.jobId, resume_version_id: expected.resumeId,
      state: "submitted", submission_mode: "review_first", run_id: expected.runId, submitted_at_ms: 1,
      receipt: { accountId: expected.accountId, applicationId: expected.applicationId,
        _bluey_server_submission_fingerprint_v1: expected.submissionFingerprintSha256,
        receiptObject: { sha256: expected.receiptObjectSha256 },
        result: { status: "submitted", confirmationText: "Synthetic receipt confirmed" } } }],
    ...extra,
  };
}

function resume(extra = {}) {
  return { id: expected.resumeId, job_id: expected.jobId, mode: "factual", content: { provenance: {
    career_track_policy_authority: { source_resume_asset_id: expected.sourceAssetId, source_resume_sha256: expected.sourceSha256 },
  } }, ...extra };
}

class Locator {
  constructor(page, key) { this.page = page; this.key = key; }
  async waitFor() { this.page.actions.push(`visible:${this.key}`); }
  async count() { return this.page.rows; }
  locator(selector) { return new Locator(this.page, `${this.key}>${selector}`); }
  getByText(text) { return new Locator(this.page, `${this.key}>text:${text}`); }
  async click() { this.page.actions.push(`click:${this.key}`); this.page.emitResume(); }
}

class Response {
  constructor(path, value) { this.path = path; this.value = value; }
  url() { return `${ORIGIN}${this.path}`; }
  request() { return { method: () => "GET" }; }
  status() { return 200; }
  json() { return this.value; }
}

class Page {
  constructor({ workspaceValue = workspace(), resumeValue = resume(), rows = 1 } = {}) {
    this.current = `${ORIGIN}/jobs/applications`; this.workspaceValue = workspaceValue;
    this.resumeValue = resumeValue; this.rows = rows; this.actions = []; this.workspaceWaiters = []; this.resumeWaiters = [];
  }
  url() { return this.current; }
  locator(selector) { return new Locator(this, selector); }
  getByText(text) { return new Locator(this, `text:${text}`); }
  waitForResponse(predicate) {
    const workspaceResponse = new Response("/api/jobs/workspace", this.workspaceValue);
    if (predicate(workspaceResponse)) return new Promise(resolve => this.workspaceWaiters.push(resolve));
    const resumeResponse = new Response(`/api/jobs/resume-versions/${expected.resumeId}`, this.resumeValue);
    assert.equal(predicate(resumeResponse), true);
    return new Promise(resolve => this.resumeWaiters.push(resolve));
  }
  async reload() { this.actions.push("reload"); for (const resolve of this.workspaceWaiters.splice(0)) resolve(new Response("/api/jobs/workspace", this.workspaceValue)); }
  emitResume() { for (const resolve of this.resumeWaiters.splice(0)) resolve(new Response(`/api/jobs/resume-versions/${expected.resumeId}`, this.resumeValue)); }
}

test("observes one submitted tracker row only after authoritative workspace and resume reads", async () => {
  const page = new Page();
  assert.deepEqual(await observeLocalQaBrowserMockReceipt({ page, origin: ORIGIN, expected }), {
    applicationCount: 1, submittedCount: 1, mockReceiptObserved: true,
  });
  assert.ok(page.actions.indexOf("reload") < page.actions.findIndex(item => item.startsWith("click:")));
  assert.ok(page.actions.some(item => item.endsWith("text:Submitted")));
});

test("refuses wrong application, resume, submitted state, absent app, or an extra draft", async () => {
  const wrongApp = workspace(); wrongApp.applications[0].id = expected.accountId;
  const wrongResume = resume(); wrongResume.id = expected.accountId;
  const wrongState = workspace(); wrongState.applications[0].state = "awaiting_review";
  const absent = workspace({ applications: [] });
  const extra = workspace({ applications: [...workspace().applications, { ...workspace().applications[0], id: expected.accountId, state: "awaiting_review" }] });
  for (const page of [new Page({ workspaceValue: wrongApp }), new Page({ resumeValue: wrongResume }),
    new Page({ workspaceValue: wrongState }), new Page({ workspaceValue: absent }), new Page({ workspaceValue: extra })]) {
    await assert.rejects(observeLocalQaBrowserMockReceipt({ page, origin: ORIGIN, expected }), /local_qa_browser_mock_receipt_refused/);
  }
});

test("refuses nonliteral origin and missing finalized receipt proof", async () => {
  await assert.rejects(observeLocalQaBrowserMockReceipt({ page: new Page(), origin: "http://localhost:5191", expected }),
    /local_qa_browser_mock_receipt_refused:origin/);
  const unfinalized = workspace(); delete unfinalized.applications[0].receipt.receiptObject;
  await assert.rejects(observeLocalQaBrowserMockReceipt({ page: new Page({ workspaceValue: unfinalized }), origin: ORIGIN, expected }),
    /local_qa_browser_mock_receipt_refused:workspace_body/);
});
