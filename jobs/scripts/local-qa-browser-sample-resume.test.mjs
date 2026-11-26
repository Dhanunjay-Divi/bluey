import assert from "node:assert/strict";
import test from "node:test";

import {
  LOCAL_QA_SAMPLE_RESUME,
  completeLocalQaBrowserSampleResume,
  createLocalQaSampleResume,
} from "./local-qa-browser-sample-resume.mjs";

const ORIGIN = "http://127.0.0.1:5191";
const JOBS_URL = `${ORIGIN}/jobs/`;
const ASSET_ID = "01234567-89ab-4cde-8fab-0123456789ab";

class FakeResponse {
  url() { return `${ORIGIN}/api/jobs/resume-source`; }
  request() { return { method: () => "POST" }; }
  status() { return 200; }
  json() {
    return {
      asset: {
        id: ASSET_ID,
        file_name: LOCAL_QA_SAMPLE_RESUME.fileName,
        media_type: LOCAL_QA_SAMPLE_RESUME.mimeType,
        file_type: "txt",
        template_status: "text_only",
        sha256: LOCAL_QA_SAMPLE_RESUME.sha256,
        size_bytes: createLocalQaSampleResume().length,
      },
      profile: {
        ...LOCAL_QA_SAMPLE_RESUME.expectedProfile,
        source_resume_asset_id: ASSET_ID,
      },
    };
  }
}

class FakeLocator {
  constructor(page, key) { this.page = page; this.key = key; }
  async waitFor() { this.page.actions.push(`visible:${this.key}`); }
  async setInputFiles(file) {
    this.page.actions.push(`file:${this.key}`);
    assert.equal(file.name, LOCAL_QA_SAMPLE_RESUME.fileName);
    assert.equal(file.mimeType, LOCAL_QA_SAMPLE_RESUME.mimeType);
    assert.equal(Buffer.compare(file.buffer, createLocalQaSampleResume()), 0);
  }
  async click() { this.page.actions.push(`click:${this.key}`); }
  async inputValue() { this.page.actions.push(`value:${this.key}`); return LOCAL_QA_SAMPLE_RESUME.name; }
  getByText(name) { return new FakeLocator(this.page, `text:${name}`); }
  getByRole(role, options) { return new FakeLocator(this.page, `${role}:${options.name}`); }
}

class FakePage {
  constructor({ body = undefined } = {}) { this.actions = []; this.body = body; }
  url() { return JOBS_URL; }
  getByRole(role, options) {
    this.actions.push(`role:${role}:${options.name}`);
    return new FakeLocator(this, `${role}:${options.name}`);
  }
  getByText(name) { return new FakeLocator(this, `text:${name}`); }
  getByLabel(name) { return new FakeLocator(this, `label:${name}`); }
  locator(selector) { this.actions.push(`locator:${selector}`); return new FakeLocator(this, selector); }
  waitForResponse(predicate) {
    const response = new FakeResponse();
    if (this.body) response.json = () => this.body;
    assert.equal(predicate(response), true);
    this.actions.push("wait:upload");
    return Promise.resolve(response);
  }
}

test("fake Page double imports the exact in-memory synthetic resume", async () => {
  const first = createLocalQaSampleResume();
  const second = createLocalQaSampleResume();
  assert.notEqual(first, second);
  assert.equal(Buffer.compare(first, second), 0);

  const page = new FakePage();
  const result = await completeLocalQaBrowserSampleResume({ page, origin: ORIGIN });
  assert.deepEqual(result, { assetId: ASSET_ID });
  assert.ok(page.actions.indexOf("wait:upload") < page.actions.indexOf("click:button:Replace resume facts"));
  assert.ok(page.actions.includes("visible:text:Baseline extracted"));
  assert.ok(page.actions.includes("value:label:Full name"));
});

test("fake Page double refuses a bad stored profile binding", async () => {
  const response = new FakeResponse().json();
  response.profile.source_resume_asset_id = "11111111-1111-4111-8111-111111111111";
  await assert.rejects(
    completeLocalQaBrowserSampleResume({ page: new FakePage({ body: response }), origin: ORIGIN }),
    /local_qa_browser_sample_resume_refused:upload_profile/,
  );
});

test("fake Page double refuses non-literal loopback origins", async () => {
  await assert.rejects(
    completeLocalQaBrowserSampleResume({ page: new FakePage(), origin: "http://localhost:5191" }),
    /local_qa_browser_sample_resume_refused:origin/,
  );
});
