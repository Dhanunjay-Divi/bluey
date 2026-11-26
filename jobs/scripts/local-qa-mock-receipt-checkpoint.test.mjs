import assert from "node:assert/strict";
import test from "node:test";
import { LOCAL_QA_SAMPLE_RESUME as sample } from "./local-qa-browser-sample-resume.mjs";
import { createOwnedQaBrowserMockReceiptCheckpoint, createOwnedQaMockReceiptCheckpoint }
  from "./local-qa-mock-receipt-checkpoint.mjs";

const id = suffix => `00000000-0000-4000-8000-${String(suffix).padStart(12, "0")}`;
const draft = Object.freeze({ accountId: id(1), applicationId: id(2), jobId: "synthetic-posting-data-engineer", resumeId: id(4),
  sourceAssetId: id(5), sourceSha256: sample.sha256 });
const worker = Object.freeze({ ...draft, runId: "owned-synthetic-run", receiptObjectSha256: "1".repeat(64),
  submissionFingerprintSha256: "2".repeat(64) });
const observed = Object.freeze({ applicationCount: 1, submittedCount: 1, mockReceiptObserved: true });
const fingerprint = "3".repeat(64);

function snapshot(submitted) {
  const row = { id: draft.applicationId, job_id: draft.jobId, resume_version_id: draft.resumeId,
    state: submitted ? "submitted" : "awaiting_review", submission_mode: "review_first",
    run_id: submitted ? worker.runId : null, submitted_at_ms: submitted ? 100 : null,
    receipt: submitted ? { applicationId: draft.applicationId, accountId: draft.accountId,
      result: { status: "submitted", confirmationText: "Synthetic confirmation" },
      receiptObject: { sha256: worker.receiptObjectSha256 },
      _bluey_server_submission_fingerprint_v1: worker.submissionFingerprintSha256 }
      : { confirmation: null, resume_version_id: draft.resumeId, job_snapshot_fingerprint: fingerprint } };
  return {
    account: { id: draft.accountId, email: sample.candidateEmail, is_admin: false, is_temporary: false },
    workspace: { profile: { ...sample.expectedProfile, source_resume_asset_id: draft.sourceAssetId,
      employment: [{ id: id(6), company: "Bluey QA Systems", title: "Data Engineer", current: true }],
      skills: ["Python", "SQL"] }, applications: [row], auto_submit_authorizations: [], browser_sessions: [] },
    resume: { id: draft.resumeId, job_id: draft.jobId, mode: "factual", content: { provenance: {
      generated_for_job_id: draft.jobId, job_snapshot_fingerprint: fingerprint,
      career_track_policy_authority: { source_resume_asset_id: draft.sourceAssetId, source_resume_sha256: sample.sha256 },
    } } },
    census: { accounts: 1, profiles: 1, applications: 1, resumes: 1, prepared: submitted ? 0 : 1,
      factual_draft_links: submitted ? 0 : 1, submitted: submitted ? 1 : 0, auto_submit_authorizations: 0,
      communication_actions: 0, browser_sessions: 0, attempts: submitted ? 1 : 0, commands: 0, execution_leases: 0,
      irreversible_receipts: 0, workflow_executions: 0, metering: 0 },
  };
}

function scenario({ before = snapshot(false), after = snapshot(true), result = worker, browser = observed,
  owned = () => true, publish = () => result } = {}) {
  const calls = [];
  let reads = 0;
  const run = createOwnedQaMockReceiptCheckpoint({ assertOwned: owned, async readOwnedState(input) {
    assert.deepEqual(input, draft);
    assert.equal(Object.isFrozen(input), true);
    calls.push("independent-read");
    return reads++ === 0 ? before : after;
  }, async runMockWorker(input) {
    assert.deepEqual(input, draft);
    calls.push("worker");
    return publish();
  }, async observeInBrowser(expected) {
    assert.deepEqual(expected, worker);
    assert.equal(Object.isFrozen(expected), true);
    calls.push("browser");
    return browser;
  } });
  return { run, calls };
}

test("requires draft, one worker, independent durable receipt, then the still-open browser", async () => {
  const { run, calls } = scenario();
  assert.deepEqual(await run(draft), observed);
  assert.deepEqual(calls, ["independent-read", "worker", "independent-read", "browser"]);
  await assert.rejects(run(draft), /already_attempted/);
  assert.deepEqual(calls, ["independent-read", "worker", "independent-read", "browser"]);
});

test("wrong candidate, stale résumé, duplicate application, existing approval, or any auto grant stops before the worker", async () => {
  for (const change of [before => { before.account.id = id(7); }, before => { before.resume.id = id(7); },
    before => { before.workspace.applications.push(before.workspace.applications[0]); },
    before => { before.workspace.applications[0].receipt.approved_execution = {}; },
    before => { before.census.auto_submit_authorizations = 1; },
    before => { before.census.execution_leases = 1; }]) {
    const before = snapshot(false);
    change(before);
    const { run, calls } = scenario({ before });
    await assert.rejects(run(draft), /candidate_readback|owned_readback|unapproved_draft/);
    assert.deepEqual(calls, ["independent-read"]);
  }
});

