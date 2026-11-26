import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { PassThrough, Writable } from "node:stream";
import { test } from "node:test";
import { ARTIFACT_CAPS, canonicalArtifactJson, copyArtifactStream, guestReaderArguments,
  guestReaderDiscoveryArguments, validateReaderApproval, validateReaderObservation,
  validateReaderProbe } from "./managed-runner-artifact-stream.mjs";

const approval = () => ({ readerSourceSha256: "a".repeat(64), runtime: {
  canonicalPath: "/usr/bin/python3.12", version: "3.12.3", platform: "Linux", architecture: "x86_64",
  uid: 0, mode: 0o755, nlink: 1, size: 12345, sha256: "b".repeat(64), protocolVersion: 1,
} });

function child() {
  const value = new EventEmitter();
  value.stdout = new PassThrough({ highWaterMark: 8 });
  value.stderr = new PassThrough();
  value.signals = [];
  value.kill = (signal) => {
    value.signals.push(signal);
    value.stdout.destroy();
    value.stdin?.destroy();
    setImmediate(() => value.emit("close", null, signal));
    return true;
  };
  return value;
}

function pair({ reply = null, delay = 0, writerCode = 0, readerCode = 0, stderr = "", writeError = false } = {}) {
  const reader = child();
  const writer = child();
  const chunks = [];
  writer.stdin = new Writable({ highWaterMark: 8, write(chunk, _encoding, done) {
    if (writeError) { done(new Error("synthetic broken pipe")); return; }
    chunks.push(Buffer.from(chunk));
    if (delay) setTimeout(done, delay); else done();
  } });
  writer.stdin.once("finish", () => {
    const value = reply ?? { name: "report.json", size: Buffer.concat(chunks).length,
      sha256: "c".repeat(64), identity: {} };
    writer.stdout.end(`${canonicalArtifactJson(value)}\n`);
    if (stderr) writer.stderr.write(stderr);
    setImmediate(() => writer.emit("close", writerCode, null));
  });
  async function send(parts) {
    for (const part of parts) {
      if (reader.stdout.destroyed) break;
      if (!reader.stdout.write(part) && !reader.stdout.destroyed) {
        await new Promise((done) => {
          const finish = () => {
            reader.stdout.off("drain", finish);
            reader.stdout.off("close", finish);
            done();
          };
          reader.stdout.once("drain", finish);
          reader.stdout.once("close", finish);
        });
      }
    }
    if (!reader.stdout.destroyed) reader.stdout.end();
    reader.emit("close", readerCode, null);
  }
  return { reader, writer, send, chunks };
}

test("portable reader approval has no per-run inode/device authority", () => {
  assert.deepEqual(validateReaderApproval(approval(), "a".repeat(64)), approval());
  for (const change of [
    { dev: "1" }, { ino: "2" }, { uid: 501 }, { mode: 0o777 }, { nlink: 2 },
    { canonicalPath: "/tmp/python3.12" }, { platform: "Darwin" }, { architecture: "arm64" },
    { size: 0 }, { sha256: "not-a-digest" }, { protocolVersion: 2 },
  ]) assert.throws(() => validateReaderApproval({ ...approval(), runtime: { ...approval().runtime, ...change } },
    "a".repeat(64)), /artifact_reader_approval/);
  assert.throws(() => validateReaderApproval(approval(), "d".repeat(64)));
  const argv = guestReaderArguments("guest-probe", approval(), approval());
  assert.deepEqual(argv.slice(0, 8), ["/usr/bin/sudo", "-n", "/usr/bin/env", "-i", "PATH=/usr/bin:/bin",
    "/usr/bin/python3.12", "-I", "-S"]);
  assert(!argv.includes("sh"));
  assert.match(argv.at(-1), /^[A-Za-z0-9_-]+$/);
  assert.equal(Buffer.from(argv.at(-1), "base64url").toString("ascii"), canonicalArtifactJson(approval()));
  assert.throws(() => guestReaderArguments("arbitrary", approval(), {}));
});

test("control encoding is sorted ASCII canonical including Unicode paths", () => {
  assert.equal(canonicalArtifactJson({ z: "é😀", a: 1 }), '{"a":1,"z":"\\u00e9\\ud83d\\ude00"}');
  assert([...Buffer.from(canonicalArtifactJson({ path: "é" }))].every((byte) => byte < 128));
});

