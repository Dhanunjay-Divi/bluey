// Join only independently held files and the released actor's terminal frame.
// This never grants browser/DB execution or supplies outer closure evidence.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { fstatSync, readSync } from "node:fs";
import { TextDecoder } from "node:util";
import { proveManagedReceiptPostgres } from "./managed-receipt-postgres-proof.mjs";
import { verifyBrowserReceiptQualification } from "./managed-receipt-browser-qualification.mjs";

const SHA = /^[0-9a-f]{64}$/;
const NONCE = /^[0-9a-f]{32}$/;
const BOOT = /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
const LIMITS = { stdout: 2 * 1024 ** 2, stderr: 4 * 1024 ** 2 };
const BINDING = ["run", "boot", "invocation", "challenge"];
const identity = (st) => [st.dev, st.ino, st.uid, st.mode, st.nlink, st.size, st.mtimeNs, st.ctimeNs];
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");

function fields(row, expected, label) {
  assert(row && typeof row === "object" && !Array.isArray(row), label);
  assert.deepEqual(Object.keys(row).sort(), [...expected].sort(), label);
}

function validScope(scope) {
  return BINDING.every((key) => typeof scope[key] === "string"
    && (key === "boot" ? BOOT : NONCE).test(scope[key])) && scope.invocation !== "0".repeat(32);
}

function utf8(bytes) {
  const text = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
  assert(Buffer.from(text).equals(bytes) && !text.includes("\0"), "receipt_stream_encoding");
  return text;
}

function parseFrame(frame) {
  assert(Buffer.isBuffer(frame) && frame.length > 0 && frame.length <= 4096, "receipt_terminal_bound");
  const row = JSON.parse(utf8(frame));
  fields(row, ["event", ...BINDING, "code", "signalNumber", "stdoutBytes", "stderrBytes",
    "stdoutSha256", "stderrSha256", "inputReadback", "runtimeAdmitted"], "receipt_terminal_fields");
  const canonical = Object.fromEntries(Object.keys(row).sort().map((key) => [key, row[key]]));
  assert(frame.equals(Buffer.from(JSON.stringify(canonical) + "\n")), "receipt_terminal_noncanonical");
  assert(row.event === "jobs_browser_owned_wrapper_terminal" && row.inputReadback === true
    && row.runtimeAdmitted === false && validScope(row), "receipt_terminal_policy");
  assert((row.signalNumber === null && Number.isInteger(row.code) && row.code >= 0 && row.code <= 255)
    || (row.code === null && Number.isInteger(row.signalNumber) && row.signalNumber > 0
      && row.signalNumber <= 64), "receipt_terminal_status");
  return row;
}

function checkedRead(stream, owner, size, expected, limit) {
  fields(stream, ["fd", "device", "inode"], "receipt_stream_binding");
  assert(Number.isInteger(stream.fd) && stream.fd >= 3 && typeof stream.device === "bigint"
    && stream.device >= 0n && typeof stream.inode === "bigint" && stream.inode > 0n,
  "receipt_stream_descriptor");
  assert(Number.isInteger(size) && size >= 0 && size <= limit && typeof expected === "string"
    && SHA.test(expected), "receipt_stream_bound");
  const first = fstatSync(stream.fd, { bigint: true });
  assert(first.isFile() && first.dev === stream.device && first.ino === stream.inode
    && first.uid === BigInt(owner) && first.nlink === 1n && (first.mode & 0o7777n) === 0o600n
    && first.size === BigInt(size), "receipt_stream_identity");
  const bytes = Buffer.alloc(size);
  for (let offset = 0; offset < size;) {
    const n = readSync(stream.fd, bytes, offset, size - offset, offset);
    assert(n > 0, "receipt_stream_short_read");
    offset += n;
  }
  assert.equal(readSync(stream.fd, Buffer.alloc(1), 0, 1, size), 0, "receipt_stream_trailing");
  assert.deepEqual(identity(fstatSync(stream.fd, { bigint: true })), identity(first),
    "receipt_stream_changed");
  assert.equal(digest(bytes), expected, "receipt_stream_digest");
  return utf8(bytes);
}

export function collectReceiptBrowserTerminal(frame, expected, streams, unitTerminal) {
  fields(expected, [...BINDING, "root", "actorUid"], "receipt_trusted_release");
  assert(validScope(expected) && typeof expected.root === "string"
    && /^\/tmp\/bluey-test-runs-[a-f0-9]{12}\/run\.[1-9][0-9]{0,15}\.[1-9][0-9]{0,9}\.[0-9]{3,12}$/.test(expected.root)
    && Number.isSafeInteger(expected.actorUid) && expected.actorUid > 0, "receipt_trusted_scope");
  fields(streams, ["stdout", "stderr"], "receipt_streams");
  fields(unitTerminal, ["code", "signal", "deadline"], "receipt_unit_terminal_fields");
  assert(unitTerminal.code === 0 && unitTerminal.signal === null && unitTerminal.deadline === false,
    "receipt_unit_terminal");
  const row = parseFrame(frame);
  assert(BINDING.every((key) => row[key] === expected[key]), "receipt_release_binding");
  assert(row.code === 0 && row.signalNumber === null, "receipt_wrapper_terminal");
  assert(streams.stdout && streams.stderr && streams.stdout.fd !== streams.stderr.fd
    && (streams.stdout.device !== streams.stderr.device || streams.stdout.inode !== streams.stderr.inode),
  "receipt_stream_alias");
  const wrapper = {
    terminal: { code: 0, signal: null }, success: true, bad: false,
    stdout: checkedRead(streams.stdout, expected.actorUid, row.stdoutBytes, row.stdoutSha256, LIMITS.stdout),
    stderr: checkedRead(streams.stderr, expected.actorUid, row.stderrBytes, row.stderrSha256, LIMITS.stderr),
  };
  const proof = proveManagedReceiptPostgres(wrapper, expected.root, "interview-prep-browser", "linux-x64");
  return { wrapper, proof, terminalFrameSha256: digest(frame), runtimeAdmitted: false };
}

