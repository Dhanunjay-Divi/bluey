// Private local-QA bridge to the debug-only durable posting fixture.  This is
// deliberately not an HTTP client, a general PostgreSQL runner, or a seeder
// for non-owned state.
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, realpath } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { spawn } from "node:child_process";
import { LOCAL_QA_SAMPLE_RESUME } from "./local-qa-browser-sample-resume.mjs";

const REQUEST = "durable-qa-posting-request.json";
const RESULT = "durable-qa-posting-result.json";
const STATE_NAME = "bluey-jobs-qa-draft";
const MAX_BINARY_BYTES = 1024 * 1024 * 1024;
const MAX_RESULT_BYTES = 16 * 1024;
const MAX_TIMEOUT_MS = 60_000;
const TERM_GRACE_MS = 5_000;
const ROOT_LEAF = /^bluey-test-runs-[a-f0-9]{12}\/run\.([1-9][0-9]{0,19})\.([1-9][0-9]{0,9})\.[0-9]{3,12}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const ID = /^[A-Za-z0-9._:-]{1,120}$/;
const U64 = /^[1-9][0-9]{0,19}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const FIXTURE_ARG = "--seed-durable-qa-posting";
const C2C_FIXTURE_ARG = "--seed-durable-c2c-qa-posting";
const IMPORTED_RESUME_FIXTURE_ARG = "--seed-durable-imported-resume-qa-posting";
const REVIEW_FIRST_MOCK_RECEIPT_FIXTURE_ARG = "--seed-durable-review-first-mock-receipt-posting";
const REVIEW_FIRST_MOCK_RECEIPT_PUBLISH_ARG = "--publish-durable-review-first-mock-receipt";
const REVIEW_FIRST_MOCK_RECEIPT_REQUEST = "durable-review-first-mock-receipt-request.json";
const REVIEW_FIRST_MOCK_RECEIPT_CONSUMED = "durable-review-first-mock-receipt-request.consumed.json";
const REVIEW_FIRST_MOCK_RECEIPT_RESULT = "durable-review-first-mock-receipt-result.json";
const LOCAL_QA_OBJECT_ENV_KEYS = Object.freeze([
  "BLUEY_OBJECT_ACCESS_KEY_ID",
  "BLUEY_OBJECT_BUCKET",
  "BLUEY_OBJECT_ENDPOINT_URL",
  "BLUEY_OBJECT_KEY_PREFIX",
  "BLUEY_OBJECT_MAX_BYTES",
  "BLUEY_OBJECT_REGION",
  "BLUEY_OBJECT_SECRET_ACCESS_KEY",
]);

function need(value) {
  if (!value) throw new Error("local_qa_durable_posting_seeder_refused");
}

function stamp(stat) {
  return [stat.dev, stat.ino, stat.uid, stat.mode, stat.nlink, stat.size, stat.mtimeNs, stat.ctimeNs]
    .map(String).join(":");
}

function boundedU64(value) {
  return typeof value === "string" && U64.test(value) && BigInt(value) <= 18_446_744_073_709_551_615n;
}

export function isDurableQaFixtureRoot(root, platform = process.platform) {
  const prefix = platform === "darwin" ? "/private/tmp/" : platform === "linux" ? "/tmp/" : null;
  if (prefix === null || typeof root !== "string" || !root.startsWith(prefix)) return false;
  const suffix = root.slice(prefix.length);
  const parsed = ROOT_LEAF.exec(suffix);
  return parsed !== null && parsed[0] === suffix && boundedU64(parsed[1])
    && Number(parsed[2]) > 1 && Number(parsed[2]) <= 2_147_483_647;
}

function fixtureVersion(mode) {
  need(mode === "w2" || mode === "c2c" || mode === "imported_resume" || mode === "review_first_mock_receipt");
  return mode === "w2" ? 1 : mode === "c2c" ? 2 : mode === "imported_resume" ? 3 : 4;
}

