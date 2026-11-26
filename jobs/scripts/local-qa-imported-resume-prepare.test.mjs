import assert from "node:assert/strict";
import test from "node:test";
import {
  createLocalQaImportedResumeMatch,
  requireImportedQaPrepared,
} from "./local-qa-imported-resume-prepare.mjs";
import { createLocalQaSampleResume, LOCAL_QA_SAMPLE_RESUME as sample } from "./local-qa-browser-sample-resume.mjs";
import {
  assertLocalQaMockReceiptResume,
  bindLocalQaMockReceiptReadback,
  requireLocalQaMockReceiptEvidence,
  runLocalQaDarwinMockReceiptJourney,
} from "./local-qa-darwin-mock-receipt-journey.mjs";

const accountId = "11111111-1111-4111-8111-111111111111";
const assetId = "22222222-2222-4222-8222-222222222222";
const identityId = "33333333-3333-4333-8333-333333333333";
const employmentId = "44444444-4444-4444-8444-444444444444";
const applicationId = "55555555-5555-4555-8555-555555555555";
const resumeId = "66666666-6666-4666-8666-666666666666";
const jobId = "qa-imported-resume-data-engineer";
const fingerprint = "a".repeat(64);

const clone = value => structuredClone(value);

function importedProfile(overrides = {}) {
  return {
    ...sample.expectedProfile,
    source_resume_asset_id: assetId,
    onboarding_complete: false,
    employment: [{ id: employmentId, company: "Bluey QA Systems", title: "Data Engineer", current: true }],
    skills: ["Python", "SQL", "PostgreSQL"],
    ...overrides,
  };
}

function workspace(profile = importedProfile()) {
  return {
    profile,
    application_identities: [{ id: identityId, email: sample.candidateEmail,
      verification_status: "verified", is_default: true }],
    applications: [], matches: [], tracks: [], browser_sessions: [], auto_submit_authorizations: [],
    entitlement: { plan: "free", local_browser: false, cloud_browser: false, used_packets: 0 },
  };
}

function matchFixture({ mutateSaved, mutateMatched, eligibility = { can_prepare: true } } = {}) {
  const state = workspace();
  const calls = [];
  const run = createLocalQaImportedResumeMatch({
    async readWorkspace() {
      calls.push("workspace");
      return clone(state);
    },
    async readTaxonomy() {
      calls.push("taxonomy");
      return { taxonomyVersion: "fixture-v1", taxonomySha256: fingerprint };
    },
    async saveOnboarding(body, headers) {
      calls.push("onboarding");
      assert.deepEqual(headers, { "x-bluey-jobs-taxonomy-version": "fixture-v1",
        "x-bluey-jobs-taxonomy-sha256": fingerprint });
      assert.equal(body.profile.source_resume_asset_id, assetId);
      assert.equal(body.profile.source_resume_sha256, sample.sha256);
      assert.deepEqual(body.preferences.employment_types, ["full_time"]);
      assert.deepEqual(body.preferences.engagement_types, ["w2"]);
      assert.equal(body.profile.resume_mode, "factual");
      assert.equal(body.profile.default_submission_mode, "review_first");
      state.profile = { ...body.profile, onboarding_complete: true };
      state.preferences = clone(body.preferences);
      state.tracks = [clone(body.track)];
      mutateSaved?.(state);
    },
    async seedPosting(body) {
      calls.push("seed");
      assert(Object.isFrozen(body));
      assert.deepEqual(body, { accountId, trackId: state.tracks[0].id, sourceResumeAssetId: assetId,
        sourceResumeSha256: sample.sha256 });
      state.matches = [{ id: jobId, title: "Data Engineer", company: "Synthetic Qualification Employer",
        track_id: body.trackId, eligibility: clone(eligibility) }];
      mutateMatched?.(state);
      return jobId;
    },
  });
  return { calls, run, state };
}

test("retains the browser-imported source facts and creates one factual review-first match", async () => {
  const { run, calls, state } = matchFixture();
  assert.deepEqual(await run({ accountId, assetId }), { jobId });
  assert.deepEqual(calls, ["workspace", "taxonomy", "onboarding", "workspace", "seed", "workspace"]);
  assert.equal(state.profile.source_resume_asset_id, assetId);
  assert.equal(state.profile.source_resume_sha256, sample.sha256);
  assert.equal(state.profile.source_resume_name, sample.fileName);
  assert.deepEqual(state.profile.employment, [{ id: employmentId, company: "Bluey QA Systems",
    title: "Data Engineer", current: true }]);
  assert.deepEqual(state.profile.skills, ["Python", "SQL", "PostgreSQL"]);
  assert.equal(state.tracks.length, 1);
  assert.equal(state.tracks[0].application_identity_id, identityId);
  assert.deepEqual(state.tracks[0].policy.relevant_employment_ids, [employmentId]);
});

