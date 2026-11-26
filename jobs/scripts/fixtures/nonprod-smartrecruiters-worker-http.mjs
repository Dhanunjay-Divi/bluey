import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { closeSync, fstatSync, lstatSync, openSync, readFileSync, readSync, realpathSync } from "node:fs";
import { registerHooks } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// This is a child-side synthetic client only.  The parent owns the loopback
// router, its signed responses, and process admission.
const SELF = fileURLToPath(import.meta.url);
const REPO = realpathSync(resolve(dirname(SELF), "../../.."));
const AUTOMATION = join(REPO, "jobs/automation/src");
const WORKFLOWS = join(REPO, "jobs/workflows/src");
const PROVIDER_FIXTURE = join(REPO, "jobs/automation/tests/fixtures/smartrecruiters-hydration-detail.json");
const PROVIDER_FIXTURE_SHA256 = "acce3a2c29c4125232d053cd34134a1af9106ca52a6fd2d02b14bf9cef6e9eaf";
const SIGNING_KEY = "synthetic-smartrecruiters-wire-signing-key-v1";
const SHA256 = /^[a-f0-9]{64}$/;
const NONCE = /^[a-f0-9]{32}$/;
const MAX_INPUT_BYTES = 256 * 1024;
const PROVIDER_URL = "https://api.smartrecruiters.com/v1/companies/acme/postings/smart-1";

assert.deepEqual(process.argv.slice(2), ["--synthetic-http-retry"]);
assert.equal(process.cwd(), REPO);
assert.equal(typeof globalThis.fetch, "function");

const runRoot = process.env.BLUEY_TEST_RUN_ROOT;
assert.match(runRoot ?? "", /^\/private\/tmp\/bluey-test-runs-[a-z0-9]{12}\/run\.\d+\.\d+\.[A-Za-z0-9]+$/);
assert.equal(realpathSync(runRoot), runRoot);
const rootInfo = lstatSync(runRoot);
assert.ok(rootInfo.isDirectory() && !rootInfo.isSymbolicLink());
assert.equal(rootInfo.uid, process.getuid());
assert.equal(rootInfo.mode & 0o777, 0o700);
const marker = join(runRoot, ".bluey-test-run-v2");
const markerInfo = lstatSync(marker);
assert.ok(markerInfo.isFile() && !markerInfo.isSymbolicLink());
assert.equal(markerInfo.uid, process.getuid());
assert.ok(markerInfo.size > 0 && markerInfo.size <= 4096);
assert.match(readFileSync(marker, "utf8"), /(?:^|\n)state=running\n/);

function modulePath(base, leaf) {
  const path = join(base, leaf);
  assert.equal(realpathSync(path), path);
  return pathToFileURL(path).href;
}

const modules = {
  originalSource: modulePath(AUTOMATION, "original-source-verification.ts"),
  publicAts: modulePath(AUTOMATION, "public-ats.ts"),
  discoveryQuality: modulePath(AUTOMATION, "discovery-quality.ts"),
  jobSourceIntelligence: modulePath(AUTOMATION, "job-source-intelligence.ts"),
  policy: modulePath(AUTOMATION, "policy.ts"),
  adapters: modulePath(AUTOMATION, "adapters.ts"),
  atsTarget: modulePath(AUTOMATION, "ats-target.ts"),
  adapterCapabilities: modulePath(AUTOMATION, "adapter-capabilities.ts"),
  contracts: modulePath(AUTOMATION, "contracts.ts"),
  workerAuth: modulePath(AUTOMATION, "worker-auth.ts"),
  api: modulePath(WORKFLOWS, "original-source-verification-api.ts"),
  runtime: modulePath(WORKFLOWS, "original-source-verification-runtime.ts"),
};

const resolutions = new Map([
  [`${import.meta.url}\0${modules.originalSource}`, modules.originalSource],
  [`${import.meta.url}\0${modules.api}`, modules.api],
  [`${import.meta.url}\0${modules.runtime}`, modules.runtime],
  [`${modules.originalSource}\0./public-ats.js`, modules.publicAts],
  [`${modules.publicAts}\0./contracts.js`, modules.contracts],
  [`${modules.publicAts}\0./discovery-quality.js`, modules.discoveryQuality],
  [`${modules.publicAts}\0./job-source-intelligence.js`, modules.jobSourceIntelligence],
  [`${modules.publicAts}\0./policy.js`, modules.policy],
  [`${modules.jobSourceIntelligence}\0./contracts.js`, modules.contracts],
  [`${modules.policy}\0./adapters.js`, modules.adapters],
  [`${modules.policy}\0./ats-target.js`, modules.atsTarget],
  [`${modules.policy}\0./adapter-capabilities.js`, modules.adapterCapabilities],
  [`${modules.adapters}\0./contracts.js`, modules.contracts],
  [`${modules.adapters}\0./ats-target.js`, modules.atsTarget],
  [`${modules.atsTarget}\0./contracts.js`, modules.contracts],
  [`${modules.adapterCapabilities}\0./contracts.js`, modules.contracts],
  [`${modules.api}\0@bluey/jobs-automation/original-source-verification`, modules.originalSource],
  [`${modules.api}\0@bluey/jobs-automation/worker-auth`, modules.workerAuth],
  [`${modules.runtime}\0@bluey/jobs-automation/original-source-verification`, modules.originalSource],
  [`${modules.runtime}\0./original-source-verification-api.js`, modules.api],
]);
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith("node:")) return nextResolve(specifier, context);
    const target = resolutions.get(`${context.parentURL ?? ""}\0${specifier}`);
    assert.ok(target, `unapproved module edge: ${String(specifier)}`);
    return { shortCircuit: true, url: target };
  },
});