export function durableQaPostingRequest({ candidateId, trackId, expectedSystemIdentifier, fixtureMode = "w2",
  sourceResumeAssetId, sourceResumeSha256 }) {
  need(typeof candidateId === "string" && UUID.test(candidateId));
  need(typeof trackId === "string" && UUID.test(trackId));
  need(boundedU64(expectedSystemIdentifier));
  const imported = fixtureMode === "imported_resume" || fixtureMode === "review_first_mock_receipt";
  need(imported ? typeof sourceResumeAssetId === "string" && UUID.test(sourceResumeAssetId)
    && sourceResumeSha256 === LOCAL_QA_SAMPLE_RESUME.sha256
    : sourceResumeAssetId === undefined && sourceResumeSha256 === undefined);
  return Object.freeze({
    version: fixtureVersion(fixtureMode),
    candidate_id: candidateId,
    track_id: trackId,
    expected_system_identifier: expectedSystemIdentifier,
    profile_scope: "candidate@bluey-qa.test",
    job_scope: Object.freeze({
      title: imported ? LOCAL_QA_SAMPLE_RESUME.headline : "Software Engineer",
      location: "New York, NY",
      skills: Object.freeze(imported ? ["Python", "SQL"] : ["Rust", "TypeScript"]),
    }),
    ...(imported ? { source_resume_asset_id: sourceResumeAssetId, source_resume_sha256: sourceResumeSha256 } : {}),
  });
}

export function parseDurableQaPostgresUrl(value) {
  need(typeof value === "string" && value.length > 0 && value.length <= 1024 && !/[\0\r\n]/.test(value));
  let url;
  try { url = new URL(value); } catch { throw new Error("local_qa_durable_posting_seeder_refused"); }
  const port = Number(url.port);
  need(url.protocol === "postgresql:" && url.hostname === "127.0.0.1" && Number.isInteger(port)
    && port > 1024 && port <= 65_535 && port !== 5432 && url.username === "bluey_qa_local"
    && /^[0-9a-f]{64}$/.test(url.password) && url.pathname === "/bluey_jobs_qa"
    && url.search === "?sslmode=disable" && url.hash === "");
  return Object.freeze({ url: value, port });
}

export function parseDurableQaPostingResult(value, fixtureMode = "w2", expectedSource) {
  const imported = fixtureMode === "imported_resume" || fixtureMode === "review_first_mock_receipt";
  need(value && Object.getPrototypeOf(value) === Object.prototype
    && Object.keys(value).sort().join(",") === (imported ? "job_id,source_resume_asset_id,source_resume_sha256,version" : "job_id,version")
    && value.version === fixtureVersion(fixtureMode) && typeof value.job_id === "string" && ID.test(value.job_id));
  need(imported ? typeof expectedSource?.assetId === "string" && UUID.test(expectedSource.assetId)
    && expectedSource.sha256 === LOCAL_QA_SAMPLE_RESUME.sha256
    && value.source_resume_asset_id === expectedSource.assetId && value.source_resume_sha256 === expectedSource.sha256
    : expectedSource === undefined);
  return Object.freeze({ version: value.version, jobId: value.job_id });
}

async function absent(path) {
  try { await lstat(path); return false; }
  catch (error) { if (error?.code === "ENOENT") return true; throw error; }
}

async function privateDirectory(path) {
  need(resolve(path) === path && await realpath(path) === path);
  const stat = await lstat(path, { bigint: true });
  need(stat.isDirectory() && stat.uid === BigInt(process.getuid()) && (stat.mode & 0o7777n) === 0o700n);
  return stamp(stat);
}