test("refuses source-asset drift before seed and source facts drift before returning the match", async () => {
  for (const fixture of [
    { mutateSaved: state => { state.profile.source_resume_asset_id = "77777777-7777-4777-8777-777777777777"; } },
    { mutateMatched: state => { state.profile.source_resume_sha256 = "b".repeat(64); } },
    { mutateMatched: state => { state.profile.employment[0].title = "Different Role"; } },
  ]) {
    const { run, calls } = matchFixture(fixture);
    await assert.rejects(run({ accountId, assetId }), /local_qa_imported_resume_prepare_refused:(onboarding_binding|current_import)/);
    assert.equal(calls.filter(call => call === "seed").length, fixture.mutateSaved ? 0 : 1);
  }
});

test("requires Prepare authority and an exactly bound imported-resume match", async () => {
  for (const fixture of [
    { eligibility: { can_prepare: false } },
    { mutateMatched: state => { state.matches[0].track_id = "wrong-track"; } },
  ]) {
    const { run, calls } = matchFixture(fixture);
    await assert.rejects(run({ accountId, assetId }), /local_qa_imported_resume_prepare_refused:match_binding/);
    assert.deepEqual(calls, ["workspace", "taxonomy", "onboarding", "workspace", "seed", "workspace"]);
  }
});

function preparedFixture(overrides = {}) {
  const row = {
    id: applicationId, job_id: jobId, resume_version_id: resumeId, state: "awaiting_review",
    submission_mode: "review_first", run_id: null, submitted_at_ms: null,
    receipt: { resume_version_id: resumeId, confirmation: null, job_snapshot_fingerprint: fingerprint },
  };
  const resume = { id: resumeId, job_id: jobId, mode: "factual", content: { provenance: {
    generated_for_job_id: jobId, job_snapshot_fingerprint: fingerprint,
    career_track_policy_authority: { source_resume_asset_id: assetId, source_resume_sha256: sample.sha256 },
  } } };
  const current = workspace({ ...importedProfile(), onboarding_complete: true });
  current.applications = [row];
  Object.assign(current, overrides.workspace);
  if (overrides.row) Object.assign(row, overrides.row);
  if (overrides.receipt) Object.assign(row.receipt, overrides.receipt);
  if (overrides.resume) Object.assign(resume, overrides.resume);
  return { workspace: current, resume };
}

test("accepts only one unapproved factual review-first draft bound to the imported asset", () => {
  const { workspace: current, resume } = preparedFixture();
  assert.doesNotThrow(() => requireImportedQaPrepared({ workspace: current, resume, jobId, assetId,
    applicationId, resumeId }));
});

test("rejects submitted, queued, approved, or source-mismatched draft evidence", () => {
  for (const fixture of [
    { row: { state: "queued" } },
    { row: { submitted_at_ms: 1 } },
    { receipt: { confirmation: { id: "not-allowed" } } },
    { receipt: { approved_execution: {} } },
    { resume: { content: { provenance: { generated_for_job_id: jobId, job_snapshot_fingerprint: fingerprint,
      career_track_policy_authority: { source_resume_asset_id: assetId, source_resume_sha256: "b".repeat(64) } } } } },
  ]) {
    const { workspace: current, resume } = preparedFixture(fixture);
    assert.throws(() => requireImportedQaPrepared({ workspace: current, resume, jobId, assetId, applicationId, resumeId }),
      /local_qa_imported_resume_prepare_refused:prepared_binding/);
  }
});

