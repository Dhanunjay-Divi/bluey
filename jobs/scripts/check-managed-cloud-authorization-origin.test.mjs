import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import {
  bindManagedCloudAuthorizationPolicy,
  selectManagedCloudAuthorizationOrigin,
  selectManagedCloudCandidateOrigin,
  verifyManagedCloudAuthorizationArchive,
} from "./check-managed-cloud-authorization-origin.mjs";

const approved = "b".repeat(64);
const sourceCommit = "a".repeat(40);
const workflowCommit = "e".repeat(40);
const archive = Buffer.from("synthetic archive bytes, not a ZIP\n");
const digest = "sha256:" + createHash("sha256").update(archive).digest("hex");
const workflow = ".github/workflows/jobs-managed-cloud-release.yml";
const complete = { status: "completed", conclusion: "success" };
const stepNames = [
  "Check out only the verifier",
  "Read the verified candidate source commit",
  "Check out candidate policy files as data only",
  "Verify the protected Observability policy before handling release signatures",
  "Reverify and authorize with protected trust material",
  "Upload authorized bytes",
];

function fixture() {
  return {
    expected: {
      artifactMaxBytes: 1_000, defaultBranch: "release/stable", repository: "test/bluey",
      repositoryId: 17, runAttempt: 2, runId: 29,
      authorizationWorkflowCommit: workflowCommit, candidateSourceCommit: sourceCommit,
    },
    run: {
      id: 29, run_attempt: 2, event: "workflow_dispatch", ...complete, path: workflow + "@release/stable",
      head_branch: "release/stable", head_sha: workflowCommit,
      repository: { id: 17, full_name: "test/bluey" },
    },
    jobs: { total_count: 2, jobs: [
      { name: "publish", status: "completed", conclusion: "skipped" },
      { name: "authorize", id: 31, run_id: 29, head_sha: workflowCommit, ...complete,
        steps: stepNames.map((name, index) => ({ name, number: index + 2, ...complete })),
      },
    ] },
    artifacts: { total_count: 2, artifacts: [
      { name: "jobs-managed-cloud-authorized-29-1", id: 63 },
      { name: "jobs-managed-cloud-authorized-29-2", id: 64, digest, size_in_bytes: archive.length, expired: false,
        workflow_run: { id: 29, repository_id: 17, head_repository_id: 17,
          head_branch: "release/stable", head_sha: workflowCommit },
      },
    ] },
  };
}

test("selects only the exact authorization attempt and immutable artifact", () => {
  const data = fixture();
  const selected = selectManagedCloudAuthorizationOrigin(data);
  assert.equal(selected.artifactId, 64);
  assert.equal(selected.artifactDigest, digest);
  assert.equal(selected.runAttempt, 2);
  assert.equal(selected.authorizationWorkflowCommit, workflowCommit);
  assert.equal(selected.candidateSourceCommit, sourceCommit);
  assert.equal(selected.environmentProtectionStatus, "workflow_declared_settings_not_attested");
  data.jobs.jobs[1].run_attempt = 2;
  assert.deepEqual(selectManagedCloudAuthorizationOrigin(data), selected);
});

test("refuses other run, attempt, source repository, branch, event or result", () => {
  for (const patch of [
    { id: 28 }, { run_attempt: 1 }, { path: ".github/workflows/noise.yml" }, { event: "push" },
    { status: "in_progress" }, { conclusion: "skipped" }, { head_branch: "main" }, { head_sha: sourceCommit },
    { repository: { id: 18, full_name: "test/bluey" } }, { head_repository: { id: 18 } },
  ]) {
    const data = fixture();
    Object.assign(data.run, patch);
    assert.throws(() => selectManagedCloudAuthorizationOrigin(data), /successful exact-source workflow attempt/);
  }
});

test("requires complete job results and one successful authorize job with the policy step in order", () => {
  for (const mutate of [
    (d) => { d.jobs.total_count = 3; },
    (d) => { d.jobs.jobs[0] = d.jobs.jobs[1]; },
    (d) => { d.jobs.jobs[1].run_attempt = 1; },
    (d) => { d.jobs.jobs[1].head_branch = "main"; },
    (d) => { d.jobs.jobs[1].steps[3].conclusion = "skipped"; },
    (d) => { d.jobs.jobs[1].steps[3].number = d.jobs.jobs[1].steps[4].number + 1; },
    (d) => { d.jobs.jobs[1].steps[3] = d.jobs.jobs[1].steps[4]; },
  ]) {
    const data = fixture();
    mutate(data);
    assert.throws(() => selectManagedCloudAuthorizationOrigin(data), /authorization (?:jobs|job|prerequisite)/);
  }
});

