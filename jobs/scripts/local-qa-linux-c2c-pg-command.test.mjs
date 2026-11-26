import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { deriveLocalQaLinuxC2cPgCapsule, LOCAL_QA_LINUX_C2C_PG_CAPSULE_PINS } from "./local-qa-linux-c2c-pg-capsule.mjs";
import { composeLocalQaLinuxC2cPgCommands } from "./local-qa-linux-c2c-pg-command.mjs";

assert.equal(typeof process.env.C2C_PG_MANIFEST, "string");
const manifest = await readFile(process.env.C2C_PG_MANIFEST);
const plan = () => deriveLocalQaLinuxC2cPgCapsule({
  run: "a".repeat(32), manifestBytes: manifest, archiveSha256: LOCAL_QA_LINUX_C2C_PG_CAPSULE_PINS.archiveSha256,
});
const state = () => Object.freeze({
  home: "/tmp/bluey-test-runs-abcdef012345/run.1.42.789/bluey-jobs-qa-draft",
  dataDirectory: "/tmp/bluey-test-runs-abcdef012345/run.1.42.789/bluey-jobs-qa-draft/postgres",
  passwordFile: "/tmp/bluey-test-runs-abcdef012345/run.1.42.789/bluey-jobs-qa-draft/.local-qa-postgres-password",
});
const input = () => ({ capsulePlan: plan(), state: state(), port: 55432 });

test("composes the only capsule-scoped PG commands", () => {
  const commands = composeLocalQaLinuxC2cPgCommands(input());
  const capsule = plan();
  assert.deepEqual(commands.initdb.argv, ["-D", state().dataDirectory, "-L", capsule.sharePath, "-U", "bluey_qa_local",
    "--auth-local=scram-sha-256", "--auth-host=scram-sha-256", "--pwfile", state().passwordFile,
    "--encoding=UTF8", "--no-locale"]);
  assert.deepEqual(commands.postgres.argv.slice(-2), ["-c", `dynamic_library_path=${capsule.extensionLibraryPath}`]);
  assert.deepEqual(commands.pgControldata.argv, [state().dataDirectory]);
  assert.deepEqual(commands.psql.postgres.argv.slice(-2), ["-d", "postgres"]);
  assert.deepEqual(commands.psql.bluey_jobs_qa.argv.slice(-2), ["-d", "bluey_jobs_qa"]);
  assert.equal(commands.psql.postgres.path, capsule.tools.psql.path);
  assert.equal(commands.runtimeQualified, false);
  assert.deepEqual(commands.initdb.env, {
    HOME: state().home, LANG: "C", LC_ALL: "C", LD_LIBRARY_PATH: capsule.libpqSearchPath,
    PATH: "/usr/bin:/bin", TZ: "UTC",
  });
  assert.equal(Object.isFrozen(commands), true);
  assert.equal(Object.isFrozen(commands.postgres.argv), true);
});

test("rejects mutated capsule, state escape, port, and ambient environment authority", () => {
  const capsule = plan();
  assert.throws(() => composeLocalQaLinuxC2cPgCommands({ ...input(), capsulePlan: {
    ...capsule, environment: { ...capsule.environment, LD_LIBRARY_PATH: "/usr/lib" },
  } }));
  assert.throws(() => composeLocalQaLinuxC2cPgCommands({ ...input(), state: {
    ...state(), dataDirectory: "/tmp/postgres",
  } }));
  assert.throws(() => composeLocalQaLinuxC2cPgCommands({ ...input(), state: {
    home: "/var/lib/postgresql", dataDirectory: "/var/lib/postgresql/postgres",
    passwordFile: "/var/lib/postgresql/.local-qa-postgres-password",
  } }));
  assert.throws(() => composeLocalQaLinuxC2cPgCommands({ ...input(), port: 5432 }));
  assert.throws(() => composeLocalQaLinuxC2cPgCommands({ ...input(), environment: process.env }));
});

test("does not inherit global loader or execution environment", () => {
  const keys = ["LD_LIBRARY_PATH", "LD_PRELOAD"];
  const before = keys.map(key => process.env[key]);
  process.env.LD_LIBRARY_PATH = "/untrusted";
  process.env.LD_PRELOAD = "/untrusted.so";
  try {
    const commands = composeLocalQaLinuxC2cPgCommands(input());
    assert.equal(commands.postgres.env.LD_LIBRARY_PATH, plan().libpqSearchPath);
    assert.equal(Object.hasOwn(commands.postgres.env, "LD_PRELOAD"), false);
    assert.equal(Object.hasOwn(commands.postgres.env, "NODE_OPTIONS"), false);
    assert.deepEqual(Object.keys(commands.postgres.env).sort(), ["HOME", "LANG", "LC_ALL", "LD_LIBRARY_PATH", "PATH", "TZ"]);
  } finally {
    for (const [index, key] of keys.entries()) {
      if (before[index] === undefined) delete process.env[key];
      else process.env[key] = before[index];
    }
  }
});