export function qualifyCollectedReceiptBrowser({
  terminalFrame, release, streams, terminalArtifacts, supervisorTranscript, trustedLaunch,
}) {
  // The separately admitted launch, not the completed transcript, binds
  // the terminal frame and raw files to the cgroup/network verification.
  const admitted = [...BINDING, "root", "actorUid"];
  fields(trustedLaunch, ["case", "timeout"], "receipt_launch_records");
  const launchFields = [...admitted, "unit", "rootDevice", "rootInode", "mountId", "eventsInode", "releaseSha256"];
  fields(trustedLaunch.case, launchFields, "receipt_launch_case_fields");
  fields(trustedLaunch.timeout, launchFields, "receipt_launch_timeout_fields");
  assert(release && admitted.every((key) => release[key] === trustedLaunch.case[key]), "receipt_launch_scope");
  fields(terminalArtifacts, ["case", "timeout"], "receipt_terminal_artifacts");
  const unitTerminal = readTerminalArtifact(terminalArtifacts.case);
  const timeoutTerminal = readTerminalArtifact(terminalArtifacts.timeout);
  assert(terminalArtifacts.case.fd !== terminalArtifacts.timeout.fd
    && (terminalArtifacts.case.device !== terminalArtifacts.timeout.device
      || terminalArtifacts.case.inode !== terminalArtifacts.timeout.inode), "receipt_terminal_artifact_alias");
  bindSupervisorTerminal(unitTerminal, trustedLaunch.case);
  bindSupervisorTerminal(timeoutTerminal, trustedLaunch.timeout);
  assert.notEqual(timeoutTerminal.challenge, unitTerminal.challenge, "receipt_unit_reused_challenge");
  const observed = collectReceiptBrowserTerminal(terminalFrame, release, streams, unitTerminal.status);
  const outerBinding = ({ actorUid: _actorUid, challenge: _challenge, releaseSha256: _release, ...launch }) => launch;
  const closure = verifyBrowserReceiptQualification(observed.wrapper, supervisorTranscript, {
    case: outerBinding(trustedLaunch.case), timeout: outerBinding(trustedLaunch.timeout),
  });
  assert.deepEqual(JSON.parse(supervisorTranscript).caseScope.terminal, unitTerminal.status,
    "receipt_unit_terminal_mismatch");
  assert.deepEqual(JSON.parse(supervisorTranscript).timeoutScope.terminal, timeoutTerminal.status,
    "receipt_timeout_terminal_mismatch");
  return { ...closure, terminalFrameSha256: observed.terminalFrameSha256,
    unitTerminalSha256: terminalArtifacts.case.sha256, timeoutTerminalSha256: terminalArtifacts.timeout.sha256,
    runtimeAdmitted: false };
}

function readTerminalArtifact(reference) {
  fields(reference, ["fd", "device", "inode", "bytes", "sha256"], "receipt_terminal_artifact_reference");
  const { fd, device, inode, bytes, sha256 } = reference;
  assert(Number.isInteger(bytes) && bytes > 0, "receipt_terminal_artifact_size");
  const text = checkedRead({ fd, device, inode }, 0, bytes, sha256, 4096);
  return parseSupervisorTerminal(text);
}

// Pure parser for tests and diagnostics. It proves bytes/shape, not custody.
export function parseSupervisorTerminal(text) {
  assert(typeof text === "string" && text.length > 0 && Buffer.byteLength(text) <= 4096,
    "receipt_supervisor_terminal_size");
  const record = JSON.parse(text);
  fields(record, [...BINDING, "root", "actorUid", "unit", "status", "releaseSha256"],
    "receipt_supervisor_terminal_fields");
  fields(record.status, ["code", "deadline", "signal"], "receipt_unit_terminal_fields");
  const sorted = (row) => Object.fromEntries(Object.keys(row).sort().map((key) => [key, row[key]]));
  assert.equal(JSON.stringify(sorted({ ...record, status: sorted(record.status) })) + "\n", text,
    "receipt_supervisor_terminal_noncanonical");
  const { code, deadline, signal } = record.status;
  assert((code === 0 && deadline === false && signal === null)
    || (code === null && deadline === true && ["SIGTERM", "SIGKILL"].includes(signal)),
  "receipt_supervisor_terminal_status");
  assert(validScope(record) && Number.isSafeInteger(record.actorUid) && record.actorUid > 0
    && record.actorUid < 2 ** 32 - 1 && typeof record.releaseSha256 === "string"
    && SHA.test(record.releaseSha256), "receipt_supervisor_terminal_scope");
  return record;
}

function bindSupervisorTerminal(record, launch) {
  assert([...BINDING, "root", "actorUid", "unit", "releaseSha256"].every((key) => record[key] === launch[key]),
    "receipt_unit_terminal_scope");
}
