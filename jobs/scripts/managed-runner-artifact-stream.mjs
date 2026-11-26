// Byte-only D1 transport. Callers admit both executables/argv before constructing children.
import { Transform } from "node:stream";
import { pipeline } from "node:stream/promises";

export const ARTIFACT_CAPS = Object.freeze({
  "runner.oci.tar": 4 * 1024 ** 3,
  "inventory.json": 32 * 1024 ** 2,
  "measurement.json": 128 * 1024,
  "report.json": 16 * 1024,
});
export const ARTIFACT_READER_PATH =
  "/var/tmp/bluey-jobs-artifact-reader-p639/reader.py";
const check = (value, code) => { if (!value) throw new Error(code); };

export function canonicalArtifactJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalArtifactJson).join(",")}]`;
  if (value && Object.getPrototypeOf(value) === Object.prototype) {
    return `{${Object.keys(value).sort().map((key) =>
      `${canonicalArtifactJson(key)}:${canonicalArtifactJson(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value)?.replace(/[\u007f-\uffff]/g,
    (character) => `\\u${character.charCodeAt(0).toString(16).padStart(4, "0")}`);
}

export function validateReaderApproval(value, sourceSha256) {
  const exact = (object, names) => check(object && Object.getPrototypeOf(object) === Object.prototype
    && Object.keys(object).sort().join("\0") === [...names].sort().join("\0"), "artifact_reader_approval");
  exact(value, ["runtime", "readerSourceSha256"]);
  exact(value.runtime, ["canonicalPath", "version", "platform", "architecture", "uid", "mode",
    "nlink", "size", "sha256", "protocolVersion"]);
  const runtime = value.runtime;
  check(value.readerSourceSha256 === sourceSha256 && /^[a-f0-9]{64}$/.test(sourceSha256)
    && /^\/usr\/bin\/python3\.[0-9]{1,2}$/.test(runtime.canonicalPath)
    && /^3\.[0-9]{1,2}\.[0-9]{1,3}$/.test(runtime.version)
    && runtime.platform === "Linux" && runtime.architecture === "x86_64"
    && runtime.uid === 0 && runtime.nlink === 1 && runtime.protocolVersion === 1
    && Number.isInteger(runtime.mode) && runtime.mode >= 0 && runtime.mode <= 0o777
    && (runtime.mode & 0o022) === 0 && (runtime.mode & 0o111) !== 0
    && Number.isSafeInteger(runtime.size) && runtime.size > 0 && runtime.size <= 128 * 1024 ** 2
    && /^[a-f0-9]{64}$/.test(runtime.sha256), "artifact_reader_approval");
  return value;
}

export function guestReaderArguments(operation, approval, request) {
  check(["guest-probe", "guest-read"].includes(operation), "artifact_reader_operation");
  return ["/usr/bin/sudo", "-n", "/usr/bin/env", "-i", "PATH=/usr/bin:/bin",
    approval.runtime.canonicalPath, "-I", "-S", "-B", ARTIFACT_READER_PATH,
    operation, Buffer.from(canonicalArtifactJson(request), "ascii").toString("base64url")];
}

export function guestReaderDiscoveryArguments(readerSourceSha256) {
  check(typeof readerSourceSha256 === "string" && /^[a-f0-9]{64}$/.test(readerSourceSha256),
    "artifact_reader_discovery");
  return ["/usr/bin/sudo", "-n", "/usr/bin/env", "-i", "PATH=/usr/bin:/bin",
    "/usr/bin/python3", "-I", "-S", "-B", ARTIFACT_READER_PATH, "guest-discover",
    Buffer.from(canonicalArtifactJson({ readerSourceSha256 }), "ascii").toString("base64url")];
}

export function validateReaderObservation(value, sourceSha256) {
  const exact = (object, names) => check(object && Object.getPrototypeOf(object) === Object.prototype
    && Object.keys(object).sort().join("\0") === [...names].sort().join("\0"),
  "artifact_reader_discovery");
  exact(value, ["version", "qualified", "candidate", "claims"]);
  exact(value.claims, ["image", "runnerStartup", "release"]);
  check(value.version === 1 && value.qualified === false
    && Object.values(value.claims).every((claim) => claim === false), "artifact_reader_discovery");
  return validateReaderApproval(value.candidate, sourceSha256);
}

export function validateReaderProbe(probe, approval) {
  const exact = (value, names) => check(value && Object.getPrototypeOf(value) === Object.prototype
    && Object.keys(value).sort().join("\0") === [...names].sort().join("\0"), "artifact_reader_probe");
  exact(probe, ["version", "reader", "noFollowProbe", "sameFdProbe"]);
  exact(probe.reader, ["runtime", "continuity", "readerSourceSha256"]);
  exact(probe.reader.continuity, ["dev", "ino"]);
  check(probe.version === 1 && probe.noFollowProbe === true && probe.sameFdProbe === true
    && canonicalArtifactJson(probe.reader.runtime) === canonicalArtifactJson(approval.runtime)
    && probe.reader.readerSourceSha256 === approval.readerSourceSha256
    && Object.values(probe.reader.continuity).every((value) => typeof value === "string"
      && /^(?:0|[1-9][0-9]{0,19})$/.test(value)) && probe.reader.continuity.ino !== "0",
  "artifact_reader_probe");
  return probe.reader;
}

export async function copyArtifactStream({ reader, writer, artifact, timeoutMs, setActive }) {
  check(Object.hasOwn(ARTIFACT_CAPS, artifact) && Number.isSafeInteger(timeoutMs)
    && timeoutMs > 0 && timeoutMs <= 15 * 60_000, "artifact_stream_scope");
  const limit = ARTIFACT_CAPS[artifact] + 2 * 4096 + 8;
  let bytes = 0;
  let readerError = Buffer.alloc(0);
  let writerError = Buffer.alloc(0);
  let reply = Buffer.alloc(0);
  let refused = false;
  let timedOut = false;
  let escalation;
  const terminate = () => {
    if (refused) return;
    refused = true;
    reader.kill("SIGTERM");
    writer.kill("SIGTERM");
    escalation = setTimeout(() => { reader.kill("SIGKILL"); writer.kill("SIGKILL"); }, 5000);
  };
  const bounded = (previous, chunk) => {
    if (previous.length + chunk.length > 65536) { terminate(); return previous; }
    return Buffer.concat([previous, chunk]);
  };
  const closed = (child) => new Promise((done) => {
    child.once("error", () => { terminate(); done({ code: null, signal: "spawn_error" }); });
    child.once("close", (code, signal) => done({ code, signal }));
  });
  const readerClosed = closed(reader);
  const writerClosed = closed(writer);
  reader.stderr.on("data", (chunk) => { readerError = bounded(readerError, chunk); });
  writer.stderr.on("data", (chunk) => { writerError = bounded(writerError, chunk); });
  writer.stdout.on("data", (chunk) => { reply = bounded(reply, chunk); });
  const limiter = new Transform({ transform(chunk, _encoding, done) {
    bytes += chunk.length;
    if (bytes > limit) { terminate(); done(new Error("artifact_stream_bound")); }
    else done(null, chunk);
  } });
  setActive({ terminate });
  const timer = setTimeout(() => { timedOut = true; terminate(); }, timeoutMs);
  let transportFailed = false;
  try {
    await pipeline(reader.stdout, limiter, writer.stdin).catch(() => { transportFailed = true; terminate(); });
    const terminals = await Promise.all([readerClosed, writerClosed]);
    check(!timedOut, "artifact_stream_timeout");
    check(!refused && !transportFailed && terminals.every((item) => item.code === 0 && item.signal === null)
      && readerError.length === 0 && writerError.length === 0, "artifact_stream_failed");
    const text = reply.toString("ascii");
    check(reply.every((byte) => byte < 128) && text.endsWith("\n"), "artifact_writer_reply");
    const value = JSON.parse(text);
    check(`${canonicalArtifactJson(value)}\n` === text && value.name === artifact
      && Object.keys(value).sort().join("\0") === ["identity", "name", "sha256", "size"].join("\0")
      && Number.isSafeInteger(value.size) && value.size > 0 && value.size <= ARTIFACT_CAPS[artifact]
      && /^[a-f0-9]{64}$/.test(value.sha256), "artifact_writer_reply");
    return value;
  } finally {
    clearTimeout(timer);
    clearTimeout(escalation);
    setActive(undefined);
  }
}
