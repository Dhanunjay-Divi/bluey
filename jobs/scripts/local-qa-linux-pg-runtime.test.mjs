import assert from "node:assert/strict";
import { once } from "node:events";
import { PassThrough } from "node:stream";
import test from "node:test";
import { expectedLocalQaLinuxC2cPgCapsule } from "./local-qa-linux-c2c-pg-capsule.mjs";
import {
  createLocalQaLinuxPgRuntime,
  requireLocalQaLinuxPgIdentityRequest,
  validateLocalQaLinuxPgRuntimeBinding,
} from "./local-qa-linux-pg-runtime.mjs";
import { createLocalQaLinuxPgReady, parseLocalQaLinuxPgReadyMarker } from "./local-qa-linux-pg-ready.mjs";
import { createLocalQaLinuxPgTools } from "./local-qa-linux-pg-tools.mjs";
import {
  createLocalQaLinuxPidfdBinder, createLocalQaLinuxPidfdProtocol, nextLocalQaLinuxPidfdState,
} from "./local-qa-linux-pidfd-client.mjs";
import { isLocalQaLinuxPostmasterReady, LINUX_DURABLE_PG_IDENTITY_SQL } from "./local-qa-linux-postgres.mjs";

const root = "/tmp/bluey-test-runs-a1b2c3d4e5f6/run.1795250653.12345.32615190360";
const stateDirectory = `${root}/bluey-jobs-qa-draft`;
const dataDirectory = `${stateDirectory}/postgres`;
const port = 43107;
const pgUid = "11111111-1111-4111-8111-111111111111";
const pgCapsule = expectedLocalQaLinuxC2cPgCapsule("1".repeat(32));
const pidfdToolchain = {
  python: { path: "/usr/bin/python3.12", sha256: "b".repeat(64) },
  helper: { path: "/run/bluey-owned/local-qa-linux-postmaster-pidfd.py", sha256: "c".repeat(64) },
};
const binding = {
  root, stateDirectory, port, pgCapsule, pidfdToolchain, networkNamespace: "net:[4026532555]",
  unitCgroup: `/system.slice/bluey-jobs-qa-${pgCapsule.run}.service`, assertOwnedUnit: async () => true,
};

test("Linux PG runtime freezes the exact capsule and pidfd proof, not launcher inputs by reference", () => {
  const mutable = { ...binding, pgCapsule: structuredClone(pgCapsule),
    pidfdToolchain: { ...pidfdToolchain, helper: { ...pidfdToolchain.helper } } };
  const bound = validateLocalQaLinuxPgRuntimeBinding(mutable);
  assert(Object.isFrozen(bound));
  assert(Object.isFrozen(bound.pgCapsule));
  assert(Object.isFrozen(bound.pgCapsule.tools.postgres));
  assert(Object.isFrozen(bound.pgCapsule.libraries.libpqLink));
  assert(Object.isFrozen(bound.pidfdToolchain.helper));
  mutable.pgCapsule.tools.postgres.sha256 = "b".repeat(64);
  mutable.pgCapsule.libraries.libpqLink.target = "/usr/lib/libpq.so";
  mutable.pidfdToolchain.helper.sha256 = "d".repeat(64);
  mutable.port = 5432;
  assert.deepEqual(bound.pgCapsule, pgCapsule);
  assert.equal(bound.pidfdToolchain.helper.sha256, pidfdToolchain.helper.sha256);
  assert.equal(bound.port, port);
});