test("a malformed worker result or uncertain worker failure cannot be retried or shown", async () => {
  for (const changed of [{ accountId: id(8) }, { resumeId: id(9) }, { sourceSha256: "0".repeat(64) }, { workerToken: "redacted" }]) {
    const { run, calls } = scenario({ result: { ...worker, ...changed } });
    await assert.rejects(run(draft), /worker_result/);
    await assert.rejects(run(draft), /already_attempted/);
    assert.deepEqual(calls, ["independent-read", "worker"]);
  }
  const { run, calls } = scenario({ publish() { throw new Error("untrusted worker payload"); } });
  await assert.rejects(run(draft), /^Error: local_qa_mock_receipt_checkpoint_refused:worker_failed$/);
  await assert.rejects(run(draft), /already_attempted/);
  assert.deepEqual(calls, ["independent-read", "worker"]);
});

test("a missing persisted receipt, an unrelated or changed packet, or no browser observation never passes", async () => {
  for (const change of [after => { after.census.submitted = 0; },
    after => { after.census.attempts = 2; },
    after => { delete after.workspace.applications[0].receipt.result.confirmationText; },
    after => { after.resume.content.provenance.job_snapshot_fingerprint = "5".repeat(64); },
    after => { after.workspace.applications[0].receipt.receiptObject.sha256 = "6".repeat(64); }]) {
    const after = snapshot(true);
    change(after);
    const { run, calls } = scenario({ after });
    await assert.rejects(run(draft), /committed_receipt/);
    assert.deepEqual(calls, ["independent-read", "worker", "independent-read"]);
  }
  const { run } = scenario({ browser: { ...observed, mockReceiptObserved: false } });
  await assert.rejects(run(draft), /browser_receipt/);
});

test("losing browser/API/database ownership during the stage stops further operations", async () => {
  let checks = 0;
  const { run, calls } = scenario({ owned: () => ++checks <= 2 });
  await assert.rejects(run(draft), /ownership/);
  assert.deepEqual(calls, ["independent-read", "worker"]);
});

test("the browser composition refuses an unowned address or wrong page before invoking a worker", async () => {
  let workerCalls = 0;
  const input = { page: { url: () => "http://127.0.0.1:6194/jobs/matches", reload() {}, waitForResponse() {}, locator() {} },
    assertOwned: () => true,
    readOwnedState: () => snapshot(false), runMockWorker: () => { workerCalls++; return worker; } };
  assert.throws(() => createOwnedQaBrowserMockReceiptCheckpoint({ ...input, origin: "http://localhost:6194" }),
    /browser_binding/);
  const run = createOwnedQaBrowserMockReceiptCheckpoint({ ...input, origin: "http://127.0.0.1:6194" });
  await assert.rejects(run(draft), /ownership/);
  assert.equal(workerCalls, 0);
});

test("accepts a safe synthetic posting ID but refuses a URL or query in the ID slot", async () => {
  for (const jobId of ["https://example.test/posting", "synthetic?id=1", "synthetic\nposting"]) {
    const { run, calls } = scenario();
    await assert.rejects(run({ ...draft, jobId }), /:input/);
    assert.deepEqual(calls, []);
  }
});

test("the composed checkpoint observes the still-signed-in browser after the real receipt surface", async () => {
  const origin = "http://127.0.0.1:6194";
  const scope = [];
  const submitted = snapshot(true);
  let persisted = false;
  let waiter;
  const locator = { waitFor: async () => {}, count: async () => 1,
    locator: () => locator, getByText: () => locator,
    click: async () => emit(`/api/jobs/resume-versions/${draft.resumeId}`, submitted.resume) };
  function emit(path, body) {
    const response = { url: () => origin + path, request: () => ({ method: () => "GET" }),
      status: () => 200, json: async () => body };
    assert.equal(waiter.predicate(response), true);
    return waiter.resolve(response);
  }
  const page = { url: () => `${origin}/jobs/applications`, locator: () => locator,
    waitForResponse: predicate => new Promise(resolve => { waiter = { predicate, resolve }; }),
    reload: async () => {
      assert.equal(persisted, true);
      scope.push("browser_reload");
      emit("/api/jobs/workspace", submitted.workspace);
    } };
  const complete = createOwnedQaBrowserMockReceiptCheckpoint({ page, origin, assertOwned: () => true,
    readOwnedState: () => (scope.push("owned_read"), persisted ? submitted : snapshot(false)),
    runMockWorker: input => {
      assert.deepEqual(input, draft);
      scope.push("mock");
      persisted = true;
      return worker;
    } });
  assert.deepEqual(await complete(draft), observed);
  assert.deepEqual(scope, ["owned_read", "mock", "owned_read", "browser_reload"]);
});