async function holdPinnedBinary(path, expectedSha256) {
  need(typeof path === "string" && resolve(path) === path && SHA256.test(expectedSha256)
    && await realpath(path) === path);
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const before = await file.stat({ bigint: true });
    need(before.isFile() && before.uid === BigInt(process.getuid()) && before.nlink === 1n
      && (before.mode & 0o7777n) === 0o700n && before.size > 0n && before.size <= BigInt(MAX_BINARY_BYTES));
    const digest = createHash("sha256"), buffer = Buffer.alloc(65_536); let read = 0;
    for (;;) {
      const result = await file.read(buffer, 0, buffer.length, null);
      if (!result.bytesRead) break;
      read += result.bytesRead; need(read <= MAX_BINARY_BYTES);
      digest.update(buffer.subarray(0, result.bytesRead));
    }
    need(BigInt(read) === before.size && digest.digest("hex") === expectedSha256
      && stamp(await file.stat({ bigint: true })) === stamp(before)
      && stamp(await lstat(path, { bigint: true })) === stamp(before));
    return Object.freeze({ before: stamp(before), close: () => file.close() });
  } catch (error) {
    await file.close();
    throw error;
  }
}

async function holdLauncherMarker(root) {
  const path = join(root, ".bluey-test-run-v2");
  need(await realpath(path) === path);
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const before = await file.stat({ bigint: true });
    need(before.isFile() && before.uid === BigInt(process.getuid()) && before.nlink === 1n
      && (before.mode & 0o022n) === 0n && before.size > 0n && before.size <= 65_536n);
    const bytes = await file.readFile();
    need(BigInt(bytes.length) === before.size && stamp(await file.stat({ bigint: true })) === stamp(before)
      && stamp(await lstat(path, { bigint: true })) === stamp(before));
    return Object.freeze({ path, before: stamp(before), close: () => file.close() });
  } catch (error) {
    await file.close();
    throw error;
  }
}

async function writeExclusivePrivateJson(path, value) {
  const bytes = Buffer.from(`${JSON.stringify(value)}\n`, "utf8");
  const file = await open(path, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
  try { await file.writeFile(bytes); await file.sync(); }
  finally { await file.close(); }
  const stat = await lstat(path, { bigint: true });
  need(stat.isFile() && stat.uid === BigInt(process.getuid()) && stat.nlink === 1n
    && (stat.mode & 0o7777n) === 0o600n && stat.size === BigInt(bytes.length));
}

async function readExclusivePrivateJson(path, fixtureMode, expectedSource) {
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const before = await file.stat({ bigint: true });
    need(before.isFile() && before.uid === BigInt(process.getuid()) && before.nlink === 1n
      && (before.mode & 0o7777n) === 0o600n && before.size > 0n && before.size <= BigInt(MAX_RESULT_BYTES));
    const bytes = await file.readFile();
    need(BigInt(bytes.length) === before.size && stamp(await file.stat({ bigint: true })) === stamp(before)
      && stamp(await lstat(path, { bigint: true })) === stamp(before));
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    const parsed = JSON.parse(text);
    need(`${JSON.stringify(parsed)}\n` === text);
    return parseDurableQaPostingResult(parsed, fixtureMode, expectedSource);
  } finally { await file.close(); }
}

export function durableFixtureDiagnostic(stderr, cancellation) {
  if (["timeout", "aborted", "closed"].includes(cancellation)) return cancellation;
  if (typeof stderr !== "string" || stderr.length > 256) return "unclassified";
  return /^\{"event":"browser_packet_fixture_failed","code":"([a-z][a-z_]{1,63})"\}\n$/.exec(stderr)?.[1]
    ?? "unclassified";
}

