import assert from "node:assert/strict";
import test from "node:test";
import { LOCAL_QA_SAMPLE_RESUME } from "./local-qa-browser-sample-resume.mjs";
import {
  DURABLE_QA_POSTING_SEEDER,
  createLocalQaDurablePostingSeeder,
  durableFixtureDiagnostic,
  durableQaPostingRequest,
  isDurableQaFixtureRoot,
  parseDurableQaPostingResult,
  parseDurableQaPostgresUrl,
  reviewFirstMockReceiptObjectEnv,
  reviewFirstMockReceiptRequest,
} from "./local-qa-durable-posting-seeder.mjs";

const candidate = "123e4567-e89b-42d3-a456-426614174000";
const track = "123e4567-e89b-42d3-a456-426614174001";
const system = "9223372036854775808";

test("receipt actor uses the exact source-bound request and only the QA store's owned local configuration", () => {
  const draft = { accountId: candidate, applicationId: track, jobId: "job-synthetic",
    resumeId: "123e4567-e89b-42d3-a456-426614174002", sourceAssetId: "123e4567-e89b-42d3-a456-426614174003",
    sourceSha256: LOCAL_QA_SAMPLE_RESUME.sha256 };
  assert.deepEqual(reviewFirstMockReceiptRequest(draft, system), { version: 1, ...draft, expectedSystemIdentifier: system });
  assert.throws(() => reviewFirstMockReceiptRequest({ ...draft, token: "unaccepted" }, system));
  assert.throws(() => reviewFirstMockReceiptRequest({ ...draft, sourceSha256: "a".repeat(64) }, system));
  const store = { BLUEY_OBJECT_ENDPOINT_URL: "http://127.0.0.1:33697",
    BLUEY_OBJECT_BUCKET: `qa-${"1".repeat(32)}`, BLUEY_OBJECT_KEY_PREFIX: `qa-${"2".repeat(32)}`,
    BLUEY_OBJECT_ACCESS_KEY_ID: `qa${"3".repeat(32)}`, BLUEY_OBJECT_SECRET_ACCESS_KEY: "4".repeat(64),
    BLUEY_OBJECT_REGION: "us-east-1", BLUEY_OBJECT_MAX_BYTES: "10485760" };
  assert.deepEqual(reviewFirstMockReceiptObjectEnv(store), store);
  for (const endpoint of ["http://127.0.0.1:33697/", "http://127.0.0.1:5432", "http://store.invalid:33697"]) {
    assert.throws(() => reviewFirstMockReceiptObjectEnv({ ...store, BLUEY_OBJECT_ENDPOINT_URL: endpoint }));
  }
  assert.throws(() => reviewFirstMockReceiptObjectEnv({ ...store, BLUEY_JOBS_WORKER_TOKEN: "unaccepted" }));
});

test("the durable seeder accepts only its platform's exact canonical launcher root shape", () => {
  const leaf = "bluey-test-runs-a1b2c3d4e5f6/run.1795250653.12345.32615190360";
  for (const [platform, prefix, other] of [["darwin", "/private/tmp/", "/tmp/"],
    ["linux", "/tmp/", "/private/tmp/"]]) {
    assert.equal(isDurableQaFixtureRoot(prefix + leaf, platform), true);
    for (const value of [other + leaf, prefix + leaf + "\n", prefix + leaf + "/child", prefix + "other/" + leaf,
      prefix + leaf.replace("a1b2c3d4e5f6", "named-run"), prefix + leaf.replace(".12345.", ".1."),
      prefix + leaf.replace(".12345.", ".2147483648."), prefix + leaf.replace(".12345.", ".012345."),
      prefix + leaf.replace("1795250653", "18446744073709551616"), prefix + leaf.replace("32615190360", ".327"),
      prefix + leaf.replace("32615190360", "1234567890123"), undefined, null]) {
      assert.equal(isDurableQaFixtureRoot(value, platform), false);
    }
  }
  assert.equal(isDurableQaFixtureRoot("/tmp/" + leaf, "win32"), false);
});

test("durable local QA request is a closed synthetic fixture shape", () => {
  assert.deepEqual(durableQaPostingRequest({ candidateId: candidate, trackId: track, expectedSystemIdentifier: system }), {
    version: 1,
    candidate_id: candidate,
    track_id: track,
    expected_system_identifier: system,
    profile_scope: "candidate@bluey-qa.test",
    job_scope: { title: "Software Engineer", location: "New York, NY", skills: ["Rust", "TypeScript"] },
  });
  assert.equal(DURABLE_QA_POSTING_SEEDER.binaryArgument, "--seed-durable-qa-posting");
  assert.equal(DURABLE_QA_POSTING_SEEDER.stateChild, "bluey-jobs-qa-draft");
});

