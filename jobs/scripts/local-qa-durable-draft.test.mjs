import assert from "node:assert/strict";
import test from "node:test";
import { createLocalQaDurableDraft } from "./local-qa-durable-draft.mjs";

const candidate = {
  accountId: "11111111-1111-4111-8111-111111111111",
  candidateEmail: "candidate@bluey-qa.test",
};
const identityId = "22222222-2222-4222-8222-222222222222";
const applicationId = "33333333-3333-4333-8333-333333333333";
const resumeId = "44444444-4444-4444-8444-444444444444";
const jobId = "job-synthetic-1", fingerprint = "a".repeat(64);
const c2cEligibility = { can_prepare: true, can_queue_local: false, can_queue_cloud: false,
  can_auto_submit: false, hard_failures: [], review_reasons: [{ code: "c2c_submission_requires_candidate" }],
  passed_checks: ["employment_type_allowed", "engagement_type_allowed",
    "track_employment_type_allowed", "track_engagement_type_allowed"] };

function workspaceFixture({ initialReply, changeProfile, changePreferences, changeMatch, changePersistence, changeAfterSeed,
  metering, rejectPrepare = false, fixtureMode, changeReceipt } = {}) {
  const operations = [];
  const c2c = fixtureMode === "c2c";
  const application = { id: applicationId, job_id: jobId, state: "awaiting_review",
    submission_mode: "review_first", run_id: null, submitted_at_ms: null, resume_version_id: resumeId,
    receipt: { resume_version_id: resumeId, confirmation: null, job_snapshot_fingerprint: fingerprint,
      ...(c2c ? { job_snapshot: { id: jobId, employment_type: "contract c2c" },
        eligibility: c2cEligibility } : {}), ...changeReceipt } };
  const resume = { id: resumeId, job_id: jobId, mode: "factual", content: { provenance: {
    generated_for_job_id: jobId, job_snapshot_fingerprint: fingerprint,
  } } };
  const workspace = { profile: { email: candidate.candidateEmail, onboarding_complete: false },
    application_identities: [{ id: identityId, email: candidate.candidateEmail,
      verification_status: "verified", is_default: true }],
    applications: [], matches: [], tracks: [], browser_sessions: [], auto_submit_authorizations: [],
    entitlement: { plan: "free", local_browser: false, cloud_browser: false, used_packets: 0 } };
  if (initialReply) Object.assign(workspace, initialReply);
  const run = createLocalQaDurableDraft({
    fixtureMode,
    async readWorkspace() {
      operations.push("workspace");
      return structuredClone(workspace);
    },
    async readTaxonomy() {
      operations.push("taxonomy");
      return { taxonomyVersion: "fixture", taxonomySha256: fingerprint };
    },
    async saveOnboarding(body, headers) {
      operations.push("onboarding");
      assert.deepEqual(headers, { "x-bluey-jobs-taxonomy-version": "fixture",
        "x-bluey-jobs-taxonomy-sha256": fingerprint });
      assert.equal(body.profile.email, candidate.candidateEmail);
      assert.equal(body.profile.daily_limit, 10);
      assert.equal(body.preferences.daily_limit, 10);
      assert.equal(body.track.application_identity_id, identityId);
      assert.deepEqual(body.track.policy.relevant_employment_ids, [body.profile.employment[0].id]);
      for (const policy of [body.preferences, body.track.policy]) {
        assert.deepEqual(policy.employment_types, [c2c ? "contract" : "full_time"]);
        assert.deepEqual(policy.engagement_types, [c2c ? "c2c" : "w2"]);
      }
      workspace.profile = { ...body.profile, onboarding_complete: true, ...changeProfile };
      workspace.preferences = { ...body.preferences, ...changePreferences };
      workspace.tracks = [body.track];
    },
    async seedPosting(body) {
      operations.push("seed");
      assert.deepEqual(body, { accountId: candidate.accountId, trackId: workspace.tracks[0].id });
      const match = { id: jobId, company: "Synthetic Qualification Employer",
        track_id: body.trackId, ...(c2c ? { employment_type: "contract c2c", eligibility: c2cEligibility }
          : { eligibility: { can_prepare: true } }) };
      workspace.matches = [{ ...match, ...changeMatch }];
      Object.assign(workspace.profile, changeAfterSeed);
      return jobId;
    },
    async prepareDraft(body) {
      operations.push("prepare");
      assert.deepEqual(body, { job_id: jobId, mode: "factual", submission_mode: "review_first" });
      if (rejectPrepare) throw new Error("synthetic_commit_uncertain");
      workspace.applications = [{ ...application, ...changePersistence }];
      return { application, resume_version: resume, metering };
    },
  });
  return { run, operations };
}

test("saves one fresh synthetic track and prepares exactly one factual review draft", async () => {
  const { run, operations } = workspaceFixture();
  const proof = await run(candidate);
  assert.deepEqual(proof, { applicationId, jobId, resumeId, jobSnapshotFingerprint: fingerprint });
  assert.deepEqual(operations,
    ["workspace", "taxonomy", "onboarding", "workspace", "seed", "workspace", "prepare", "workspace"]);
  await assert.rejects(run(candidate), /qa_draft_candidate_scope/);
});