function runPinnedFixture(binary, fixtureArg, env, signal) {
  let child, timeout, escalation, settled = false, cancellation = null, abort;
  let stderr = "", diagnosticBytes = 0;
  const stop = reason => {
    if (cancellation) return;
    cancellation = reason;
    if (child && child.exitCode === null && child.signalCode === null) {
      child.kill("SIGTERM");
      escalation = setTimeout(() => {
        if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
      }, TERM_GRACE_MS);
    }
  };
  const promise = new Promise((resolveRun, rejectRun) => {
    try { child = spawn(binary, [fixtureArg], { cwd: env.HOME, env, stdio: ["ignore", "ignore", "pipe"] }); }
    catch (error) { rejectRun(error); return; }
    child.stderr.on("data", bytes => {
      diagnosticBytes += bytes.length;
      if (diagnosticBytes <= 256) stderr += bytes.toString("utf8");
      else stderr = "";
    });
    const finish = value => {
      if (settled) return;
      settled = true; clearTimeout(timeout); clearTimeout(escalation);
      if (signal && abort) signal.removeEventListener("abort", abort);
      resolveRun({ ...value, cancellation, diagnostic: durableFixtureDiagnostic(stderr, cancellation) });
    };
    child.once("error", error => {
      if (settled) return;
      settled = true; clearTimeout(timeout); clearTimeout(escalation);
      if (signal && abort) signal.removeEventListener("abort", abort);
      rejectRun(error);
    });
    child.once("close", (code, childSignal) => finish({ code, signal: childSignal }));
    timeout = setTimeout(() => stop("timeout"), MAX_TIMEOUT_MS);
    if (signal) {
      abort = () => stop("aborted");
      signal.addEventListener("abort", abort, { once: true });
      if (signal.aborted) abort();
    }
  });
  return Object.freeze({ promise, stop });
}

// The caller must already own a fresh canonical launcher root and its prepared
// state. This function intentionally writes exactly one request file and never
// retries; a retained request/result is an operator-visible refusal on rerun.
function validDataKey(value) {
  if (typeof value !== "string" || !/^[A-Za-z0-9+/]{43}=$/.test(value)) return false;
  try { return Buffer.from(value, "base64").length === 32; } catch { return false; }
}

function exact(value, keys) {
  return value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype
    && Object.keys(value).sort().join(",") === keys;
}

function reviewFirstMockReceiptDraft(value) {
  need(exact(value, "accountId,applicationId,jobId,resumeId,sourceAssetId,sourceSha256")
    && UUID.test(value.accountId) && UUID.test(value.applicationId) && UUID.test(value.resumeId)
    && UUID.test(value.sourceAssetId) && typeof value.jobId === "string"
    && /^[A-Za-z0-9_-]{1,160}$/.test(value.jobId)
    && value.sourceSha256 === LOCAL_QA_SAMPLE_RESUME.sha256);
  return Object.freeze({ accountId: value.accountId, applicationId: value.applicationId, jobId: value.jobId,
    resumeId: value.resumeId, sourceAssetId: value.sourceAssetId, sourceSha256: value.sourceSha256 });
}

export function reviewFirstMockReceiptRequest(rawDraft, systemIdentifier) {
  const draft = reviewFirstMockReceiptDraft(rawDraft);
  need(boundedU64(systemIdentifier));
  return Object.freeze({
    version: 1,
    accountId: draft.accountId,
    applicationId: draft.applicationId,
    jobId: draft.jobId,
    resumeId: draft.resumeId,
    sourceAssetId: draft.sourceAssetId,
    sourceSha256: draft.sourceSha256,
    expectedSystemIdentifier: systemIdentifier,
  });
}

export function reviewFirstMockReceiptObjectEnv(value) {
  need(exact(value, LOCAL_QA_OBJECT_ENV_KEYS.join(",")));
  const endpoint = value.BLUEY_OBJECT_ENDPOINT_URL;
  let url;
  try { url = new URL(endpoint); } catch { throw new Error("local_qa_durable_posting_seeder_refused"); }
  need(url.protocol === "http:" && url.hostname === "127.0.0.1" && url.username === ""
    && url.password === "" && url.pathname === "/" && url.search === "" && url.hash === ""
    && endpoint === `http://127.0.0.1:${url.port}`
    && Number.isInteger(Number(url.port)) && Number(url.port) > 1024 && Number(url.port) <= 65_535
    && Number(url.port) !== 5432
    && /^qa-[0-9a-f]{32}$/.test(value.BLUEY_OBJECT_BUCKET)
    && /^qa-[0-9a-f]{32}$/.test(value.BLUEY_OBJECT_KEY_PREFIX)
    && /^qa[0-9a-f]{32}$/.test(value.BLUEY_OBJECT_ACCESS_KEY_ID)
    && SHA256.test(value.BLUEY_OBJECT_SECRET_ACCESS_KEY)
    && value.BLUEY_OBJECT_REGION === "us-east-1" && value.BLUEY_OBJECT_MAX_BYTES === "10485760");
  return Object.freeze(Object.fromEntries(LOCAL_QA_OBJECT_ENV_KEYS.map(key => [key, value[key]])));
}

