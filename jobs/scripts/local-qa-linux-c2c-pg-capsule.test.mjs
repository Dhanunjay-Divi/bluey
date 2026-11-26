import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { deriveLocalQaLinuxC2cPgCapsule, LOCAL_QA_LINUX_C2C_PG_CAPSULE_PINS, validateLocalQaLinuxC2cPgCapsulePlan } from "./local-qa-linux-c2c-pg-capsule.mjs";

assert.equal(typeof process.env.C2C_PG_MANIFEST, "string");
const manifest = await readFile(process.env.C2C_PG_MANIFEST);
const input = () => ({ run: "a".repeat(32), manifestBytes: manifest, archiveSha256: LOCAL_QA_LINUX_C2C_PG_CAPSULE_PINS.archiveSha256 });

test("PG capsule plan pins only the Noble PG17/vector 0.8.7 closure", () => {
  const plan = deriveLocalQaLinuxC2cPgCapsule(input());
  assert.equal(validateLocalQaLinuxC2cPgCapsulePlan(plan), true);
  assert.equal(plan.tools.postgres.path, "/run/bluey-jobs-c2c-postgres/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/capsule/usr/lib/postgresql/17/bin/postgres");
  assert.deepEqual(Object.keys(plan.environment).sort(), ["LANG", "LC_ALL", "LD_LIBRARY_PATH", "PATH", "TZ"]);
  assert.equal(plan.environment.LD_LIBRARY_PATH, plan.libpqSearchPath);
  assert.deepEqual(plan.postgresArguments, ["-c", `dynamic_library_path=${plan.extensionLibraryPath}`]);
  assert.equal(plan.vectorVersion, "0.8.7");
  assert.deepEqual(plan.libraries.libpqLink, {
    path: `${plan.libpqSearchPath}/libpq.so.5`, type: "link", mode: 0o777, target: "libpq.so.5.18",
  });
  assert.equal(plan.libraries.vector.path, `${plan.extensionLibraryPath}/vector.so`);
});

test("PG capsule refuses archive, manifest, run, and loader substitutions", () => {
  assert.throws(() => deriveLocalQaLinuxC2cPgCapsule({ ...input(), archiveSha256: "0".repeat(64) }));
  assert.throws(() => deriveLocalQaLinuxC2cPgCapsule({ ...input(), manifestBytes: Buffer.from(manifest.subarray(0, -1)) }));
  assert.throws(() => deriveLocalQaLinuxC2cPgCapsule({ ...input(), run: "../bad" }));
  const plan = deriveLocalQaLinuxC2cPgCapsule(input());
  assert.throws(() => validateLocalQaLinuxC2cPgCapsulePlan({ ...plan, environment: { ...plan.environment, LD_LIBRARY_PATH: "/lib" } }));
  assert.throws(() => validateLocalQaLinuxC2cPgCapsulePlan({ ...plan, tools: { ...plan.tools,
    postgres: { ...plan.tools.postgres, path: "/usr/bin/postgres" } } }));
  assert.throws(() => validateLocalQaLinuxC2cPgCapsulePlan({ ...plan, tools: { ...plan.tools,
    postgres: { ...plan.tools.postgres, sha256: "0".repeat(64) } } }));
  assert.throws(() => validateLocalQaLinuxC2cPgCapsulePlan({ ...plan, postgresArguments: ["-c", "x", "extra"] }));
  assert.throws(() => validateLocalQaLinuxC2cPgCapsulePlan({ ...plan, environment: { ...plan.environment, HOME: "/root" } }));
});

test("PG plan rejects extra authority fields at each loader boundary", () => {
  const plan = deriveLocalQaLinuxC2cPgCapsule(input());
  assert.throws(() => validateLocalQaLinuxC2cPgCapsulePlan({ ...plan, fallbackToSystem: true }));
  assert.throws(() => validateLocalQaLinuxC2cPgCapsulePlan({
    ...plan, postgresArguments: Object.assign([...plan.postgresArguments], { env: { HOME: "/root" } }),
  }));
  assert.throws(() => validateLocalQaLinuxC2cPgCapsulePlan({
    ...plan, environment: Object.defineProperty({ ...plan.environment }, "LD_PRELOAD", { value: "/tmp/lib.so" }),
  }));
});

test("PG plan refuses missing or substituted extension and libpq pins", () => {
  const plan = deriveLocalQaLinuxC2cPgCapsule(input());
  for (const [name, pin] of Object.entries(plan.libraries)) {
    const missing = { ...plan.libraries };
    delete missing[name];
    assert.throws(() => validateLocalQaLinuxC2cPgCapsulePlan({ ...plan, libraries: missing }));
    assert.throws(() => validateLocalQaLinuxC2cPgCapsulePlan({
      ...plan, libraries: { ...plan.libraries, [name]: pin.type === "link"
        ? { ...pin, target: "/usr/lib/libpq.so.5" } : { ...pin, sha256: "0".repeat(64) } },
    }));
  }
});
