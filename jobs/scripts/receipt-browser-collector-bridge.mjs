// Fixed stdin/FD transport to the read-only receipt collector.
import assert from "node:assert/strict";
import { readSync } from "node:fs";
import { TextDecoder } from "node:util";
import { qualifyCollectedReceiptBrowser } from "./receipt-browser-terminal-collector.mjs";

const LIMIT = 65536;
const SHA = /^[0-9a-f]{64}$/;

function fields(row, expected, reason) {
  assert(row && typeof row === "object" && !Array.isArray(row), reason);
  assert.deepEqual(Object.keys(row).sort(), [...expected].sort(), reason);
}

function identity(value, minimum) {
  assert(typeof value === "string" && /^(0|[1-9][0-9]{0,19})$/.test(value), "bridge_fd_identity");
  const number = BigInt(value);
  assert(number >= minimum && number < 2n ** 64n, "bridge_fd_identity");
  return number;
}

function reference(value, terminal) {
  const keys = ["fd", "device", "inode"];
  fields(value, terminal ? [...keys, "bytes", "sha256"] : keys, "bridge_fd_fields");
  assert(Number.isInteger(value.fd) && value.fd >= 3 && value.fd < 2 ** 31, "bridge_fd_number");
  if (terminal) {
    assert(Number.isInteger(value.bytes) && value.bytes > 0 && value.bytes <= 4096
      && typeof value.sha256 === "string" && SHA.test(value.sha256), "bridge_terminal_bounds");
  }
  return { ...value, device: identity(value.device, 0n), inode: identity(value.inode, 1n) };
}

export function decodeControl(raw) {
  assert(Buffer.isBuffer(raw) && raw.length > 0 && raw.length <= LIMIT, "bridge_control_bound");
  const text = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(raw);
  const value = JSON.parse(text);
  fields(value, ["terminalFrameBase64", "release", "streams", "terminalArtifacts",
    "supervisorTranscript", "trustedLaunch"], "bridge_control_fields");
  assert.equal(JSON.stringify(value) + "\n", text, "bridge_control_noncanonical");
  const { terminalFrameBase64, streams, terminalArtifacts, ...rest } = value;
  assert(typeof terminalFrameBase64 === "string" && terminalFrameBase64.length <= 5464, "bridge_frame_bound");
  const terminalFrame = Buffer.from(terminalFrameBase64, "base64");
  assert(terminalFrame.length > 0 && terminalFrame.length <= 4096
    && terminalFrame.toString("base64") === terminalFrameBase64, "bridge_frame_encoding");
  fields(streams, ["stdout", "stderr"], "bridge_stream_fields");
  fields(terminalArtifacts, ["case", "timeout"], "bridge_terminal_fields");
  const out = reference(streams.stdout, false), err = reference(streams.stderr, false);
  const normal = reference(terminalArtifacts.case, true), timeout = reference(terminalArtifacts.timeout, true);
  const inputs = [out, err, normal, timeout];
  assert(new Set(inputs.map((x) => x.fd)).size === 4
    && new Set(inputs.map((x) => x.device + ":" + x.inode)).size === 4, "bridge_fd_alias");
  return { ...rest, terminalFrame, streams: { stdout: out, stderr: err },
    terminalArtifacts: { case: normal, timeout } };
}

// The supervisor invokes this module through an exact, independently pinned
// entrypoint. Importing the parser does not consume stdin or open descriptors.
export function collectFromStdin() {
  const bytes = Buffer.alloc(LIMIT + 1);
  let length = 0;
  while (length < bytes.length) {
    const n = readSync(0, bytes, length, bytes.length - length, null);
    if (n === 0) break;
    length += n;
  }
  try {
    const receipt = qualifyCollectedReceiptBrowser(decodeControl(bytes.subarray(0, length)));
    process.stdout.write(JSON.stringify(receipt) + "\n");
  } catch (error) {
    const reason = typeof error?.message === "string" && /^(?:bridge|receipt|browser)_[a-z_]+$/.test(error.message)
      ? error.message : "bridge_collector_refused";
    process.stderr.write(reason + "\n");
    process.exitCode = 1;
  }
}