function parseReviewFirstMockReceiptResult(value, draft) {
  need(exact(value, "accountId,applicationId,jobId,receiptObjectSha256,resumeId,runId,sourceAssetId,sourceSha256,submissionFingerprintSha256")
    && value.accountId === draft.accountId && value.applicationId === draft.applicationId
    && value.jobId === draft.jobId && value.resumeId === draft.resumeId
    && value.sourceAssetId === draft.sourceAssetId && value.sourceSha256 === draft.sourceSha256
    && typeof value.runId === "string" && /^[A-Za-z0-9_-]{1,160}$/.test(value.runId)
    && SHA256.test(value.receiptObjectSha256) && SHA256.test(value.submissionFingerprintSha256));
  return Object.freeze({ accountId: value.accountId, applicationId: value.applicationId, jobId: value.jobId,
    resumeId: value.resumeId, sourceAssetId: value.sourceAssetId, sourceSha256: value.sourceSha256,
    runId: value.runId, receiptObjectSha256: value.receiptObjectSha256,
    submissionFingerprintSha256: value.submissionFingerprintSha256 });
}

async function readReviewFirstMockReceiptResult(path, draft) {
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const before = await file.stat({ bigint: true });
    need(before.isFile() && before.uid === BigInt(process.getuid()) && before.nlink === 1n
      && (before.mode & 0o7777n) === 0o600n && before.size > 0n && before.size <= BigInt(MAX_RESULT_BYTES));
    const bytes = await file.readFile();
    need(BigInt(bytes.length) === before.size && stamp(await file.stat({ bigint: true })) === stamp(before)
      && stamp(await lstat(path, { bigint: true })) === stamp(before));
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    const parsed = JSON.parse(text);
    need(`${JSON.stringify(parsed)}\n` === text);
    return parseReviewFirstMockReceiptResult(parsed, draft);
  } finally { await file.close(); }
}

