import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  fstatSync,
  lstatSync,
  openSync,
  readFileSync,
  readSync,
  realpathSync,
  closeSync,
} from "node:fs";
import { registerHooks } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const SELF = fileURLToPath(import.meta.url);
const REPO = realpathSync(resolve(dirname(SELF), "../../.."));
const AUTOMATION = join(REPO, "jobs/automation/src");
const WORKFLOWS = join(REPO, "jobs/workflows/src");
const PROVIDER_FIXTURE = join(
  REPO,
  "jobs/automation/tests/fixtures/smartrecruiters-hydration-detail.json",
);
const PROVIDER_FIXTURE_SHA256 =
  "acce3a2c29c4125232d053cd34134a1af9106ca52a6fd2d02b14bf9cef6e9eaf";
const MAX_INPUT_BYTES = 256 * 1024;
const SHA256 = /^[a-f0-9]{64}$/;
const EXPECTED_PROVIDER_URL =
  "https://api.smartrecruiters.com/v1/companies/acme/postings/smart-1";
const MOCK_API_ORIGIN = "http://127.0.0.1:9";

assert.deepEqual(process.argv.slice(2), ["--synthetic-lease-wire"]);
assert.equal(process.cwd(), REPO);
assert.equal(process.stdin.isTTY, undefined);

const runRoot = process.env.BLUEY_TEST_RUN_ROOT;
assert.match(
  runRoot ?? "",
  /^\/private\/tmp\/bluey-test-runs-[a-z0-9]{12}\/run\.\d+\.\d+\.[A-Za-z0-9]+$/,
);
assert.equal(realpathSync(runRoot), runRoot);
const rootInfo = lstatSync(runRoot);
assert.ok(rootInfo.isDirectory());
assert.equal(rootInfo.uid, process.getuid());
assert.equal(rootInfo.mode & 0o777, 0o700);
const marker = join(runRoot, ".bluey-test-run-v2");
const markerInfo = lstatSync(marker);
assert.ok(markerInfo.isFile() && !markerInfo.isSymbolicLink());
assert.equal(markerInfo.uid, process.getuid());
assert.ok(markerInfo.size > 0 && markerInfo.size <= 4096);
const markerText = readFileSync(marker, "utf8");
assert.match(markerText, /(?:^|\n)state=running\n/);

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
  [
    `${modules.api}\0@bluey/jobs-automation/original-source-verification`,
    modules.originalSource,
  ],
  [`${modules.api}\0@bluey/jobs-automation/worker-auth`, modules.workerAuth],
  [
    `${modules.runtime}\0@bluey/jobs-automation/original-source-verification`,
    modules.originalSource,
  ],
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
    assert.deepEqual(
      [opened.dev, opened.ino, opened.mode, opened.nlink, opened.size],
      [before.dev, before.ino, before.mode, before.nlink, before.size],
    );
    assert.ok(before.uid === BigInt(process.getuid()) || before.uid === 0n);
    assert.equal(Number(before.mode & 0o022n), 0);
    assert.ok(before.size > 0n && before.size <= BigInt(maximum));
    const bytes = Buffer.alloc(Number(before.size) + 1);
    const count = readSync(descriptor, bytes, 0, bytes.length, 0);
    const after = lstatSync(path, { bigint: true });
    assert.deepEqual(
      [before.dev, before.ino, before.mode, before.nlink, before.size, before.mtimeNs, before.ctimeNs],
      [after.dev, after.ino, after.mode, after.nlink, after.size, after.mtimeNs, after.ctimeNs],
    );
    assert.equal(count, Number(before.size));
    const exact = bytes.subarray(0, count);
    assert.equal(createHash("sha256").update(exact).digest("hex"), digest);
    return exact;
  } finally {
    closeSync(descriptor);
  }
}

async function stdinBytes() {
  const chunks = [];
  let size = 0;
  for await (const chunk of process.stdin) {
    const bytes = Buffer.from(chunk);
    size += bytes.length;
    assert.ok(size > 0 && size <= MAX_INPUT_BYTES);
    chunks.push(bytes);
  }
  const input = Buffer.concat(chunks, size);
  assert.ok(input.length > 0 && !input.includes(0x0a) && !input.includes(0x0d));
  return input;
}

function exactRecord(value, keys, label) {
  assert.ok(value && typeof value === "object" && !Array.isArray(value), `${label} is invalid`);
  assert.deepEqual(Object.keys(value).sort(), [...keys].sort(), `${label} keys changed`);
  return value;
}