test("refuses substituted, duplicate, incomplete, expired or oversized archives", () => {
  for (const mutate of [
    (d) => { d.artifacts.total_count = 3; },
    (d) => { d.artifacts.artifacts[0] = d.artifacts.artifacts[1]; },
    (d) => { d.artifacts.artifacts[1].name = "jobs-managed-cloud-authorized"; },
    (d) => { d.artifacts.artifacts[1].expired = true; },
    (d) => { d.artifacts.artifacts[1].workflow_run.repository_id = 18; },
    (d) => { d.artifacts.artifacts[1].workflow_run.head_repository_id = 18; },
    (d) => { d.artifacts.artifacts[1].workflow_run.head_sha = "c".repeat(40); },
    (d) => { d.artifacts.artifacts[1].digest = null; },
    (d) => { d.artifacts.artifacts[1].size_in_bytes = d.expected.artifactMaxBytes + 1; },
  ]) {
    const data = fixture();
    mutate(data);
    assert.throws(() => selectManagedCloudAuthorizationOrigin(data), /artifact/);
  }
});

test("requires a policy-bound receipt for the exact selected source", () => {
  const row = selectManagedCloudAuthorizationOrigin(fixture());
  const receipt = {
    audience: "bluey-jobs-managed-cloud-authorization-v2",
    authorizationId: "authorization-29-2",
    version: 2, sourceCommit, observabilityPolicySha256: approved,
  };
  assert.equal(bindManagedCloudAuthorizationPolicy(row, receipt, approved).observabilityPolicySha256, approved);
  for (const diff of [{ version: 1 }, { sourceCommit: undefined }, { sourceCommit: workflowCommit },
    { authorizationId: "authorization-29-1" },
    { observabilityPolicySha256: "d".repeat(64) }]) {
    assert.throws(() => bindManagedCloudAuthorizationPolicy(row, { ...receipt, ...diff }, approved),
      /does not bind the protected approved policy and source/);
  }
  assert.throws(() => bindManagedCloudAuthorizationPolicy(row, receipt, undefined),
    /does not bind the protected approved policy and source/);
});

test("authenticates the original selected archive bytes, without extracting them", async () => {
  const row = selectManagedCloudAuthorizationOrigin(fixture());
  assert.deepEqual(await verifyManagedCloudAuthorizationArchive(row, [archive.subarray(0, 1), archive.subarray(1)]),
    { artifactDigest: digest, artifactSizeBytes: archive.length });
  await assert.rejects(() => verifyManagedCloudAuthorizationArchive(row, [archive.subarray(0, 1)]), /size and digest/);
  await assert.rejects(() => verifyManagedCloudAuthorizationArchive(row, [Buffer.alloc(archive.length)]), /size and digest/);
  await assert.rejects(() => verifyManagedCloudAuthorizationArchive(row, [archive, Buffer.alloc(1)]), /exceeds/);
});

function candidateFixture() {
  const answer = fixture();
  const job = (name, id, labels) => ({
    name, id, run_id: 29, head_sha: workflowCommit, ...complete,
    steps: labels.map((label, index) => ({ name: label, number: index + 2, ...complete })),
  });
  answer.jobs = { total_count: 2, jobs: [
    job("candidate", 40, [
      "Bind successful exact-tip CI instead of synthetic candidate evidence", "Build each component exactly once",
      "Assemble the deterministic release candidate", "Upload the only candidate bytes",
    ]),
    job("verify", 41, ["Fetch candidate bytes from the build job", "Independently inspect the downloaded bytes"]),
  ] };
  answer.artifacts.artifacts[1].name = "jobs-managed-cloud-candidate-29-2";
  return answer;
}

test("candidate selection derives the source and requires distinct build and verify jobs", () => {
  const selected = selectManagedCloudCandidateOrigin(candidateFixture());
  assert.equal(selected.candidateSourceCommit, workflowCommit);
  assert.equal(selected.candidateBuildJobId, 40);
  assert.equal(selected.candidateVerifyJobId, 41);
  assert.equal(selected.artifactId, 64);
  for (const mutate of [
    (d) => { d.run.run_attempt = 1; },
    (d) => { d.jobs.jobs[0].steps[3].conclusion = "cancelled"; },
    (d) => { d.jobs.jobs[1].id = 40; },
    (d) => { d.jobs.jobs[1].steps[0].number = 5; },
    (d) => { d.jobs.jobs[1].head_sha = sourceCommit; },
    (d) => { d.jobs.jobs[1].conclusion = "skipped"; },
    (d) => { d.jobs.total_count = 3; },
    (d) => { d.artifacts.artifacts[1].name = "jobs-managed-cloud-candidate-29-1"; },
    (d) => { d.artifacts.artifacts[1].name = "jobs-managed-cloud-authorized-29-2"; },
    (d) => { d.artifacts.artifacts[1].workflow_run.head_sha = sourceCommit; },
  ]) {
    const data = candidateFixture();
    mutate(data);
    assert.throws(() => selectManagedCloudCandidateOrigin(data), /candidate/);
  }
});