function checkedBytes(path, maximum, digest) {
  const descriptor = openSync(path, "r");
  try {
    const before = lstatSync(path, { bigint: true });
    const opened = fstatSync(descriptor, { bigint: true });
    assert.ok(before.isFile() && !before.isSymbolicLink() && before.nlink === 1n);
    assert.deepEqual([opened.dev, opened.ino, opened.mode, opened.nlink, opened.size], [before.dev, before.ino, before.mode, before.nlink, before.size]);
    assert.ok(before.uid === BigInt(process.getuid()) || before.uid === 0n);
    assert.equal(Number(before.mode & 0o022n), 0);
    assert.ok(before.size > 0n && before.size <= BigInt(maximum));
    const bytes = Buffer.alloc(Number(before.size) + 1);
    const count = readSync(descriptor, bytes, 0, bytes.length, 0);
    const after = lstatSync(path, { bigint: true });
    assert.deepEqual([before.dev, before.ino, before.size, before.mtimeNs, before.ctimeNs], [after.dev, after.ino, after.size, after.mtimeNs, after.ctimeNs]);
    assert.equal(count, Number(before.size));
    const exact = bytes.subarray(0, count);
    assert.equal(createHash("sha256").update(exact).digest("hex"), digest);
    return exact;
  } finally { closeSync(descriptor); }
}

async function stdinJson() {
  const chunks = []; let size = 0;
  for await (const chunk of process.stdin) {
    const bytes = Buffer.from(chunk); size += bytes.length;
    assert.ok(size > 0 && size <= MAX_INPUT_BYTES); chunks.push(bytes);
  }
  const bytes = Buffer.concat(chunks, size);
  assert.ok(bytes.length > 0 && !bytes.includes(0x0a) && !bytes.includes(0x0d));
  const value = JSON.parse(bytes.toString("utf8"));
  assert.ok(value && typeof value === "object" && !Array.isArray(value));
  assert.deepEqual(Object.keys(value).sort(), ["binding", "origin", "runtimeIdentitySha256"]);
  assert.ok(SHA256.test(value.runtimeIdentitySha256));
  return value;
}

function checkedOrigin(value) {
  assert.equal(typeof value, "string");
  const matched = /^http:\/\/127\.0\.0\.1:([1-9][0-9]{3,4})$/.exec(value);
  assert.ok(matched);
  const port = Number(matched[1]);
  assert.ok(port > 1024 && port <= 65535);
  return value;
}

function expectedPaths(assignmentId) {
  const base = `/api/jobs/internal/original-source-verifications/${assignmentId}`;
  return new Set([
    "/api/jobs/internal/original-source-verifications/lease",
    `${base}/heartbeat`, `${base}/complete`, `${base}/fail`,
  ]);
}

function assertTerminalWire(value) {
  assert.ok(value && typeof value === "object" && !Array.isArray(value));
  assert.deepEqual(Object.keys(value).sort(), ["assignment_id", "head", "receipt_sha256", "replayed", "state"]);
  if (value.head !== null) {
    assert.ok(value.head && typeof value.head === "object" && !Array.isArray(value.head));
    assert.deepEqual(Object.keys(value.head).sort(), [
      "assignment_id", "assurance", "checked_at_ms", "expires_at_ms", "head_revision",
      "material_generation", "material_sha256", "receipt_id", "receipt_sha256", "result",
      "subject_sha256",
    ]);
  }
  assert.ok(value.head !== null);
  assert.equal(value.head.receipt_sha256, value.receipt_sha256);
}

function assertAuthoritativeTerminalResponse(response) {
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("cache-control"), "no-store");
  assert.equal(response.headers.get("content-type"), "application/json");
  assert.equal(response.headers.get("x-content-type-options"), "nosniff");
}