test("Linux PG runtime refuses foreign roots, unbound stop credentials, default ports, and malformed pins", () => {
  for (const change of [
    { root: "/tmp/bluey-test-runs-browser-legacy/run.1795250653.12345.32615190360" },
    { root: "/tmp", stateDirectory: "/tmp/bluey-jobs-qa-draft" },
    { stateDirectory: `${root}/bluey-jobs-qa-production` }, { port: 5432 }, { port: 1024 }, { port: 65536 },
    { port: "43107" }, { bindPostmasterStop: async () => true }, { assertOwnedUnit: undefined },
    { pidfdToolchain: null }, { pidfdToolchain: { helper: pidfdToolchain.helper } },
    { pidfdToolchain: { ...pidfdToolchain, helper: { ...pidfdToolchain.helper, path: "/run/foreign.py" } } },
    { pidfdToolchain: { ...pidfdToolchain, python: { ...pidfdToolchain.python, sha256: "b" } } },
    { networkNamespace: "net:[04026532555]" }, { networkNamespace: "net:[4026532555]\n" },
    { unitCgroup: "/" }, { unitCgroup: "/system.slice/x/../y" },
    { pgCapsule: expectedLocalQaLinuxC2cPgCapsule("2".repeat(32)) },
    { pgCapsule: { ...pgCapsule, environment: { ...pgCapsule.environment, LD_LIBRARY_PATH: "/usr/lib" } } },
    { pgCapsule: { ...pgCapsule, tools: { ...pgCapsule.tools, psql: { ...pgCapsule.tools.psql, path: "psql" } } } },
    { toolchain: pgCapsule.tools },
    { query: async () => {} },
  ]) assert.throws(() => validateLocalQaLinuxPgRuntimeBinding({ ...binding, ...change }), /pg_runtime_refused/);
});

test("the closed identity client does not accept arbitrary SQL, credentials or endpoint overrides", () => {
  const proof = { pid: 12345, root, port, dataDirectory };
  const databaseUrl = `postgresql://bluey_qa_local:${"a".repeat(64)}@127.0.0.1:${port}/bluey_jobs_qa?sslmode=disable`;
  const request = { ...proof, sql: LINUX_DURABLE_PG_IDENTITY_SQL, databaseUrl };
  assert.doesNotThrow(() => requireLocalQaLinuxPgIdentityRequest(request, proof, databaseUrl));
  for (const change of [{ sql: "SELECT 1" }, { port: 5432 }, { pid: 12346 }, { dataDirectory: root },
    { databaseUrl: databaseUrl.replace("127.0.0.1", "192.0.2.17") }, { extra: "closed" }]) {
    assert.throws(() => requireLocalQaLinuxPgIdentityRequest({ ...request, ...change }, proof, databaseUrl),
      /pg_runtime_refused/);
  }
});

test("only the current postmaster's PostgreSQL 17 ready marker allows the listener proof", () => {
  const proof = { pid: 12345, dataDirectory, port };
  const text = [proof.pid, dataDirectory, "1795251131", port, "", "127.0.0.1", "   12  33", "ready   ", ""].join("\n");
  assert.equal(isLocalQaLinuxPostmasterReady(Buffer.from(text), proof), true);
  assert.equal(isLocalQaLinuxPostmasterReady(Buffer.from(text.replace("ready   ", "starting")), proof), false);
  assert.equal(isLocalQaLinuxPostmasterReady(Buffer.from(text.split("\n").slice(0, 3).join("\n")), proof), false);
  for (const drift of [{ pid: 12346 }, { port: 43108 }, { dataDirectory: root }]) {
    assert.throws(() => isLocalQaLinuxPostmasterReady(Buffer.from(text), { ...proof, ...drift }),
      /BLUEY_LOCAL_QA_LINUX_POSTGRES_REFUSED/);
  }
});

test("the stopped PostgreSQL ready record matches the existing durable fixture format on reconstruct", () => {
  const identity = { qaId: pgUid, rootIdentity: "1:2:1001:16832", dataIdentity: "1:3:1001:16832" };
  const configHashes = Object.fromEntries(["postgresql.conf", "postgresql.auto.conf", "pg_hba.conf", "pg_ident.conf", "PG_VERSION"]
    .map(name => [name, "b".repeat(64)]));
  const marker = { version: 1, ...identity, systemIdentifier: "9223372036854775808",
    passwordSha256: "a".repeat(64), configHashes };
  const encode = value => Buffer.from(`${JSON.stringify(value)}\n`);
  assert.deepEqual(parseLocalQaLinuxPgReadyMarker(encode(marker), identity), marker);
  assert.throws(() => parseLocalQaLinuxPgReadyMarker(encode(marker), { ...identity, dataIdentity: "1:4:1001:16832" }));
  for (const drift of [{ systemIdentifier: "18446744073709551616" }, { qaId: "other" }, { version: 0 },
    { configHashes: { ...configHashes, recovered: "b".repeat(64) } },
    { configHashes: { ...configHashes, PG_VERSION: "invalid" } }, { passwordSha256: "invalid" }]) {
    assert.throws(() => parseLocalQaLinuxPgReadyMarker(encode({ ...marker, ...drift }), identity), /pg_ready_refused/);
  }
});