// One pinned debug fixture publishes exactly one local, review-first mock receipt.
// The caller cannot provide a worker token, browser token, or arbitrary child env.
export async function createLocalQaDurableMockReceiptActor({ root, stateDirectory, artifactPath, artifactSha256 }) {
  const requestPath = join(stateDirectory, REVIEW_FIRST_MOCK_RECEIPT_REQUEST);
  const consumedPath = join(stateDirectory, REVIEW_FIRST_MOCK_RECEIPT_CONSUMED);
  const resultPath = join(stateDirectory, REVIEW_FIRST_MOCK_RECEIPT_RESULT);
  need(isDurableQaFixtureRoot(root));
  need(resolve(stateDirectory) === stateDirectory && stateDirectory === join(root, STATE_NAME));
  await privateDirectory(dirname(root));
  await privateDirectory(root);
  // Hold the same build before account/bootstrap state exists. The
  // operation checks the newly initialized state before creating its request.
  need(await absent(stateDirectory));
  need(typeof artifactPath === "string" && artifactPath.startsWith(`${root}/`)
    && resolve(artifactPath) === artifactPath);
  const marker = await holdLauncherMarker(root);
  let held;
  try { held = await holdPinnedBinary(artifactPath, artifactSha256); }
  catch (error) { await marker.close(); throw error; }
  let consumed = false, closed = false, active;
  const close = async () => {
    if (closed) return;
    closed = true;
    if (active) {
      active.stop("closed");
      try { await active.promise; } catch { /* closure is the refusal boundary */ }
    }
    try { await held.close(); } finally { await marker.close(); }
  };
  const operation = async (rawDraft, { systemIdentifier, databaseUrl, dataKey, objectEnv, signal } = {}) => {
    need(!closed && !consumed && (signal === undefined || signal instanceof AbortSignal) && !signal?.aborted);
    consumed = true;
    const draft = reviewFirstMockReceiptDraft(rawDraft);
    const postgres = parseDurableQaPostgresUrl(databaseUrl);
    need(validDataKey(dataKey));
    const objectStore = reviewFirstMockReceiptObjectEnv(objectEnv);
    const request = reviewFirstMockReceiptRequest(draft, systemIdentifier);
    await privateDirectory(dirname(root)); await privateDirectory(root); await privateDirectory(stateDirectory);
    need(stamp(await lstat(marker.path, { bigint: true })) === marker.before
      && stamp(await lstat(artifactPath, { bigint: true })) === held.before);
    need(await absent(requestPath) && await absent(consumedPath) && await absent(resultPath));
    need(!closed && !signal?.aborted);
    await writeExclusivePrivateJson(requestPath, request);
    const env = Object.freeze({
      PATH: "/usr/bin:/bin", LANG: "C", LC_ALL: "C", TZ: "UTC",
      HOME: stateDirectory, TMPDIR: join(root, "tmp"), TMP: join(root, "tmp"),
      TEMP: join(root, "tmp"), SQLITE_TMPDIR: join(root, "tmp"),
      BLUEY_TEST_RUN_ROOT: root, BLUEY_TEST_POSTGRES_URL: postgres.url, BLUEY_JOBS_DATA_KEY: dataKey,
      ...objectStore,
    });
    need(stamp(await lstat(marker.path, { bigint: true })) === marker.before
      && stamp(await lstat(artifactPath, { bigint: true })) === held.before && !closed && !signal?.aborted);
    active = runPinnedFixture(artifactPath, REVIEW_FIRST_MOCK_RECEIPT_PUBLISH_ARG, env, signal);
    let terminal;
    try { terminal = await active.promise; } finally { active = undefined; }
    if (terminal.cancellation !== null || terminal.code !== 0 || terminal.signal !== null) {
      throw new Error(`local_qa_durable_fixture_${terminal.diagnostic}`);
    }
    need(!closed && !signal?.aborted && terminal.cancellation === null
      && terminal.code === 0 && terminal.signal === null);
    const result = await readReviewFirstMockReceiptResult(resultPath, draft);
    need(!closed && !signal?.aborted && stamp(await lstat(artifactPath, { bigint: true })) === held.before
      && stamp(await lstat(marker.path, { bigint: true })) === marker.before);
    return result;
  };
  return Object.freeze({ operation, close, requestPath, resultPath });
}