test("C2C prepares a factual review draft with candidate-only authority", async () => {
  const { run, operations } = workspaceFixture({ fixtureMode: "c2c" });
  assert.deepEqual(await run(candidate), { applicationId, jobId, resumeId, jobSnapshotFingerprint: fingerprint,
    fixtureMode: "c2c" });
  assert.equal(operations.filter(value => value === "prepare").length, 1);
  assert.throws(() => workspaceFixture({ fixtureMode: "arbitrary" }), /qa_draft_fixture_mode/);
});

test("C2C refuses policy, eligibility, or job drift before Prepare", async () => {
  for (const change of [
    { changePreferences: { engagement_types: ["w2"] } },
    { changeMatch: { employment_type: "full_time w2" } },
    ...[{ can_queue_local: true }, { can_queue_cloud: true }, { can_auto_submit: true },
      { hard_failures: [{ code: "mismatch" }] }, { review_reasons: [] }, { passed_checks: [] }]
      .map(override => ({ changeMatch: { eligibility: { ...c2cEligibility, ...override } } })),
  ]) {
    const { run, operations } = workspaceFixture({ fixtureMode: "c2c", ...change });
    await assert.rejects(run(candidate), /qa_draft_c2c_policy|qa_draft_c2c_candidate_only/);
    assert(!operations.includes("prepare"));
  }
});

test("C2C refuses approved execution or a changed persisted receipt without retry", async () => {
  for (const change of [
    { changeReceipt: { approved_execution: {} } },
    { changePersistence: { receipt: { resume_version_id: resumeId, confirmation: null,
      job_snapshot_fingerprint: fingerprint, job_snapshot: { id: jobId, employment_type: "contract c2c" },
      eligibility: { ...c2cEligibility, can_auto_submit: true } } } },
  ]) {
    const { run, operations } = workspaceFixture({ fixtureMode: "c2c", ...change });
    await assert.rejects(run(candidate), /qa_draft_c2c_receipt/);
    await assert.rejects(run(candidate), /qa_draft_candidate_scope/);
    assert.equal(operations.filter(value => value === "prepare").length, 1);
  }
});

test("foreign sign-in, existing applications and unverified identities never reach writes", async () => {
  const foreign = workspaceFixture();
  await assert.rejects(foreign.run({ ...candidate, candidateEmail: "operator@bluey-qa.test" }),
    /qa_draft_candidate_scope/);
  assert.deepEqual(foreign.operations, []);
  for (const initialReply of [{ applications: [{}] }, { application_identities: [] }, {
    application_identities: [{ id: identityId, email: candidate.candidateEmail,
      verification_status: "verified", is_default: false }],
  }, {
    entitlement: { plan: "free", local_browser: true, cloud_browser: false, used_packets: 0 },
  }]) {
    const { run, operations } = workspaceFixture({ initialReply });
    await assert.rejects(run(candidate), /qa_draft_candidate/);
    assert.deepEqual(operations, ["workspace"]);
  }
});

test("a returned listing without current Prepare authority is not applied or retried", async () => {
  const { run, operations } = workspaceFixture({ changeMatch: { eligibility: { can_prepare: false } } });
  await assert.rejects(run(candidate), /qa_draft_verified_match/);
  await assert.rejects(run(candidate), /qa_draft_candidate_scope/);
  assert(!operations.includes("prepare"));
});

test("nonstandard saved preferences refuse before signed posting or Prepare", async () => {
  for (const changePreferences of [{ daily_limit: 5 }, { apply_once_per_company: false }]) {
    const { run, operations } = workspaceFixture({ changePreferences });
    await assert.rejects(run(candidate), /qa_draft_onboarding_readback/);
    assert.deepEqual(operations, ["workspace", "taxonomy", "onboarding", "workspace"]);
  }
});

test("a stale or incomplete saved profile refuses before posting seed or Prepare", async () => {
  for (const changeProfile of [{ headline: "Old headline" }, { current_location: "Boston, MA" },
    { skills: [] }, { employment: [] }]) {
    const { run, operations } = workspaceFixture({ changeProfile });
    await assert.rejects(run(candidate), /qa_draft_onboarding_readback/);
    assert.deepEqual(operations, ["workspace", "taxonomy", "onboarding", "workspace"]);
  }
});

test("a profile changed during posting seed refuses before Prepare", async () => {
  const { run, operations } = workspaceFixture({ changeAfterSeed: { skills: ["Python"] } });
  await assert.rejects(run(candidate), /qa_draft_profile_drift_before_prepare/);
  assert.deepEqual(operations, ["workspace", "taxonomy", "onboarding", "workspace", "seed", "workspace"]);
});

test("ambiguous Prepare and changed persisted receipt or state fail without retry", async () => {
  for (const fixture of [{ rejectPrepare: true }, { changePersistence: { state: "queued" } },
    { changePersistence: { receipt: { job_snapshot_fingerprint: "b".repeat(64) } } }, { metering: {} }]) {
    const { run, operations } = workspaceFixture(fixture);
    await assert.rejects(run(candidate), /synthetic_commit_uncertain|qa_draft_application_binding|qa_draft_unexpected_metering/);
    await assert.rejects(run(candidate), /qa_draft_candidate_scope/);
    assert.equal(operations.filter(value => value === "prepare").length, 1);
  }
});