function darwinJourney(fault) {
  const state = workspace();
  const readback = preparedFixture();
  const calls = [];
  const digest = "b".repeat(64), runId = "review-first-mock-fixture";
  const input = {
    accountId,
    async call(path, options) {
      calls.push(path);
      switch (path) {
        case "/api/jobs/resume-source":
          assert.equal(options.method, "POST");
          assert.equal(options.body.file_name, sample.fileName);
          assert.deepEqual(Buffer.from(options.body.bytes_base64, "base64"), createLocalQaSampleResume());
          assert.equal(options.body.profile.employment[0].company, "Bluey QA Systems");
          state.profile = { ...options.body.profile, ...sample.expectedProfile,
            source_resume_asset_id: assetId, onboarding_complete: false };
          return { profile: clone(state.profile), asset: { id: assetId, file_name: sample.fileName,
            media_type: sample.mimeType, file_type: "txt", template_status: "text_only", sha256: sample.sha256,
            size_bytes: createLocalQaSampleResume().length } };
        case "/api/jobs/workspace":
          assert.equal(options, undefined);
          return clone(state);
        case "/api/jobs/taxonomy":
          assert.equal(options, undefined);
          return { taxonomyVersion: "fixture-v1", taxonomySha256: fingerprint };
        case "/api/jobs/onboarding/complete":
          assert.equal(options.method, "POST");
          state.profile = { ...options.body.profile, onboarding_complete: true };
          state.tracks = [clone(options.body.track)];
          return {};
        case "/api/jobs/applications":
          assert.equal(options.method, "POST");
          assert.deepEqual(options.body, { job_id: jobId, mode: "factual", submission_mode: "review_first" });
          state.applications = clone(readback.workspace.applications);
          return { application: clone(state.applications[0]), resume_version: clone(readback.resume) };
        case `/api/jobs/resume-versions/${resumeId}`:
          assert.equal(options, undefined);
          return clone(readback.resume);
        default:
          assert.fail(`unexpected route: ${path}`);
      }
    },
    async seedPosting(binding) {
      assert.deepEqual(binding, { accountId, trackId: state.tracks[0].id,
        sourceResumeAssetId: assetId, sourceResumeSha256: sample.sha256 });
      state.matches = [{ id: jobId, title: "Data Engineer", company: "Synthetic Qualification Employer",
        track_id: binding.trackId, eligibility: { can_prepare: true } }];
      if (fault !== "plan") state.entitlement = { plan: "cloud", local_browser: false,
        cloud_browser: fault === "distribution", used_packets: fault === "spent" ? 1 : 0 };
      if (fault === "active") state.browser_sessions.push({});
      return jobId;
    },
    async readCensus(_draft, phase, expectedRun) {
      calls.push(phase);
      const submitted = phase === "submitted" ? 1 : 0;
      assert.equal(expectedRun, submitted ? runId : null);
      return { accounts: 1, profiles: 1, applications: 1, resumes: 1,
        auto_submit_authorizations: fault === "authority" ? 1 : 0, communication_actions: 0,
        prepared: 1 - submitted, factual_draft_links: 1 - submitted, submitted,
        attempts: submitted, browser_sessions: submitted, execution_leases: submitted, metering: submitted };
    },
    async publishMock(draft) {
      calls.push("publish");
      assert.deepEqual(draft, { accountId, applicationId, jobId, resumeId,
        sourceAssetId: assetId, sourceSha256: sample.sha256 });
      Object.assign(state.applications[0], { state: "submitted", run_id: runId, submitted_at_ms: 1 });
      Object.assign(state.applications[0].receipt, { accountId, applicationId, result: { status: "submitted" },
        receiptObject: { sha256: digest }, _bluey_server_submission_fingerprint_v1: fingerprint });
      if (fault === "duplicate") state.applications.push(clone(state.applications[0]));
      if (fault === "resume") readback.resume.content.provenance.career_track_policy_authority.source_resume_sha256 = digest;
      return { ...draft, runId, receiptObjectSha256: digest, submissionFingerprintSha256: fingerprint };
    },
  };
  return {
    input,
    calls,
    restartReadback() {
      return { workspace: clone(state), resume: clone(readback.resume) };
    },
  };
}

test("Darwin headless journey binds the fixed sample, prepared draft and one persisted mock receipt", async () => {
  const { input, calls } = darwinJourney();
  assert.deepEqual(await runLocalQaDarwinMockReceiptJourney(input), {
    sampleResumeImported: true, reviewFirstPrepared: true, mockReceiptPublished: true,
    applicationCount: 1, submittedCount: 1,
  });
  assert.deepEqual(calls.filter(value => !value.startsWith("/")), ["prepared", "publish", "submitted"]);
});

