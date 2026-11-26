import assert from "node:assert/strict";
import test from "node:test";

import { LOCAL_QA_SAMPLE_RESUME } from "./local-qa-browser-sample-resume.mjs";
import { completeLocalQaBrowserSamplePrepare } from "./local-qa-browser-sample-prepare.mjs";

const ORIGIN = "http://127.0.0.1:5191";
const JOB_ID = "01234567-89ab-4cde-8fab-0123456789ab";
const ASSET_ID = "11234567-89ab-4cde-8fab-0123456789ab";
const APPLICATION_ID = "21234567-89ab-4cde-8fab-0123456789ab";
const RESUME_ID = "31234567-89ab-4cde-8fab-0123456789ab";

class Locator {
  constructor(page, key) { this.page = page; this.key = key; }
  filter({ hasText }) { return new Locator(this.page, `${this.key}|${hasText}`); }
  async waitFor() { this.page.actions.push(`visible:${this.key}`); }
  async count() { return this.page.rowCount; }
  async click() {
    this.page.actions.push(`click:${this.key}`);
    if (this.key.includes("Prepare application")) this.page.navigate(`${ORIGIN}/jobs/applications`);
  }
  getByRole(role, options) { return new Locator(this.page, `${this.key}>${role}:${options.name}`); }
  getByText(text) { return new Locator(this.page, `${this.key}>text:${text}`); }
}

class Response {
  constructor(page) { this.page = page; }
  url() { return `${ORIGIN}/api/jobs/applications`; }
  request() {
    return { method: () => "POST", postData: () => this.page.requestBody };
  }
  status() { return this.page.status; }
  json() { return this.page.body; }
}

class Page {
  constructor({ rowCount = 1, status = 200, body = undefined } = {}) {
    this.current = `${ORIGIN}/jobs/matches`;
    this.rowCount = rowCount;
    this.status = status;
    this.actions = [];
    this.waiters = [];
    this.requestBody = JSON.stringify({ job_id: JOB_ID, mode: "factual", submission_mode: "review_first" });
    this.body = body ?? envelope();
  }
  url() { return this.current; }
  locator(selector) { this.actions.push(`locator:${selector}`); return new Locator(this, selector); }
  getByRole(role, options) { return new Locator(this, `${role}:${options.name}`); }
  getByText(text) { return new Locator(this, `text:${text}`); }
  waitForResponse(predicate) {
    const response = new Response(this);
    assert.equal(predicate(response), true);
    this.actions.push("wait:prepare");
    return Promise.resolve(response);
  }
  waitForURL(url) { return new Promise((resolve) => this.waiters.push({ url, resolve })); }
  navigate(url) {
    this.current = url;
    const matches = this.waiters.filter((item) => item.url === url);
    this.waiters = this.waiters.filter((item) => item.url !== url);
    for (const item of matches) item.resolve();
  }
}

function envelope() {
  return {
    application: { id: APPLICATION_ID, job_id: JOB_ID, resume_version_id: RESUME_ID,
      state: "awaiting_review", submission_mode: "review_first", run_id: null,
      submitted_at_ms: null, receipt: {} },
    resume_version: { id: RESUME_ID, job_id: JOB_ID, mode: "factual", content: { provenance: {
      career_track_policy_authority: { source_resume_asset_id: ASSET_ID,
        source_resume_sha256: LOCAL_QA_SAMPLE_RESUME.sha256 },
    } } },
  };
}

test("fake Page double performs one factual review-first Prepare and reaches Applications", async () => {
  const page = new Page();
  const result = await completeLocalQaBrowserSamplePrepare({ page, origin: ORIGIN, jobId: JOB_ID, assetId: ASSET_ID });
  assert.deepEqual(result, { applicationId: APPLICATION_ID, resumeId: RESUME_ID });
  const prepareClicks = page.actions.filter((item) => item.includes("click:") && item.includes("Prepare application"));
  assert.ok(page.actions.indexOf("wait:prepare") < page.actions.indexOf(prepareClicks[0]));
  assert.equal(prepareClicks.length, 1);
  assert.ok(page.actions.some((item) => item.includes("Review first")));
  assert.ok(page.actions.some((item) => item.endsWith("text:Awaiting Review")));
});

test("refuses an ambiguous match row, source mismatch, or submission-capable response", async () => {
  await assert.rejects(
    completeLocalQaBrowserSamplePrepare({ page: new Page({ rowCount: 2 }), origin: ORIGIN, jobId: JOB_ID, assetId: ASSET_ID }),
    /local_qa_browser_sample_prepare_refused:match_row/,
  );
  const body = envelope();
  body.resume_version.content.provenance.career_track_policy_authority.source_resume_asset_id = JOB_ID;
  await assert.rejects(
    completeLocalQaBrowserSamplePrepare({ page: new Page({ body }), origin: ORIGIN, jobId: JOB_ID, assetId: ASSET_ID }),
    /local_qa_browser_sample_prepare_refused:prepare_body/,
  );
  const submitted = envelope();
  submitted.application.submitted_at_ms = 1;
  await assert.rejects(
    completeLocalQaBrowserSamplePrepare({ page: new Page({ body: submitted }), origin: ORIGIN, jobId: JOB_ID, assetId: ASSET_ID }),
    /local_qa_browser_sample_prepare_refused:prepare_body/,
  );
});

test("refuses non-literal loopback and a non-200 envelope", async () => {
  await assert.rejects(
    completeLocalQaBrowserSamplePrepare({ page: new Page(), origin: "http://localhost:5191", jobId: JOB_ID, assetId: ASSET_ID }),
    /local_qa_browser_sample_prepare_refused:origin/,
  );
  await assert.rejects(
    completeLocalQaBrowserSamplePrepare({ page: new Page({ status: 409 }), origin: ORIGIN, jobId: JOB_ID, assetId: ASSET_ID }),
    /local_qa_browser_sample_prepare_refused:prepare_status/,
  );
});
