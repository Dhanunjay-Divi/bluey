import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, realpath, readFile, writeFile, chmod, rename, mkdir, symlink, link, rm, lstat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { inspect } from "node:util";
import { initializeLocalQaState, inspectLocalQaState, loadLocalQaSecrets,
  claimLocalQaPostgresInitialization } from "./local-qa-state.mjs";
import { LOCAL_QA_OBJECT_MARKER } from "./local-qa-object-store.mjs";

async function fixture(t) {
  const parent = await realpath(await mkdtemp(join(tmpdir(), "local-qa-state-")));
  // Only this test-created temporary parent is removed, never a product state directory.
  t.after(() => rm(parent, { recursive: true, force: true }));
  return { parent, path: join(parent, "bluey-jobs-qa-test") };
}

test("init and repeat inspection preserve private authority and directories", async t => {
  const { path } = await fixture(t);
  const first = await initializeLocalQaState(path);
  const secrets = await loadLocalQaSecrets(path);
  const markerBefore = await readFile(join(path, ".bluey-local-qa.json"));
  assert.deepEqual(await initializeLocalQaState(path), first);
  assert.deepEqual(await inspectLocalQaState(path), first);
  for (const name of ["jwtSecret", "jobsDataKey", "objectAccessKey", "objectSecret"]) {
    assert((await loadLocalQaSecrets(path))[name] === secrets[name], "stable private key changed");
  }
  assert.deepEqual(await readFile(join(path, ".bluey-local-qa.json")), markerBefore);
  assert.equal((await lstat(path)).mode & 0o7777, 0o700);
  for (const name of ["jwt.key", "jobs-data.key", "object-access.key", "object-secret.key", ".bluey-local-qa.json"]) {
    assert.equal((await lstat(join(path, name))).mode & 0o7777, 0o600);
  }
  assert.equal(Buffer.from(secrets.jobsDataKey, "base64").length, 32);
});

test("public status and default secret serialization do not disclose keys", async t => {
  const { path } = await fixture(t);
  const state = await initializeLocalQaState(path);
  const secrets = await loadLocalQaSecrets(path);
  assert.deepEqual(Object.keys(secrets), []);
  assert.equal(JSON.stringify(secrets), '"[Local QA secrets redacted]"');
  const outputs = JSON.stringify(state) + inspect(secrets) + JSON.stringify(secrets)
    + (await readFile(join(path, ".bluey-local-qa.json"), "utf8"));
  for (const name of ["jwtSecret", "jobsDataKey", "objectAccessKey", "objectSecret"]) {
    assert(!outputs.includes(secrets[name]));
  }
});

test("PostgreSQL initialization intent is durable, one-way and does not change secrets", async t => {
  const { path } = await fixture(t);
  const before = await initializeLocalQaState(path), keys = await loadLocalQaSecrets(path);
  assert.equal(before.postgresInitialization, "unclaimed");
  const claimed = await claimLocalQaPostgresInitialization(path);
  assert.equal(claimed.postgresInitialization, "claimed");
  assert.equal(claimed.qaId, before.qaId);
  assert((await loadLocalQaSecrets(path)).jwtSecret === keys.jwtSecret, "private authority changed");
  await assert.rejects(claimLocalQaPostgresInitialization(path), /postgres_already_claimed/);
  assert.deepEqual(await initializeLocalQaState(path), claimed);
});

test("ambiguous PostgreSQL claim intent is retained rather than overwritten", async t => {
  const { path } = await fixture(t);
  await initializeLocalQaState(path);
  const pending = join(path, ".postgres-claim.pending");
  await writeFile(pending, "uncertain prior attempt\n", { mode: 0o600 });
  await assert.rejects(claimLocalQaPostgresInitialization(path));
  assert.equal(await readFile(pending, "utf8"), "uncertain prior attempt\n");
  assert.equal((await inspectLocalQaState(path)).postgresInitialization, "unclaimed");
});

test("unknown existing state is never overwritten or silently repaired", async t => {
  const { path } = await fixture(t);
  await mkdir(path, { mode: 0o700 });
  await writeFile(join(path, "sentinel"), "owner data", { mode: 0o600 });
  await assert.rejects(initializeLocalQaState(path));
  assert.equal(await readFile(join(path, "sentinel"), "utf8"), "owner data");
  await assert.rejects(readFile(join(path, "jwt.key")), { code: "ENOENT" });
});

