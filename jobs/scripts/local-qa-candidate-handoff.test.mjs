import test from "node:test";
import assert from "node:assert/strict";
import { lstatSync, readdirSync, readFileSync } from "node:fs";
import { lstat, mkdtemp, open, readFile, readdir, realpath, rename, rm,
  symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  createLocalQaCandidateHandoffOwner,
  revokeLocalQaCandidateHandoff,
  writeLocalQaCandidateHandoff,
} from "./local-qa-candidate-handoff.mjs";

const PASSWORD = `Qa-${"a".repeat(48)}`;

async function fixture(t) {
  const path = await realpath(await mkdtemp(join(tmpdir(), "local-qa-candidate-handoff-")));
  t.after(() => rm(path, { recursive: true, force: true }));
  return path;
}

function options(directory, overrides = {}) {
  return { directory, email: "candidate@bluey-qa.test", expiresAtMs: Date.now() + 60_000,
    origin: "http://127.0.0.1:43125", password: PASSWORD, ...overrides };
}

function deferred() {
  let resolve, reject;
  const promise = new Promise((accept, refuse) => { resolve = accept; reject = refuse; });
  return { promise, resolve, reject };
}

test("writes one canonical candidate-only private handoff", async t => {
  const directory = await fixture(t);
  const before = Date.now(), input = options(directory);
  const result = await writeLocalQaCandidateHandoff(input);
  const after = Date.now();
  assert.deepEqual(result, { path: join(directory, "candidate-login.json"),
    expiresAtMs: input.expiresAtMs });
  const info = await lstat(result.path);
  assert.equal(info.isFile(), true);
  assert.equal(info.nlink, 1);
  assert.equal(info.mode & 0o7777, 0o600);
  assert.deepEqual(await readdir(directory), ["candidate-login.json"]);
  const bytes = await readFile(result.path, "utf8");
  const expected = JSON.parse(bytes);
  assert(expected.issuedAtMs >= before && expected.issuedAtMs <= after);
  assert.deepEqual(expected, { version: 1, kind: "bluey_jobs_local_qa_candidate_login",
    origin: "http://127.0.0.1:43125", email: "candidate@bluey-qa.test",
    password: PASSWORD, issuedAtMs: expected.issuedAtMs, expiresAtMs: input.expiresAtMs });
  assert.equal(bytes, `${JSON.stringify(expected)}\n`);
  assert.deepEqual(Object.keys(JSON.parse(bytes)),
    ["version", "kind", "origin", "email", "password", "issuedAtMs", "expiresAtMs"]);
  for (const forbidden of ["operator", "token", "config", "access_token", "refresh_token"])
    assert.equal(bytes.includes(forbidden), false);
});

test("duplicate and symlink destinations refuse without replacement", async t => {
  const directory = await fixture(t);
  const first = await writeLocalQaCandidateHandoff(options(directory));
  const original = await readFile(first.path);
  await assert.rejects(writeLocalQaCandidateHandoff(options(directory)),
    /local_qa_candidate_handoff_refused/);
  assert.deepEqual(await readFile(first.path), original);

  const second = await fixture(t);
  const target = join(second, "target.json");
  await writeFile(target, "unchanged\n", { mode: 0o600 });
  await symlink(target, join(second, "candidate-login.json"));
  await assert.rejects(writeLocalQaCandidateHandoff(options(second)),
    /local_qa_candidate_handoff_refused/);
  assert.equal(await readFile(target, "utf8"), "unchanged\n");
  assert.deepEqual((await readdir(second)).sort(), ["candidate-login.json", "target.json"]);
});

test("only complete verified bytes become visible at the fixed handoff name", async t => {
  const directory = await fixture(t);
  const now = Date.now(), input = options(directory);
  const path = join(directory, "candidate-login.json");
  const seen = [];
  let reads = 0;
  t.mock.method(Date, "now", () => {
    if (++reads === 2) {
      const files = readdirSync(directory);
      seen.push({ files, bytes: files.map(file => readFileSync(join(directory, file), "utf8")),
        finalExists: Boolean(lstatSync(path, { throwIfNoEntry: false })) });
    }
    return now;
  });
  const result = await writeLocalQaCandidateHandoff(input);
  assert.equal(seen.length, 1);
  assert.equal(seen[0].finalExists, false);
  assert.equal(seen[0].files.length, 1);
  assert.deepEqual(seen[0].bytes, [await readFile(result.path, "utf8")]);
  await revokeLocalQaCandidateHandoff(result);
  assert.deepEqual(await readdir(directory), []);
});

test("expiry during staging or final verification leaves no credential file", async t => {
  for (const expiryCheck of [2, 3]) {
    const directory = await fixture(t);
    const now = Date.now(), input = options(directory, { expiresAtMs: now + 10_000 });
    let reads = 0;
    const clock = t.mock.method(Date, "now", () => ++reads < expiryCheck ? now : input.expiresAtMs);
    try {
      await assert.rejects(writeLocalQaCandidateHandoff(input), /local_qa_candidate_handoff_refused/);
      assert.equal(reads, expiryCheck);
      assert.deepEqual(await readdir(directory), []);
    } finally { clock.mock.restore(); }
  }
});