test("Linux PG ready, tools and runtime never reach the owner in an unbound launcher", async () => {
  let sideEffects = 0;
  const assertUnit = async () => { sideEffects += 1; return true; };
  await assert.rejects(createLocalQaLinuxPgRuntime({ ...binding, assertOwnedUnit: assertUnit }), /pg_runtime_refused/);
  await assert.rejects(createLocalQaLinuxPgReady({ root, stateDirectory, assertUnit }), /pg_ready_refused/);
  await assert.rejects(createLocalQaLinuxPgTools({ root, stateDirectory, assertUnit, pgCapsule, port }), /PG_TOOLS_REFUSED/);
  await assert.rejects(createLocalQaLinuxPidfdBinder({ root, stateDirectory, assertOwnedUnit: assertUnit,
    unitCgroup: binding.unitCgroup, networkNamespace: binding.networkNamespace, pidfdToolchain }), /LINUX_PIDFD_REFUSED/);
  assert.equal(sideEffects, 0);
});

test("the pidfd client admits only the exact next private response", () => {
  for (const [before, good, after] of [
    ["await_bound", "bound\n", "bound"],
    ["stop_requested", "stopped\n", "stopped"],
    ["dispose_requested", "disposed\n", "disposed"],
  ]) {
    assert.equal(nextLocalQaLinuxPidfdState(before, good), after);
    for (const bad of ["", "bound\r\n", "stopped\ndisposed\n", good + "anything",
      good.slice(0, -1), good + "\0", "unknown\n"]) {
      assert.throws(() => nextLocalQaLinuxPidfdState(before, bad), /LINUX_PIDFD_REFUSED/);
    }
  }
  for (const terminal of ["disposed", "bound", "stopped", "constructor"]) {
    assert.throws(() => nextLocalQaLinuxPidfdState(terminal, "bound\n"), /LINUX_PIDFD_REFUSED/);
  }
});

function pidfdTestStreams() {
  const child = { stdout: new PassThrough(), stdin: new PassThrough() };
  return { child, protocol: createLocalQaLinuxPidfdProtocol(child) };
}

test("a fragmented final pidfd reply requires clean EOF to complete", async () => {
  const { child, protocol } = pidfdTestStreams();
  const response = protocol.expect(2_000, true);
  child.stdout.write("dis");
  child.stdout.write("posed\n");
  assert.equal(await response, "disposed\n");
  assert.equal(protocol.complete, false);
  const ended = once(child.stdout, "end");
  child.stdout.end();
  await ended;
  assert.equal(protocol.complete, true);
  child.stdin.emit("error", new Error("synthetic EPIPE"));
  assert.equal(protocol.complete, false);
});

test("pidfd framing refuses unsolicited, oversized, pipelined and post-dispose output", async () => {
  for (const corrupt of [Buffer.alloc(65, 0x61), Buffer.from("bound\nstopped\n")]) {
    const { child, protocol } = pidfdTestStreams();
    const request = assert.rejects(protocol.expect(2_000), /LINUX_PIDFD_REFUSED/);
    child.stdout.write(corrupt);
    await request;
    assert.equal(protocol.complete, false);
  }
  {
    const { child, protocol } = pidfdTestStreams();
    child.stdout.write("bound\n");
    await assert.rejects(protocol.expect(2_000), /LINUX_PIDFD_REFUSED/);
  }
  {
    const { child, protocol } = pidfdTestStreams();
    const response = protocol.expect(2_000, true);
    child.stdout.write("disposed\n");
    assert.equal(await response, "disposed\n");
    child.stdout.write("stopped\n");
    child.stdout.end();
    assert.equal(protocol.complete, false);
  }
});