function authoritativeResponse(value, status = 200) {
  return new Response(status === 204 ? null : JSON.stringify(value), {
    status,
    headers:
      status === 204
        ? {}
        : {
            "cache-control": "no-store",
            "content-type": "application/json",
            "x-content-type-options": "nosniff",
          },
  });
}

async function main() {
  const input = exactRecord(
    JSON.parse((await stdinBytes()).toString("utf8")),
    ["binding", "lease", "runtimeIdentitySha256"],
    "input",
  );
  assert.ok(SHA256.test(input.runtimeIdentitySha256));

  const [{ OriginalSourceVerifier, parseOriginalSourceVerificationSubject }, apiModule, runtimeModule] =
    await Promise.all([import(modules.originalSource), import(modules.api), import(modules.runtime)]);
  const subject = parseOriginalSourceVerificationSubject(
    JSON.parse(input.lease.canonical_subject_json),
  );
  assert.deepEqual(
    {
      original_url: subject.original_url,
      provider_family: subject.provider_family,
      provider_record_id: subject.provider_record_id,
      provider_target: subject.provider_target,
      expected: subject.expected,
    },
    {
      original_url: "https://jobs.smartrecruiters.com/acme/smart-1",
      provider_family: "smartrecruiters",
      provider_record_id:
        "smartrecruiters:jobs.smartrecruiters.com:acme:smart-1",
      provider_target: {
        host: "jobs.smartrecruiters.com",
        tenant: "acme",
        job: "smart-1",
        variant: "smartrecruiters_posting",
      },
      expected: {
        availability_status: "active",
        company: "Acme",
        compensation: "USD 120000-160000 year",
        description: "Build reliable systems. SQL & Rust.",
        employment_type: "full_time",
        location: "Austin, TX",
        posted_at_ms: null,
        title: "Platform Engineer",
        workplace: "hybrid",
      },
    },
  );

  const providerBytes = checkedBytes(
    PROVIDER_FIXTURE,
    128 * 1024,
    PROVIDER_FIXTURE_SHA256,
  );
  let providerCalls = 0;
  const verifier = new OriginalSourceVerifier({
    maxAttempts: 1,
    fetch: async (url, init = {}) => {
      providerCalls += 1;
      assert.equal(providerCalls, 1);
      assert.equal(String(url), EXPECTED_PROVIDER_URL);
      assert.equal(init.method, "GET");
      assert.equal(init.redirect, "error");
      assert.equal(new Headers(init.headers).get("accept"), "application/json");
      assert.equal(init.signal?.aborted, false);
      return new Response(providerBytes, {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    },
  });

  const heartbeatBodies = [];
  let completionBody;
  const paths = [];
  const apiFetch = async (url, init = {}) => {
    const parsed = new URL(String(url));
    assert.equal(parsed.origin, MOCK_API_ORIGIN);
    assert.equal(init.method, "POST");
    assert.equal(init.redirect, "error");
    assert.equal(init.signal?.aborted, false);
    const headers = new Headers(init.headers);
    assert.equal(headers.get("accept"), "application/json");
    assert.equal(headers.get("content-type"), "application/json");
    assert.equal(headers.get("x-bluey-jobs-worker-scope"), "original-source-verification");
    const bodyText = String(init.body);
    assert.equal(
      headers.get("x-bluey-jobs-worker-content-sha256"),
      createHash("sha256").update(bodyText).digest("hex"),
    );
    const body = JSON.parse(bodyText);
    paths.push(parsed.pathname);

    if (paths.length === 1) {
      assert.equal(parsed.pathname, "/api/jobs/internal/original-source-verifications/lease");
      assert.deepEqual(body, { binding: input.binding });
      return authoritativeResponse(input.lease);
    }
    const heartbeatPath =
      `/api/jobs/internal/original-source-verifications/${input.lease.assignment_id}/heartbeat`;
    if (paths.length === 2 || paths.length === 3) {
      assert.equal(parsed.pathname, heartbeatPath);
      assert.deepEqual(Object.keys(body).sort(), [
        "assignment_id",
        "attempt_id",
        "binding",
        "fence",
        "heartbeat_sequence",
        "lease_token",
      ]);
      assert.equal(body.assignment_id, input.lease.assignment_id);
      assert.equal(body.attempt_id, input.lease.attempt_id);
      assert.equal(body.fence, input.lease.fence);
      assert.equal(body.lease_token, input.lease.lease_token);
      assert.deepEqual(body.binding, input.binding);
      const expectedSequence = input.lease.heartbeat_sequence + paths.length - 1;
      assert.equal(body.heartbeat_sequence, expectedSequence);
      heartbeatBodies.push(bodyText);
      return authoritativeResponse({
        heartbeat_sequence: expectedSequence,
        lease_expires_at_ms: input.lease.lease_expires_at_ms,
        hard_deadline_at_ms: input.lease.hard_deadline_at_ms,
        replayed: false,
      });
    }
    assert.equal(paths.length, 4);
    assert.equal(
      parsed.pathname,
      `/api/jobs/internal/original-source-verifications/${input.lease.assignment_id}/complete`,
    );
    completionBody = bodyText;
    assert.deepEqual(Object.keys(body).sort(), [
      "assignment_id",
      "attempt_id",
      "binding",
      "fence",
      "lease_token",
      "observation",
      "request_id",
    ]);
    assert.equal(body.assignment_id, input.lease.assignment_id);
    assert.equal(body.attempt_id, input.lease.attempt_id);
    assert.equal(body.fence, input.lease.fence);
    assert.equal(body.lease_token, input.lease.lease_token);
    assert.deepEqual(body.binding, input.binding);
    assert.deepEqual(
      {
        assurance: body.observation.assurance,
        result: body.observation.result,
        requested_url: body.observation.requested_url,
        canonical_observed_url: body.observation.canonical_observed_url,
        canonical_application_url: body.observation.canonical_application_url,
        application_domain: body.observation.application_domain,
        retrieval_status: body.observation.retrieval_status,
        http_status: body.observation.http_status,
        parser_version: body.observation.parser_version,
        provider_record_id: body.observation.provider_record_id,
        company: body.observation.company,
        title: body.observation.title,
        location: body.observation.location,
        workplace: body.observation.workplace,
        description: body.observation.description,
        employment_type: body.observation.employment_type,
        posted_at_ms: body.observation.posted_at_ms,
        mismatched_fields: body.observation.mismatched_fields,
        worker_runtime_identity_sha256: body.observation.worker_runtime_identity_sha256,
      },
      {
        assurance: "original_verified",
        result: "open",
        requested_url: EXPECTED_PROVIDER_URL,
        canonical_observed_url: "https://jobs.smartrecruiters.com/acme/smart-1",
        canonical_application_url:
          "https://jobs.smartrecruiters.com/acme/smart-1-platform-engineer?oga=true",
        application_domain: "jobs.smartrecruiters.com",
        retrieval_status: "observed",
        http_status: 200,
        parser_version: "smartrecruiters.original_source.v3",
        provider_record_id:
          "smartrecruiters:jobs.smartrecruiters.com:acme:smart-1",
        company: "Acme",
        title: "Platform Engineer",
        location: "Austin, TX",
        workplace: "hybrid",
        description: "Build reliable systems. SQL & Rust.",
        employment_type: "full_time",
        posted_at_ms: Date.parse("2026-08-25T12:00:00Z"),
        mismatched_fields: [],
        worker_runtime_identity_sha256: input.runtimeIdentitySha256,
      },
    );
    assert.ok(SHA256.test(body.observation.evidence_sha256));
    assert.ok(SHA256.test(body.observation.parser_digest));
    assert.ok(SHA256.test(body.observation.content_digest));
    return authoritativeResponse({
      assignment_id: input.lease.assignment_id,
      state: "idle",
      replayed: false,
      receipt_sha256: "f".repeat(64),
      head: null,
    });
  };

  const api = new apiModule.OriginalSourceVerificationApiClient({
    origin: MOCK_API_ORIGIN,
    signingKey: "synthetic-smartrecruiters-wire-signing-key-v1",
    binding: input.binding,
    fetch: apiFetch,
    reportAttempts: 1,
  });
  const runtime = new runtimeModule.OriginalSourceVerifierRuntime({
    api,
    verifier,
    logger: { log() {} },
    leaseHeartbeatIntervalMs: 30_000,
    workerRuntimeIdentitySha256: input.runtimeIdentitySha256,
  });
  assert.equal(await runtime.pollOnce(), "completed");
  assert.equal(providerCalls, 1);
  assert.deepEqual(paths, [
    "/api/jobs/internal/original-source-verifications/lease",
    `/api/jobs/internal/original-source-verifications/${input.lease.assignment_id}/heartbeat`,
    `/api/jobs/internal/original-source-verifications/${input.lease.assignment_id}/heartbeat`,
    `/api/jobs/internal/original-source-verifications/${input.lease.assignment_id}/complete`,
  ]);
  assert.equal(heartbeatBodies.length, 2);
  assert.equal(typeof completionBody, "string");
  process.stdout.write(`${JSON.stringify({ heartbeatBodies, completionBody })}\n`);
}

const deadline = setTimeout(() => process.exit(70), 9_000);
try {
  await main();
} finally {
  clearTimeout(deadline);
}