test("disagreeing final readback refuses to return a handoff capability", async t => {
  const directory = await fixture(t);
  const sentinel = await open(join(directory, "unrelated.txt"), "wx", 0o600);
  const prototype = Object.getPrototypeOf(sentinel);
  await sentinel.close();
  const read = prototype.read;
  let starts = 0;
  t.mock.method(prototype, "read", async function (buffer, offset, length, position) {
    const result = await read.call(this, buffer, offset, length, position);
    // Inject a mismatching byte only in the final descriptor's read response.
    // No file or metadata is altered; the next read can prove safe cleanup.
    if (offset === 0 && result.bytesRead && ++starts === 2) buffer[0] ^= 1;
    return result;
  });
  await assert.rejects(writeLocalQaCandidateHandoff(options(directory)),
    { message: "local_qa_candidate_handoff_refused" });
  assert.equal(starts, 3);
  assert.deepEqual(await readdir(directory), ["unrelated.txt"]);
});

test("concurrent publication yields one complete handoff and no staging leftovers", async t => {
  const directory = await fixture(t);
  const results = await Promise.allSettled(Array.from({ length: 6 }, () =>
    writeLocalQaCandidateHandoff(options(directory))));
  const accepted = results.filter(result => result.status === "fulfilled");
  assert.equal(accepted.length, 1);
  for (const result of results.filter(result => result.status === "rejected"))
    assert.equal(result.reason.message, "local_qa_candidate_handoff_refused");
  const publication = accepted[0].value;
  assert.equal((await lstat(publication.path)).nlink, 1);
  assert.deepEqual(await readdir(directory), ["candidate-login.json"]);
  await revokeLocalQaCandidateHandoff(publication);
  assert.deepEqual(await readdir(directory), []);
});

test("only the exact publication capability can revoke the private file", async t => {
  const directory = await fixture(t);
  const result = await writeLocalQaCandidateHandoff(options(directory));
  await assert.rejects(revokeLocalQaCandidateHandoff({ ...result }), /local_qa_candidate_handoff_refused/);
  assert((await lstat(result.path)).isFile());
  await revokeLocalQaCandidateHandoff(result);
  await assert.rejects(lstat(result.path), { code: "ENOENT" });
  await assert.rejects(revokeLocalQaCandidateHandoff(result), /local_qa_candidate_handoff_refused/);
});

test("revocation refuses substituted files and preserves the replacement", async t => {
  const directory = await fixture(t);
  const result = await writeLocalQaCandidateHandoff(options(directory));
  await rename(result.path, join(directory, "previous.json"));
  await writeFile(result.path, "unrelated", { mode: 0o600, flag: "wx" });
  await assert.rejects(revokeLocalQaCandidateHandoff(result), /local_qa_candidate_handoff_cleanup_unproven/);
  assert.equal(await readFile(result.path, "utf8"), "unrelated");
});

test("invalid or expired candidate login options refuse before writing", async t => {
  const directory = await fixture(t);
  const now = Date.now();
  const invalid = [
    { email: "operator@bluey-qa.test" },
    { password: `Qa-${"g".repeat(48)}` },
    { origin: "https://127.0.0.1:43125" },
    { origin: "http://localhost:43125" },
    { origin: "http://127.0.0.1:43125/jobs" },
    { expiresAtMs: now },
    { expiresAtMs: now - 1 },
    { expiresAtMs: now + 2 * 60 * 60 * 1000 + 10_000 },
  ];
  for (const delta of invalid) {
    await assert.rejects(writeLocalQaCandidateHandoff(options(directory, delta)),
      /local_qa_candidate_handoff_refused/);
  }
  await assert.rejects(writeLocalQaCandidateHandoff({ ...options(directory), extra: true }),
    /local_qa_candidate_handoff_refused/);
  await assert.rejects(lstat(join(directory, "candidate-login.json")), { code: "ENOENT" });
});

test("directory must already be canonical current-owner mode 0700", async t => {
  const directory = await fixture(t);
  await import("node:fs/promises").then(({ chmod }) => chmod(directory, 0o755));
  await assert.rejects(writeLocalQaCandidateHandoff(options(directory)),
    /local_qa_candidate_handoff_refused/);

  const parent = await fixture(t);
  const target = await fixture(t);
  const linked = join(parent, "linked");
  await symlink(target, linked, "dir");
  await assert.rejects(writeLocalQaCandidateHandoff(options(linked)),
    /local_qa_candidate_handoff_refused/);
});

test("failure after exclusive creation removes only the newly created handoff", async t => {
  const directory = await fixture(t);
  const sibling = join(directory, "unrelated.txt");
  await writeFile(sibling, "unchanged");
  const previous = process.umask(0o777);
  try {
    await assert.rejects(writeLocalQaCandidateHandoff(options(directory)),
      { message: "local_qa_candidate_handoff_refused" });
  } finally { process.umask(previous); }
  await assert.rejects(lstat(join(directory, "candidate-login.json")), { code: "ENOENT" });
  assert.equal(await readFile(sibling, "utf8"), "unchanged");
  assert.deepEqual(await readdir(directory), ["unrelated.txt"]);
});