test("durable local QA request refuses arbitrary scopes and identities", () => {
  for (const input of [
    { candidateId: "candidate", trackId: track, expectedSystemIdentifier: system },
    { candidateId: candidate, trackId: "track-qa", expectedSystemIdentifier: system },
    { candidateId: candidate, trackId: track.replace("-42d3-", "-12d3-"), expectedSystemIdentifier: system },
    { candidateId: candidate, trackId: track, expectedSystemIdentifier: "0" },
  ]) assert.throws(() => durableQaPostingRequest(input), /local_qa_durable_posting_seeder_refused/);
});

test("durable local QA PostgreSQL URL is exact loopback role and database", () => {
  const url = "postgresql://bluey_qa_local:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa@127.0.0.1:43107/bluey_jobs_qa?sslmode=disable";
  assert.deepEqual(parseDurableQaPostgresUrl(url), {
    url, port: 43107,
  });
  for (const value of [
    "postgresql://bluey_qa_local:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa@localhost:43107/bluey_jobs_qa?sslmode=disable",
    "postgresql://other:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa@127.0.0.1:43107/bluey_jobs_qa?sslmode=disable",
    "postgresql://bluey_qa_local:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa@127.0.0.1:43107/other?sslmode=disable",
    "postgresql://bluey_qa_local:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa@127.0.0.1:5432/bluey_jobs_qa?x=1",
    url.replace(":43107/", ":5432/"),
  ]) assert.throws(() => parseDurableQaPostgresUrl(value), /local_qa_durable_posting_seeder_refused/);
});

test("durable local QA result admits one bounded job ID only", () => {
  assert.deepEqual(parseDurableQaPostingResult({ version: 1, job_id: "posting-42" }), { version: 1, jobId: "posting-42" });
  for (const value of [
    { version: 2, job_id: "posting-42" }, { version: 1, job_id: "" },
    { version: 1, job_id: "posting-42", extra: true },
  ]) assert.throws(() => parseDurableQaPostingResult(value), /local_qa_durable_posting_seeder_refused/);
});

test("the C2C request and result require their distinct schema version", async () => {
  const input = { candidateId: candidate, trackId: track, expectedSystemIdentifier: system };
  const legacy = durableQaPostingRequest(input);
  assert.deepEqual(durableQaPostingRequest({ ...input, fixtureMode: "c2c" }), { ...legacy, version: 2 });
  const result = { version: 2, job_id: "posting-42" };
  assert.deepEqual(parseDurableQaPostingResult(result, "c2c"), { version: 2, jobId: "posting-42" });
  assert.throws(() => parseDurableQaPostingResult(result));
  assert.throws(() => parseDurableQaPostingResult({ ...result, version: 1 }, "c2c"));
  assert.throws(() => durableQaPostingRequest({ ...input, fixtureMode: "toString" }));
  await assert.rejects(createLocalQaDurablePostingSeeder({ fixtureMode: "auto" }),
    /local_qa_durable_posting_seeder_refused/);
});

test("the v3 seed and result bind the exact uploaded sample asset and retain legacy fences", () => {
  const sha256 = LOCAL_QA_SAMPLE_RESUME.sha256;
  const options = { candidateId: candidate, trackId: track, expectedSystemIdentifier: system,
    fixtureMode: "imported_resume", sourceResumeAssetId: candidate, sourceResumeSha256: sha256 };
  const made = durableQaPostingRequest(options);
  assert.equal(made.version, 3);
  assert.deepEqual(made.job_scope, { title: "Data Engineer", location: "New York, NY", skills: ["Python", "SQL"] });
  assert.equal(made.source_resume_asset_id, candidate);
  assert.equal(made.source_resume_sha256, sha256);
  const result = { version: 3, job_id: "posting-42", source_resume_asset_id: candidate, source_resume_sha256: sha256 };
  assert.deepEqual(parseDurableQaPostingResult(result, "imported_resume", { assetId: candidate, sha256 }),
    { version: 3, jobId: "posting-42" });
  assert.throws(() => parseDurableQaPostingResult(result, "imported_resume", { assetId: track, sha256 }));
  assert.throws(() => parseDurableQaPostingResult(result, "w2"));
  for (const change of [{ fixtureMode: "w2" }, { fixtureMode: "c2c" }, { sourceResumeAssetId: undefined },
    { sourceResumeSha256: "b".repeat(64) }]) assert.throws(() => durableQaPostingRequest({ ...options, ...change }));
});