test("reader discovery uses only a fixed bootstrap and cannot grant qualification", () => {
  const candidate = approval();
  const value = { version: 1, qualified: false, candidate,
    claims: { image: false, runnerStartup: false, release: false } };
  const args = guestReaderDiscoveryArguments(candidate.readerSourceSha256);
  assert.deepEqual(args.slice(0, -1), ["/usr/bin/sudo", "-n", "/usr/bin/env", "-i",
    "PATH=/usr/bin:/bin", "/usr/bin/python3", "-I", "-S", "-B",
    "/var/tmp/bluey-jobs-artifact-reader-p639/reader.py", "guest-discover"]);
  assert.equal(Buffer.from(args.at(-1), "base64url").toString("ascii"),
    canonicalArtifactJson({ readerSourceSha256: candidate.readerSourceSha256 }));
  assert.deepEqual(validateReaderObservation(value, candidate.readerSourceSha256), candidate);
  for (const invalid of [
    { ...value, qualified: true }, { ...value, version: 2 }, { ...value, extra: true },
    { ...value, claims: { ...value.claims, release: true } },
    { ...value, claims: { image: false, runnerStartup: false } },
    { ...value, candidate: { ...candidate, continuity: { dev: "1", ino: "2" } } },
    { ...value, candidate: { ...candidate, runtime: { ...candidate.runtime, canonicalPath: "/tmp/python" } } },
  ]) assert.throws(() => validateReaderObservation(invalid, candidate.readerSourceSha256));
  for (const invalid of ["", "a".repeat(63), "A".repeat(64), "x;sh", null, 1]) {
    assert.throws(() => guestReaderDiscoveryArguments(invalid), /artifact_reader_discovery/);
  }
  assert.throws(() => validateReaderObservation(value, "e".repeat(64)));
});

test("probe requires actual closed success and per-run continuity separately", () => {
  const probe = { version: 1, reader: { runtime: approval().runtime,
    readerSourceSha256: approval().readerSourceSha256, continuity: { dev: "1", ino: "2" } },
  noFollowProbe: true, sameFdProbe: true };
  assert.deepEqual(validateReaderProbe(probe, approval()), probe.reader);
  for (const change of [
    { ...probe, noFollowProbe: false }, { ...probe, sameFdProbe: false },
    { ...probe, extra: true }, { ...probe, reader: { ...probe.reader, continuity: { dev: "1", ino: "0" } } },
    { ...probe, reader: { ...probe.reader, runtime: { ...probe.reader.runtime, sha256: "e".repeat(64) } } },
  ]) assert.throws(() => validateReaderProbe(change, approval()), /artifact_reader_probe/);
});

test("stream transports with backpressure without accumulating payload", async () => {
  const fixture = pair({ delay: 2 });
  const parts = Array.from({ length: 40 }, () => Buffer.alloc(128, 7));
  let active;
  const copied = copyArtifactStream({ ...fixture, artifact: "report.json", timeoutMs: 1000,
    setActive: (value) => { active = value; } });
  await fixture.send(parts);
  const result = await copied;
  assert.equal(result.size, 5120);
  assert.deepEqual(Buffer.concat(fixture.chunks), Buffer.concat(parts));
  assert.equal(active, undefined);
  assert.deepEqual(fixture.reader.signals, []);
});

test("stream refuses byte overflow, stderr and nonzero terminals", async () => {
  for (const options of [{ stderr: "artifact_source_changed\n" }, { stderr: "x".repeat(65537) },
    { writerCode: 1 }, { readerCode: 1 }, { writeError: true }]) {
    const fixture = pair(options);
    const copied = copyArtifactStream({ ...fixture, artifact: "report.json", timeoutMs: 1000,
      setActive() {} });
    const rejected = assert.rejects(copied, /artifact_stream_failed/);
    await fixture.send([Buffer.from("synthetic")]);
    await rejected;
  }
  const fixture = pair();
  const copied = copyArtifactStream({ ...fixture, artifact: "report.json", timeoutMs: 1000,
    setActive() {} });
  const rejected = assert.rejects(copied);
  fixture.reader.stdout.end(Buffer.alloc(ARTIFACT_CAPS["report.json"] + 8192 + 9));
  await rejected;
  assert(fixture.reader.signals.includes("SIGTERM"));
  assert(fixture.writer.signals.includes("SIGTERM"));
});

test("stream timeout or explicit interruption terminates both children", async () => {
  for (const interrupt of [false, true]) {
    const fixture = pair();
    let active;
    const copied = copyArtifactStream({ ...fixture, artifact: "report.json", timeoutMs: 10,
      setActive: (value) => { active = value; } });
    const rejected = assert.rejects(copied, /artifact_stream_(?:timeout|failed)/);
    if (interrupt) active.terminate();
    await rejected;
    assert(fixture.reader.signals.includes("SIGTERM"));
    assert(fixture.writer.signals.includes("SIGTERM"));
  }
});

test("spawn error closes the other transport child without a success reply", async () => {
  const fixture = pair();
  const copied = copyArtifactStream({ ...fixture, artifact: "report.json", timeoutMs: 1000,
    setActive() {} });
  const rejected = assert.rejects(copied, /artifact_stream_failed/);
  fixture.reader.emit("error", new Error("synthetic spawn failure"));
  await rejected;
  assert(fixture.writer.signals.includes("SIGTERM"));
});

test("stream rejects noncanonical or extra writer reply fields", async () => {
  for (const reply of [
    { name: "other", size: 1, sha256: "a".repeat(64), identity: {} },
    { name: "report.json", size: 1, sha256: "a".repeat(64), identity: {}, extra: true },
  ]) {
    const fixture = pair({ reply });
    const copied = copyArtifactStream({ ...fixture, artifact: "report.json", timeoutMs: 1000,
      setActive() {} });
    const rejected = assert.rejects(copied, /artifact_writer_reply/);
    await fixture.send([Buffer.from("x")]);
    await rejected;
  }
});