async function main() {
  const input = await stdinJson();
  const origin = checkedOrigin(input.origin);
  const providerBytes = checkedBytes(PROVIDER_FIXTURE, 128 * 1024, PROVIDER_FIXTURE_SHA256);
  const [{ OriginalSourceVerifier }, apiModule, runtimeModule] = await Promise.all([
    import(modules.originalSource), import(modules.api), import(modules.runtime),
  ]);
  let providerCalls = 0;
  const verifier = new OriginalSourceVerifier({
    maxAttempts: 1,
    fetch: async (url, init = {}) => {
      providerCalls += 1;
      assert.equal(providerCalls, 1);
      assert.equal(String(url), PROVIDER_URL);
      assert.equal(init.method, "GET"); assert.equal(init.redirect, "error");
      assert.equal(new Headers(init.headers).get("accept"), "application/json");
      assert.equal(init.signal?.aborted, false);
      return new Response(providerBytes, { status: 200, headers: { "content-type": "application/json" } });
    },
  });

  const nativeFetch = globalThis.fetch;
  let assignmentId; let attemptId; let firstCompletion; let receiptSha256; let completionRequests = 0;
  const apiFetch = async (url, init = {}) => {
    const parsed = new URL(String(url));
    assert.equal(parsed.origin, origin);
    assert.equal(parsed.protocol, "http:"); assert.equal(parsed.hostname, "127.0.0.1");
    assert.equal(parsed.pathname.includes("//"), false);
    assert.equal(init.method, "POST"); assert.equal(init.redirect, "error");
    assert.equal(init.signal?.aborted, false);
    const body = String(init.body);
    const headers = new Headers(init.headers);
    assert.equal(headers.get("accept"), "application/json");
    assert.equal(headers.get("content-type"), "application/json");
    assert.equal(headers.get("x-bluey-jobs-worker-scope"), "original-source-verification");
    assert.equal(headers.get("x-bluey-jobs-worker-content-sha256"), createHash("sha256").update(body).digest("hex"));
    assert.ok(NONCE.test(headers.get("x-bluey-jobs-worker-nonce") ?? ""));
    if (assignmentId === undefined) {
      assert.equal(parsed.pathname, "/api/jobs/internal/original-source-verifications/lease");
      assert.deepEqual(JSON.parse(body), { binding: input.binding });
    } else assert.ok(expectedPaths(assignmentId).has(parsed.pathname));

    if (assignmentId !== undefined && parsed.pathname.endsWith("/complete")) {
      completionRequests += 1;
      const nonce = headers.get("x-bluey-jobs-worker-nonce");
      const signature = headers.get("x-bluey-jobs-worker-signature");
      if (completionRequests === 1) {
        assert.equal(typeof signature, "string"); assert.ok(signature.length > 0);
        firstCompletion = { body, nonce, signature };
        const response = await nativeFetch(url, init);
        const text = await response.text();
        assertAuthoritativeTerminalResponse(response);
        const value = JSON.parse(text);
        assertTerminalWire(value);
        assert.equal(value.replayed, false);
        assert.ok(SHA256.test(value.receipt_sha256)); receiptSha256 = value.receipt_sha256;
        throw new Error("synthetic_terminal_response_lost");
      }
      assert.equal(completionRequests, 2);
      assert.equal(body, firstCompletion.body);
      assert.notEqual(nonce, firstCompletion.nonce);
      assert.equal(typeof signature, "string"); assert.ok(signature.length > 0);
      assert.notEqual(signature, firstCompletion.signature);
      const response = await nativeFetch(url, init);
      const value = await response.clone().json();
      assertAuthoritativeTerminalResponse(response);
      assertTerminalWire(value);
      assert.equal(value.replayed, true);
      assert.equal(value.receipt_sha256, receiptSha256);
      return response;
    }
    const response = await nativeFetch(url, init);
    if (parsed.pathname.endsWith("/lease")) {
      const lease = await response.clone().json();
      assert.ok(response.ok);
      assert.equal(typeof lease.assignment_id, "string"); assert.equal(typeof lease.attempt_id, "string");
      assignmentId = lease.assignment_id; attemptId = lease.attempt_id;
    }
    return response;
  };

  const api = new apiModule.OriginalSourceVerificationApiClient({
    origin, signingKey: SIGNING_KEY, binding: input.binding, fetch: apiFetch,
    reportAttempts: 2, reportRetryMs: 0,
  });
  const runtime = new runtimeModule.OriginalSourceVerifierRuntime({
    api, verifier, logger: { log() {} }, leaseHeartbeatIntervalMs: 30_000,
    workerRuntimeIdentitySha256: input.runtimeIdentitySha256,
  });
  assert.equal(await runtime.pollOnce(), "replayed");
  assert.equal(providerCalls, 1); assert.equal(completionRequests, 2);
  assert.equal(typeof assignmentId, "string"); assert.equal(typeof attemptId, "string");
  assert.ok(SHA256.test(receiptSha256));
  process.stdout.write(`${JSON.stringify({ assignmentId, attemptId, receiptSha256, completionRequests: 2, replayed: true })}\n`);
}

const deadline = setTimeout(() => process.exit(70), 9_000);
try { await main(); } finally { clearTimeout(deadline); }