test("candidate handoff owner publishes once and coalesces exact revocation", async t => {
  const directory = await fixture(t);
  let active = true, prepares = 0;
  const owner = createLocalQaCandidateHandoffOwner({
    isActive: () => active,
    prepare: async () => { prepares += 1; return options(directory); },
  });
  assert.equal(Object.isFrozen(owner), true);
  assert.deepEqual(Object.keys(owner).sort(), ["publish", "revoke"]);

  const publication = await owner.publish();
  assert.equal(prepares, 1);
  assert.deepEqual(publication, {
    path: join(directory, "candidate-login.json"),
    expiresAtMs: publication.expiresAtMs,
  });
  assert.equal((await lstat(publication.path)).mode & 0o7777, 0o600);
  await assert.rejects(owner.publish(), { message: "local_qa_candidate_handoff_refused" });
  assert.equal(prepares, 1);

  const first = owner.revoke();
  const second = owner.revoke();
  active = false;
  await Promise.all([first, second]);
  await assert.rejects(lstat(publication.path), { code: "ENOENT" });
  assert.deepEqual(await readdir(directory), []);
  await assert.rejects(owner.publish(), { message: "local_qa_candidate_handoff_refused" });
  assert.equal(prepares, 1);
});

test("revocation disarms a pending publication before prepared options can write", async t => {
  const directory = await fixture(t);
  const gate = deferred();
  let prepares = 0;
  const owner = createLocalQaCandidateHandoffOwner({
    isActive: () => true,
    prepare: async () => { prepares += 1; return gate.promise; },
  });

  const publication = owner.publish();
  await assert.rejects(owner.publish(), { message: "local_qa_candidate_handoff_refused" });
  assert.equal(prepares, 1);
  const first = owner.revoke();
  const second = owner.revoke();
  gate.resolve(options(directory));

  await assert.rejects(publication, { message: "local_qa_candidate_handoff_refused" });
  await Promise.all([first, second]);
  assert.deepEqual(await readdir(directory), []);
  await assert.rejects(owner.publish(), { message: "local_qa_candidate_handoff_refused" });
  assert.equal(prepares, 1);
});

test("candidate handoff owner refuses inactive and expired publication boundaries", async t => {
  const inactiveDirectory = await fixture(t);
  let prepares = 0;
  const inactive = createLocalQaCandidateHandoffOwner({
    isActive: () => false,
    prepare: async () => { prepares += 1; return options(inactiveDirectory); },
  });
  await assert.rejects(inactive.publish(), { message: "local_qa_candidate_handoff_refused" });
  assert.equal(prepares, 0);
  assert.deepEqual(await readdir(inactiveDirectory), []);

  const expiredDirectory = await fixture(t);
  const expired = createLocalQaCandidateHandoffOwner({
    isActive: () => true,
    prepare: async () => options(expiredDirectory, { expiresAtMs: Date.now() }),
  });
  await assert.rejects(expired.publish(), { message: "local_qa_candidate_handoff_refused" });
  assert.deepEqual(await readdir(expiredDirectory), []);

  const changedDirectory = await fixture(t);
  const gate = deferred(), started = deferred();
  let active = true;
  const changed = createLocalQaCandidateHandoffOwner({
    isActive: () => active,
    prepare: async () => { started.resolve(); return gate.promise; },
  });
  const pending = changed.publish();
  await started.promise;
  active = false;
  gate.resolve(options(changedDirectory));
  await assert.rejects(pending, { message: "local_qa_candidate_handoff_refused" });
  assert.deepEqual(await readdir(changedDirectory), []);
});

test("later teardown still refuses an already reported ambiguous publication", async t => {
  const directory = await fixture(t);
  const uncertain = { message: "local_qa_candidate_handoff_cleanup_unproven" };
  const owner = createLocalQaCandidateHandoffOwner({
    isActive: () => true,
    async prepare() { throw new Error(uncertain.message); },
  });
  await assert.rejects(owner.publish(), uncertain);
  await assert.rejects(owner.revoke(), uncertain);
  assert.deepEqual(await readdir(directory), []);
});

test("candidate handoff owner preserves a substituted credential on failed revoke", async t => {
  const directory = await fixture(t);
  const owner = createLocalQaCandidateHandoffOwner({
    isActive: () => true,
    prepare: async () => options(directory),
  });
  const publication = await owner.publish();
  await rename(publication.path, join(directory, "previous.json"));
  await writeFile(publication.path, "replacement\n", { mode: 0o600, flag: "wx" });

  const first = owner.revoke();
  const second = owner.revoke();
  await assert.rejects(first, { message: "local_qa_candidate_handoff_cleanup_unproven" });
  await assert.rejects(second, { message: "local_qa_candidate_handoff_cleanup_unproven" });
  assert.equal(await readFile(publication.path, "utf8"), "replacement\n");
  assert.deepEqual((await readdir(directory)).sort(), ["candidate-login.json", "previous.json"]);
});