test("branded Darwin evidence binds a fresh submitted workspace and factual resume readback", async () => {
  const { input, restartReadback } = darwinJourney();
  const evidence = await runLocalQaDarwinMockReceiptJourney(input);
  assert.strictEqual(requireLocalQaMockReceiptEvidence(evidence), evidence);

  const { workspace: restartedWorkspace, resume: restartedResume } = restartReadback();
  const binding = bindLocalQaMockReceiptReadback(evidence, accountId, restartedWorkspace);
  assert.deepEqual(binding, { accountId, resumeId });
  assert(Object.isFrozen(binding));
  assert.equal(assertLocalQaMockReceiptResume(evidence, restartedResume), undefined);
  assert.equal(restartedWorkspace.applications[0].state, "submitted");
  assert.equal(restartedWorkspace.applications[0].submission_mode, "review_first");
});

test("submitted receipt restart evidence rejects clones and every identity or source drift", async () => {
  const { input, restartReadback } = darwinJourney();
  const evidence = await runLocalQaDarwinMockReceiptJourney(input);
  const refused = { message: "local_qa_darwin_mock_receipt_journey_refused" };

  assert.throws(
    () => requireLocalQaMockReceiptEvidence(JSON.parse(JSON.stringify(evidence))),
    refused,
  );
  const canonical = restartReadback();
  assert.throws(
    () => bindLocalQaMockReceiptReadback(
      evidence,
      "77777777-7777-4777-8777-777777777777",
      canonical.workspace,
    ),
    refused,
  );

  const workspaceFaults = [
    current => { current.applications[0].id = "77777777-7777-4777-8777-777777777777"; },
    current => { current.applications[0].receipt.receiptObject.sha256 = "c".repeat(64); },
    current => {
      current.applications[0].receipt._bluey_server_submission_fingerprint_v1 = "c".repeat(64);
    },
    current => { current.applications[0].run_id = "different-synthetic-run"; },
    current => {
      current.profile.source_resume_asset_id = "77777777-7777-4777-8777-777777777777";
    },
    current => { current.applications[0].submission_mode = "auto_submit"; },
  ];
  for (const mutate of workspaceFaults) {
    const { workspace: restartedWorkspace } = restartReadback();
    mutate(restartedWorkspace);
    assert.throws(
      () => bindLocalQaMockReceiptReadback(evidence, accountId, restartedWorkspace),
      refused,
    );
  }

  const resumeFaults = [
    resume => {
      resume.content.provenance.job_snapshot_fingerprint = "c".repeat(64);
    },
    resume => {
      resume.content.provenance.career_track_policy_authority.source_resume_asset_id =
        "77777777-7777-4777-8777-777777777777";
    },
    resume => {
      resume.content.provenance.career_track_policy_authority.source_resume_sha256 =
        "c".repeat(64);
    },
    resume => { resume.content.provenance.generated_for_job_id = "non-synthetic-draft"; },
  ];
  for (const mutate of resumeFaults) {
    const { resume: restartedResume } = restartReadback();
    mutate(restartedResume);
    assert.throws(() => assertLocalQaMockReceiptResume(evidence, restartedResume), refused);
  }

  const finalReadback = restartReadback();
  assert.deepEqual(
    bindLocalQaMockReceiptReadback(evidence, accountId, finalReadback.workspace),
    { accountId, resumeId },
  );
  assert.equal(assertLocalQaMockReceiptResume(evidence, finalReadback.resume), undefined);
});

test("Darwin headless journey stops at census authority or changed post-publication evidence", async () => {
  for (const fault of ["authority", "duplicate", "resume"]) {
    const { input, calls } = darwinJourney(fault);
    const stage = fault === "authority" ? "prepared_census" : "receipt_readback";
    await assert.rejects(runLocalQaDarwinMockReceiptJourney(input), {
      message: `local_qa_darwin_mock_receipt_journey_refused:${stage}`,
    });
    assert.equal(calls.includes("publish"), fault !== "authority");
    assert.equal(calls.includes("submitted"), false);
  }
});

test("the mock plan must transition at seed with distribution off and no usage or active session", async () => {
  for (const fault of ["plan", "spent", "active", "distribution"]) {
    const { input, calls } = darwinJourney(fault);
    await assert.rejects(runLocalQaDarwinMockReceiptJourney(input), {
      message: "local_qa_darwin_mock_receipt_journey_refused:seed_onboarding_binding",
    });
    assert.equal(calls.includes("/api/jobs/applications"), false);
    assert.equal(calls.includes("publish"), false);
  }
});