// Preflight is deliberately separate from operation: the caller can hold the
// exact checked executable across normal account/onboarding work, but this
// module cannot write a request until the caller supplies that durable state.
export async function createLocalQaDurablePostingSeeder({
  root, stateDirectory, artifactPath, artifactSha256, fixtureMode = "w2",
}) {
  const version = fixtureVersion(fixtureMode);
  const fixtureArg = version === 1 ? FIXTURE_ARG : version === 2 ? C2C_FIXTURE_ARG
    : version === 3 ? IMPORTED_RESUME_FIXTURE_ARG : REVIEW_FIRST_MOCK_RECEIPT_FIXTURE_ARG;
  const prefix = version === 3 ? "durable-imported-resume-qa-posting"
    : version === 4 ? "durable-review-first-mock-receipt-posting" : "durable-qa-posting";
  const requestPath = join(stateDirectory, `${prefix}-request.json`);
  const resultPath = join(stateDirectory, `${prefix}-result.json`);
  const consumedPath = join(stateDirectory, `${prefix}-request.consumed.json`);
  need(isDurableQaFixtureRoot(root));
  need(resolve(stateDirectory) === stateDirectory && stateDirectory === join(root, STATE_NAME));
  await privateDirectory(dirname(root));
  await privateDirectory(root);
  // Session bootstrap creates this directory only after this preflight has
  // established binary/launcher custody. A preexisting state is never adopted.
  need(await absent(stateDirectory));
  need(typeof artifactPath === "string" && artifactPath.startsWith(`${root}/`)
    && resolve(artifactPath) === artifactPath);
  const marker = await holdLauncherMarker(root);
  let held;
  try { held = await holdPinnedBinary(artifactPath, artifactSha256); }
  catch (error) { await marker.close(); throw error; }
  let consumed = false, closed = false, active;
  const close = async () => {
    if (closed) return;
    closed = true;
    if (active) {
      active.stop("closed");
      try { await active.promise; } catch { /* closure itself is the refusal boundary */ }
    }
    try { await held.close(); } finally { await marker.close(); }
  };
  const operation = async ({ accountId, trackId, systemIdentifier, databaseUrl, dataKey,
    sourceResumeAssetId, sourceResumeSha256 }, { signal } = {}) => {
    need(!closed && !consumed && (signal === undefined || signal instanceof AbortSignal) && !signal?.aborted);
    consumed = true;
    const postgres = parseDurableQaPostgresUrl(databaseUrl);
    need(validDataKey(dataKey));
    const request = durableQaPostingRequest({
      candidateId: accountId, trackId, expectedSystemIdentifier: systemIdentifier, fixtureMode,
      sourceResumeAssetId, sourceResumeSha256,
    });
    // Recheck the unmutated state just before the first exclusive create.
    await privateDirectory(dirname(root)); await privateDirectory(root); await privateDirectory(stateDirectory);
    need(stamp(await lstat(marker.path, { bigint: true })) === marker.before
      && stamp(await lstat(artifactPath, { bigint: true })) === held.before);
    need(await absent(requestPath) && await absent(resultPath) && await absent(consumedPath));
    need(!closed && !signal?.aborted);
    await writeExclusivePrivateJson(requestPath, request);
    const env = Object.freeze({
      PATH: "/usr/bin:/bin", LANG: "C", LC_ALL: "C", TZ: "UTC",
      HOME: stateDirectory, TMPDIR: join(root, "tmp"), TMP: join(root, "tmp"),
      TEMP: join(root, "tmp"), SQLITE_TMPDIR: join(root, "tmp"),
      BLUEY_TEST_RUN_ROOT: root, BLUEY_TEST_POSTGRES_URL: postgres.url, BLUEY_JOBS_DATA_KEY: dataKey,
    });
    need(stamp(await lstat(marker.path, { bigint: true })) === marker.before
      && stamp(await lstat(artifactPath, { bigint: true })) === held.before);
    need(!closed && !signal?.aborted);
    active = runPinnedFixture(artifactPath, fixtureArg, env, signal);
    let terminal;
    try { terminal = await active.promise; } finally { active = undefined; }
    if (terminal.cancellation !== null || terminal.code !== 0 || terminal.signal !== null) {
      throw new Error(`local_qa_durable_fixture_${terminal.diagnostic}`);
    }
    need(!closed && !signal?.aborted && terminal.cancellation === null
      && terminal.code === 0 && terminal.signal === null);
    const expectedSource = version === 3 || version === 4
      ? { assetId: sourceResumeAssetId, sha256: sourceResumeSha256 } : undefined;
    const result = await readExclusivePrivateJson(resultPath, fixtureMode, expectedSource);
    need(!closed && !signal?.aborted && stamp(await lstat(artifactPath, { bigint: true })) === held.before
      && stamp(await lstat(marker.path, { bigint: true })) === marker.before);
    need(!closed && !signal?.aborted);
    return result.jobId;
  };
  return Object.freeze({
    operation,
    close,
    requestPath,
    resultPath,
  });
}

export const DURABLE_QA_POSTING_SEEDER = Object.freeze({
  binaryArgument: FIXTURE_ARG,
  requestFile: REQUEST,
  resultFile: RESULT,
  stateChild: STATE_NAME,
});