test("missing or changed secrets refuse instead of regenerating", async t => {
  for (const filename of ["jwt.key", "jobs-data.key", "object-access.key", "object-secret.key"]) {
    const { path } = await fixture(t);
    await initializeLocalQaState(path);
    const markerBefore = await readFile(join(path, ".bluey-local-qa.json"));
    await rename(join(path, filename), join(path, "saved-key"));
    await assert.rejects(initializeLocalQaState(path));
    await assert.rejects(readFile(join(path, filename)), { code: "ENOENT" });
    await writeFile(join(path, filename), "wrong\n", { mode: 0o600 });
    await assert.rejects(inspectLocalQaState(path), /secret_changed/);
    assert.deepEqual(await readFile(join(path, ".bluey-local-qa.json")), markerBefore);
  }
});

test("wrong permissions, symlink and hard-linked key refuse", async t => {
  const { path } = await fixture(t);
  await initializeLocalQaState(path);
  const key = join(path, "jwt.key");
  await chmod(key, 0o640);
  await assert.rejects(inspectLocalQaState(path), /file_permissions/);
  await chmod(key, 0o600);
  await link(key, join(path, "key-alias"));
  await assert.rejects(inspectLocalQaState(path), /file_permissions/);
  await rm(join(path, "key-alias"));
  await rename(key, join(path, "key-original"));
  await symlink(join(path, "key-original"), key);
  await assert.rejects(inspectLocalQaState(path));
});

test("replaced or missing child directories are not reinitialized", async t => {
  const { path } = await fixture(t);
  await initializeLocalQaState(path);
  await rename(join(path, "objects"), join(path, "old-objects"));
  await assert.rejects(initializeLocalQaState(path));
  await mkdir(join(path, "objects"), { mode: 0o700 });
  await assert.rejects(inspectLocalQaState(path), /directory_changed/);
});

test("lost or changed object-store marker refuses without regenerating durable state", async t => {
  const { path } = await fixture(t);
  await initializeLocalQaState(path);
  const stateMarker = await readFile(join(path, ".bluey-local-qa.json"));
  const file = join(path, "objects", LOCAL_QA_OBJECT_MARKER);
  const original = await readFile(file);
  await rm(file);
  await assert.rejects(initializeLocalQaState(path));
  await assert.rejects(readFile(file), { code: "ENOENT" });
  await writeFile(file, Buffer.concat([original, Buffer.from("\n")]), { mode: 0o600 });
  await assert.rejects(inspectLocalQaState(path), /object_store_changed/);
  assert.deepEqual(await readFile(join(path, ".bluey-local-qa.json")), stateMarker);
});

test("malformed, duplicate and unknown marker fields refuse", async t => {
  const { path } = await fixture(t);
  await initializeLocalQaState(path);
  const file = join(path, ".bluey-local-qa.json");
  const original = await readFile(file, "utf8");
  for (const bytes of ["{}\n", original.replace('"schemaVersion": 1,', '"schemaVersion": 1,\n  "schemaVersion": 1,'),
    JSON.stringify({ ...JSON.parse(original), surprise: true }, null, 2) + "\n", "x".repeat(4097)]) {
    await writeFile(file, bytes);
    await assert.rejects(inspectLocalQaState(path));
  }
});

test("unsafe names, Git ancestors and nonprivate parents refuse before mutation", async t => {
  const { parent } = await fixture(t);
  for (const path of ["relative/bluey-jobs-qa-test", parent, join(parent, "other-name"),
    join(parent, "bluey-jobs-qa-test") + "/", parent + "/../bluey-jobs-qa-test"]) {
    await assert.rejects(initializeLocalQaState(path));
  }
  const path = join(parent, "bluey-jobs-qa-refused");
  await writeFile(join(parent, ".git"), "gitdir: nowhere\n");
  await assert.rejects(initializeLocalQaState(path), /git_overlap/);
  await assert.rejects(lstat(path), { code: "ENOENT" });
  await rm(join(parent, ".git"));
  await chmod(parent, 0o755);
  await assert.rejects(initializeLocalQaState(path), /directory_permissions/);
  assert.equal((await lstat(parent)).mode & 0o777, 0o755);
  await chmod(parent, 0o700);
});
