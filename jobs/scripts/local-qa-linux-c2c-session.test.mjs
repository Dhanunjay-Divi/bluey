import assert from "node:assert/strict";
import test from "node:test";
import {
  prepareLocalQaLinuxC2cDraft,
  requireLocalQaLinuxC2cEnvironment,
  requireLocalQaLinuxC2cSession,
} from "./local-qa-linux-c2c-session.mjs";
import { attachLocalQaLinuxPostgres, validateLinuxDurablePgIdentity } from "./local-qa-linux-postgres.mjs";

const candidate = "11111111-1111-4111-8111-111111111111";
const ready = {
  accountId: candidate, candidateEmail: "candidate@bluey-qa.test", systemIdentifier: "9223372036854775808",
  dataKey: Buffer.from(Array.from({ length: 32 }, (_, index) => index)).toString("base64"),
  databaseUrl: `postgresql://bluey_qa_local:${"a".repeat(64)}@127.0.0.1:43107/bluey_jobs_qa?sslmode=disable`,
  readWorkspace: async () => {}, readTaxonomy: async () => {}, saveOnboarding: async () => {},
  prepareDraft: async () => {}, close: async () => {},
  postgresRuntime: { postgresPath: "/usr/lib/postgresql/17/bin/postgres", postgresSha256: "a".repeat(64),
    unitCgroup: "/system.slice/bluey-jobs-test-owned.service", networkNamespace: "net:[4026532555]",
    assertOwnedUnit: async () => true },
  queryIdentity: async () => {},
};

test("the Linux C2C session binds only the exact synthetic candidate and current database contract", () => {
  const original = { ...ready, postgresRuntime: { ...ready.postgresRuntime } };
  const session = requireLocalQaLinuxC2cSession(original);
  assert.deepEqual(session, ready);
  assert(Object.isFrozen(session));
  assert(Object.isFrozen(session.postgresRuntime));
  original.accountId = "ui_fixture_not_durable";
  original.postgresRuntime.unitCgroup += "-substituted";
  assert.equal(session.accountId, candidate);
  assert.equal(session.postgresRuntime.unitCgroup, ready.postgresRuntime.unitCgroup);
});

test("Linux C2C session rejects stale fixture families, unsafe connection scopes, and unowned operations", () => {
  for (const [change, reason] of [
    [{ accountId: `ui_fixture_${candidate}` }, /candidate_scope/],
    [{ accountId: `${candidate}\n` }, /candidate_scope/],
    [{ candidateEmail: "operator@bluey-qa.test" }, /candidate_scope/],
    [{ systemIdentifier: "18446744073709551616" }, /system_identity_shape/],
    [{ systemIdentifier: "01" }, /system_identity_shape/],
    [{ systemIdentifier: "42\n" }, /system_identity_shape/],
    [{ dataKey: "0".repeat(64) }, /data_key_shape/],
    [{ databaseUrl: ready.databaseUrl.replace("bluey_jobs_qa", "bluey_jobs_nonprod") }, /seeder_refused/],
    [{ databaseUrl: ready.databaseUrl.replace("127.0.0.1", "localhost") }, /seeder_refused/],
    [{ databaseUrl: `${ready.databaseUrl}&options=unexpected` }, /seeder_refused/],
    [{ assertOwnedPostmaster: async () => true }, /session_shape/],
    [{ close: undefined }, /session_operations/],
    [{ approve: async () => {} }, /session_shape/],
  ]) assert.throws(() => requireLocalQaLinuxC2cSession({ ...ready, ...change }), reason);
});

test("Linux C2C session requires the exact runtime binding instead of a caller-supplied process check", () => {
  assert.throws(() => requireLocalQaLinuxC2cSession({ ...ready, postgresRuntime: null }), /runtime_binding/);
  for (const drift of [{ assertOwnedUnit: undefined }, { postgresPath: "postgres" }, { postgresSha256: "0" },
    { networkNamespace: "net:[04026532555]" }, { unitCgroup: "/" }, { unitCgroup: "/system.slice/../wrong" },
    { assertOwnedPostmaster: async () => true }]) {
    const postgresRuntime = { ...ready.postgresRuntime, ...drift };
    assert.throws(() => requireLocalQaLinuxC2cSession({ ...ready, postgresRuntime }), /runtime_binding/);
  }
});

test("the Linux C2C entrypoint never reaches owned startup from an unbound or malformed root", async () => {
  let startupCalled = false;
  await assert.rejects(prepareLocalQaLinuxC2cDraft({ root: "/tmp/bluey-test-runs-unbound/run.10.42.123",
    startOwnedSession: async () => { startupCalled = true; } }), /local_qa_linux_c2c_run_root/);
  assert.equal(startupCalled, false);
});

test("native C2C QA rejects ambient provider/database authority and enabled effects", () => {
  assert.doesNotThrow(() => requireLocalQaLinuxC2cEnvironment({ BLUEY_JOBS_MODEL_GENERATION_ENABLED: "0" }));
  for (const name of ["BLUEY_DATABASE_URL", "BLUEY_TEST_POSTGRES_URL", "PGPASSWORD", "OPENAI_API_KEY",
    "BLUEY_JOBS_WORKFLOW_ORIGIN", "BLUEY_JOBS_GOOGLE_CLIENT_SECRET"]) {
    assert.throws(() => requireLocalQaLinuxC2cEnvironment({ [name]: "synthetic-denied" }), /ambient_authority/);
  }
  for (const name of ["BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED", "BLUEY_JOBS_MODEL_GENERATION_ENABLED",
    "BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED", "BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED"]) {
    assert.throws(() => requireLocalQaLinuxC2cEnvironment({ [name]: "1" }), /effect_flags/);
  }
});

const stagedDirectory = "/tmp/bluey-test-runs-a1b2c3d4e5f6/run.1795250653.12345.32615190360";
const expectedIdentity = { address: "127.0.0.1", port: 43107, database: "bluey_jobs_qa", role: "bluey_qa_local",
  dataDirectory: `${stagedDirectory}/bluey-jobs-qa-draft/postgres`, systemIdentifier: ready.systemIdentifier };

test("the Linux PG adapter binds all six physical SQL identity fields and refuses old fixture roles", () => {
  const accepted = validateLinuxDurablePgIdentity(expectedIdentity, { ...expectedIdentity });
  assert.deepEqual(accepted, expectedIdentity);
  assert(Object.isFrozen(accepted));
  for (const drift of [{ address: "::1" }, { port: 43111 }, { port: "43107" },
    { database: "bluey_jobs_nonprod" }, { role: "bluey_jobs_test" }, { dataDirectory: `${stagedDirectory}/other` },
    { systemIdentifier: "42" }, { extra: "synthetic" }]) {
    assert.throws(() => validateLinuxDurablePgIdentity({ ...expectedIdentity, ...drift }, expectedIdentity),
      /BLUEY_LOCAL_QA_LINUX_POSTGRES_REFUSED/);
  }
});

test("an unowned Linux PostgreSQL attachment never calls process or database observers", async () => {
  let calls = 0;
  await assert.rejects(attachLocalQaLinuxPostgres({ root: stagedDirectory, databaseUrl: ready.databaseUrl,
    dataKey: ready.dataKey, expectedSystemIdentifier: ready.systemIdentifier, expectedCandidateId: candidate,
    async assertOwnedPostmaster() { calls++; return true; }, async queryIdentity() { calls++; return expectedIdentity; },
  }), /BLUEY_LOCAL_QA_LINUX_POSTGRES_REFUSED/);
  assert.equal(calls, 0);
});