test("the v4 review-first mock receipt seed has the exact imported-resume scope and distinct wire contract", () => {
  const sha256 = LOCAL_QA_SAMPLE_RESUME.sha256;
  const input = {
    candidateId: candidate,
    trackId: track,
    expectedSystemIdentifier: system,
    fixtureMode: "review_first_mock_receipt",
    sourceResumeAssetId: candidate,
    sourceResumeSha256: sha256,
  };
  const v3 = durableQaPostingRequest({ ...input, fixtureMode: "imported_resume" });
  const v4 = durableQaPostingRequest(input);
  assert.deepEqual(v4, { ...v3, version: 4 });
  assert.equal(v4.job_scope.title, "Data Engineer");
  assert.deepEqual(v4.job_scope.skills, ["Python", "SQL"]);
  const result = {
    version: 4,
    job_id: "posting-42",
    source_resume_asset_id: candidate,
    source_resume_sha256: sha256,
  };
  assert.deepEqual(
    parseDurableQaPostingResult(result, "review_first_mock_receipt", { assetId: candidate, sha256 }),
    { version: 4, jobId: "posting-42" },
  );
  assert.throws(() => parseDurableQaPostingResult(result, "imported_resume", { assetId: candidate, sha256 }));
  assert.throws(() => parseDurableQaPostingResult({ ...result, version: 3 }, "review_first_mock_receipt", { assetId: candidate, sha256 }));
  for (const changed of [
    { candidateId: "candidate-not-a-uuid" },
    { sourceResumeSha256: "b".repeat(64) },
    { sourceResumeAssetId: undefined },
  ]) assert.throws(() => durableQaPostingRequest({ ...input, ...changed }));
  for (const drift of [
    { assetId: track, sha256 },
    { assetId: candidate, sha256: "b".repeat(64) },
  ]) assert.throws(() => parseDurableQaPostingResult(result, "review_first_mock_receipt", drift));
  assert.equal(durableQaPostingRequest({ candidateId: candidate, trackId: track, expectedSystemIdentifier: system }).version, 1);
  assert.equal(durableQaPostingRequest({ candidateId: candidate, trackId: track, expectedSystemIdentifier: system, fixtureMode: "c2c" }).version, 2);
});

test("the v4 mock-receipt seeder keeps unique private files and its fixed CLI selector", async () => {
  const source = await import("node:fs/promises")
    .then(({ readFile }) => readFile(new URL("./local-qa-durable-posting-seeder.mjs", import.meta.url), "utf8"));
  assert.match(source, /--seed-durable-review-first-mock-receipt-posting/);
  assert.ok(source.includes('version === 4 ? "durable-review-first-mock-receipt-posting"'));
  for (const path of [
    'const requestPath = join(stateDirectory, `${prefix}-request.json`);',
    'const resultPath = join(stateDirectory, `${prefix}-result.json`);',
    'const consumedPath = join(stateDirectory, `${prefix}-request.consumed.json`);',
  ]) assert.ok(source.includes(path));
  assert.doesNotMatch(source, /version === 4 \? IMPORTED_RESUME_FIXTURE_ARG/);
});

test("durable seeder source retains the two-phase one-use and secret boundary", async () => {
  const source = await import("node:fs/promises").then(({ readFile }) => readFile(new URL("./local-qa-durable-posting-seeder.mjs", import.meta.url), "utf8"));
  assert.equal(typeof createLocalQaDurablePostingSeeder, "function");
  assert.match(source, /BLUEY_JOBS_DATA_KEY: dataKey/);
  assert.match(source, /BLUEY_TEST_POSTGRES_URL: postgres\.url/);
  assert.doesNotMatch(source, /BLUEY_DATABASE_URL: postgres\.url/);
  assert.match(source, /stdio: \["ignore", "ignore", "pipe"\]/);
  assert.match(source, /need\(!closed && !consumed/);
  assert.match(source, /O_CREAT \| constants.O_EXCL \| constants.O_NOFOLLOW/);
  assert.match(source, /const TERM_GRACE_MS = 5_000/);
  assert.match(source, /child\.kill\("SIGTERM"\)/);
  assert.match(source, /child\.kill\("SIGKILL"\)/);
  assert.match(source, /terminal\.cancellation === null/);
  assert.match(source, /active\.stop\("closed"\)/);
  assert.match(source, /JSON\.stringify\(parsed\)\}\\n/);
});

test("fixture diagnostics accept only one closed code and never echo unstructured stderr", () => {
  const diagnostic = '{"event":"browser_packet_fixture_failed","code":"durable_state_marker"}\n';
  assert.equal(durableFixtureDiagnostic(diagnostic, null), "durable_state_marker");
  assert.equal(durableFixtureDiagnostic(diagnostic, "timeout"), "timeout");
  for (const invalid of [diagnostic + "\n", "databaseUrl=secret", diagnostic.replace("durable_state_marker", "secret/")]) {
    assert.equal(durableFixtureDiagnostic(invalid, null), "unclassified");
  }
});
