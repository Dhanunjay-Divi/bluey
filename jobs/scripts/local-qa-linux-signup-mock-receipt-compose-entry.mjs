// Bounded stdin/stdout bridge for the already-installed mock-receipt composer.
// This module neither opens paths supplied by a caller nor starts a service.
import { composeLocalQaLinuxSignupMockReceiptSpec } from "./local-qa-linux-signup-mock-receipt-spec.mjs";
import { realpathSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const MAX_REQUEST_BYTES = 128 << 10;
export const REQUEST_SCHEMA = "bluey-jobs-signup-mock-receipt-compose-request-v1";
const KEYS = [
  "application", "browser", "chromiumInventoryBase64", "codePlanBase64", "ports", "postgres",
  "python", "reservationSha256", "schema", "systemctlSha256", "waiting",
];

function refused() {
  throw new Error("local_qa_linux_signup_mock_receipt_compose_refused");
}

function plain(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    && Object.getPrototypeOf(value) === Object.prototype;
}

function exactKeys(value, keys) {
  return plain(value) && Object.keys(value).length === keys.length
    && keys.every((key) => Object.hasOwn(value, key));
}

function base64(value) {
  if (typeof value !== "string" || value.length === 0 || value.length > MAX_REQUEST_BYTES) refused();
  const bytes = Buffer.from(value, "base64");
  if (bytes.length === 0 || bytes.length > MAX_REQUEST_BYTES || bytes.toString("base64") !== value) refused();
  return bytes;
}

/** Decode compact, path-free file bytes and receipt objects before composition. */
export function decodeComposeRequest(raw) {
  if (!Buffer.isBuffer(raw) || raw.length === 0 || raw.length > MAX_REQUEST_BYTES || raw.at(-1) !== 0x0a) refused();
  let value;
  try {
    value = JSON.parse(raw.toString("utf8"));
  } catch {
    refused();
  }
  if (!exactKeys(value, KEYS) || JSON.stringify(value) + "\n" !== raw.toString("utf8")
    || value.schema !== REQUEST_SCHEMA || !["waiting", "application", "browser", "postgres", "ports", "python"].every(
      (key) => plain(value[key]),
    ) || typeof value.reservationSha256 !== "string" || typeof value.systemctlSha256 !== "string") refused();
  return Object.freeze({
    reservationSha256: value.reservationSha256,
    waiting: value.waiting,
    application: value.application,
    browser: value.browser,
    postgres: value.postgres,
    codePlanBytes: base64(value.codePlanBase64),
    chromiumInventoryBytes: base64(value.chromiumInventoryBase64),
    ports: value.ports,
    systemctlSha256: value.systemctlSha256,
    python: value.python,
  });
}

export async function readComposeRequest(stream = process.stdin) {
  const chunks = [];
  let total = 0;
  for await (const chunk of stream) {
    if (!Buffer.isBuffer(chunk) || chunk.length === 0 || (total += chunk.length) > MAX_REQUEST_BYTES) refused();
    chunks.push(chunk);
  }
  return decodeComposeRequest(Buffer.concat(chunks, total));
}

export async function composeFromStdin(input = process.stdin, output = process.stdout) {
  const request = await readComposeRequest(input);
  const result = composeLocalQaLinuxSignupMockReceiptSpec(request);
  if (!Buffer.isBuffer(result?.specBytes) || result.specBytes.length === 0) refused();
  await new Promise((resolve, reject) => output.write(result.specBytes, (error) => error ? reject(error) : resolve()));
}

if (typeof process.argv[1] === "string"
  && fileURLToPath(import.meta.url) === realpathSync(resolve(process.argv[1]))) {
  composeFromStdin().catch(() => { process.exitCode = 1; });
}
