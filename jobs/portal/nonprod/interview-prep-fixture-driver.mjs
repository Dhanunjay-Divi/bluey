#!/usr/bin/env node
/*
 * Linux-only, staged interview-prep browser fixture driver.  The enclosing
 * controller is responsible for the qualified-container fence; this program
 * refuses to run without its explicit, fixed attestation.
 */
import { createHash } from "node:crypto";
import {
  lstatSync,
  readFileSync,
  realpathSync,
} from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const INPUT_LIMIT = 16 * 1024;
const SCHEMA = "jobs-interview-prep-browser-v1";
const RESULT_SCHEMA = "jobs-interview-prep-browser-result-v2";
const OUTER_FENCE = "qualified-linux-interview-prep-fixture-v1";
const RECEIPT_SHA256 = "6627199b42c4e921630a8b020a1db330d69c062b8408535a6721426a47171d54";
const SHA256 = /^[0-9a-f]{64}$/;
const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const TOKEN = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,255}$/;
const JWT = /^[A-Za-z0-9_-]{1,2048}\.[A-Za-z0-9_-]{1,2048}\.[A-Za-z0-9_-]{1,2048}$/;
const here = realpathSync(dirname(fileURLToPath(import.meta.url)));

function fail() {
  throw new Error("interview_prep_fixture_failed");
}

function isPlainRecord(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function exactKeys(value, keys) {
  return isPlainRecord(value)
    && Object.keys(value).length === keys.length
    && keys.every((key) => Object.prototype.hasOwnProperty.call(value, key));
}

function boundedString(value, matcher = /^.{1,4096}$/su) {
  return typeof value === "string" && matcher.test(value) && !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u.test(value);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function readFrame() {
  const chunks = [];
  let size = 0;
  for await (const chunk of process.stdin) {
    size += chunk.length;
    if (size > INPUT_LIMIT) fail();
    chunks.push(chunk);
  }
  if (size === 0) fail();
  let value;
  try {
    value = JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    fail();
  }
  return value;
}

function validateExpected(value) {
  const keys = [
    "applicationId", "receiptId", "resumeVersionId", "resumeVersionNo",
    "resumeChecksum", "resumeDocumentSha256", "receiptFingerprint", "company",
    "jobTitle", "provider", "model", "content",
  ];
  if (!exactKeys(value, keys)) fail();
  for (const key of ["applicationId", "receiptId", "resumeVersionId"]) {
    if (!boundedString(value[key], TOKEN)) fail();
  }
  for (const key of ["resumeChecksum", "company", "jobTitle", "provider", "model", "content"]) {
    if (!boundedString(value[key])) fail();
  }
  if (!Number.isSafeInteger(value.resumeVersionNo) || value.resumeVersionNo < 1) fail();
  if (!SHA256.test(value.resumeDocumentSha256) || !SHA256.test(value.receiptFingerprint)) fail();
  return Object.freeze({ ...value });
}

function validateFrame(value) {
  const keys = ["schema", "nonce", "jobsPort", "accountId", "accessToken", "balanceCents", "expected"];
  if (!exactKeys(value, keys) || value.schema !== SCHEMA) fail();
  if (!boundedString(value.nonce, /^[A-Za-z0-9_-]{16,128}$/)) fail();
  if (!Number.isSafeInteger(value.jobsPort) || value.jobsPort < 1024 || value.jobsPort > 65535) fail();
  if (!boundedString(value.accountId, /^ui_fixture_[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)) fail();
  if (!boundedString(value.accessToken, JWT)) fail();
  if (!Number.isSafeInteger(value.balanceCents) || value.balanceCents < 0) fail();
  return Object.freeze({ ...value, expected: validateExpected(value.expected) });
}

function stagedFile(parts, maximumBytes = 32 * 1024 * 1024) {
  const candidate = resolve(here, ...parts);
  if (relative(here, candidate).startsWith("..")) fail();
  let cursor = here;
  for (const part of parts.slice(0, -1)) {
    cursor = join(cursor, part);
    const directory = lstatSync(cursor);
    if (!directory.isDirectory() || directory.isSymbolicLink() || directory.uid !== process.getuid()) fail();
  }
  const stat = lstatSync(candidate);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.uid !== process.getuid()
    || stat.nlink !== 1 || (stat.mode & 0o022) !== 0 || stat.size > maximumBytes) fail();
  return candidate;
}

function loadAssets() {
  const portal = resolve(here, "portal");
  const portalStat = lstatSync(portal);
  if (relative(here, portal).startsWith("..") || !portalStat.isDirectory() || portalStat.isSymbolicLink()
    || portalStat.uid !== process.getuid()) fail();
  const manifestFile = stagedFile(["portal", "portal-manifest.json"]);
  const receiptFile = stagedFile(["receipt.json"]);
  let manifest;
  let receipt;
  try {
    manifest = JSON.parse(readFileSync(manifestFile, "utf8"));
    const receiptBytes = readFileSync(receiptFile);
    if (sha256(receiptBytes) !== RECEIPT_SHA256) fail();
    receipt = JSON.parse(receiptBytes.toString("utf8"));
  } catch {
    fail();
  }
  if (!Array.isArray(manifest) || manifest.length !== 36 || !isPlainRecord(receipt) || !Array.isArray(receipt.bundle)) fail();
  // The receipt's canonical bundle is the only accepted asset-list authority.
  const expectedManifestSha256 = sha256(Buffer.from(JSON.stringify(receipt.bundle)));
  const actualManifestSha256 = sha256(Buffer.from(JSON.stringify(manifest)));
  if (actualManifestSha256 !== expectedManifestSha256) fail();
  const assets = new Map();
  const paths = new Set();
  for (const row of manifest) {
    if (!Array.isArray(row) || row.length !== 2 || !boundedString(row[0], /^[A-Za-z0-9][A-Za-z0-9._/-]{0,255}$/) || row[0].includes("..") || !SHA256.test(row[1]) || paths.has(row[0])) fail();
    paths.add(row[0]);
    const file = stagedFile(["portal", ...row[0].split("/")]);
    const bytes = readFileSync(file);
    if (sha256(bytes) !== row[1]) fail();
    assets.set(`/jobs/${row[0]}`, bytes);
  }
  if (!assets.has("/jobs/index.html") || assets.size !== 36) fail();
  return assets;
}

function closeQuietly(value) {
  return value?.close?.();
}

async function run(frame) {
  if (process.env.BLUEY_INTERVIEW_PREP_OUTER_FENCE !== OUTER_FENCE) fail();
  if (process.argv.length !== 2) fail();
  const gatewayModule = await import(pathToFileURL(stagedFile(["interview-prep-fixture-gateway.mjs"])).href);
  const witnessModule = await import(pathToFileURL(stagedFile(["interview-prep-witness.mjs"])).href);
  const playwrightModule = await import(pathToFileURL(stagedFile(["node_modules", "playwright-core", "index.mjs"])).href);
  const chrome = stagedFile(["chromium", "chrome"], 384 * 1024 * 1024);
  const assets = loadAssets();
  if (typeof gatewayModule.startInterviewPrepFixtureGateway !== "function" || typeof witnessModule.observeReceiptInterviewPrepInPortal !== "function" || !playwrightModule.chromium) fail();

  let gateway;
  let browser;
  let context;
  let page;
  let completed = false;
  let prepRequestSha256;
  let cleanupFailure;
  try {
    gateway = await gatewayModule.startInterviewPrepFixtureGateway({
      jobsPort: frame.jobsPort,
      assetBytes: assets,
      accountId: frame.accountId,
      accessToken: frame.accessToken,
      balanceCents: frame.balanceCents,
      applicationId: frame.expected.applicationId,
      resumeVersionId: frame.expected.resumeVersionId,
      receiptFingerprint: frame.expected.receiptFingerprint,
    });
    if (!isPlainRecord(gateway) || !boundedString(gateway.origin, /^http:\/\/127\.0\.0\.1:\d{1,5}$/)) fail();
    browser = await playwrightModule.chromium.launch({ executablePath: chrome, headless: true, chromiumSandbox: true });
    context = await browser.newContext({ acceptDownloads: true, serviceWorkers: "block" });
    await context.route("**/*", (route) => {
      try {
        if (new URL(route.request().url()).origin !== gateway.origin) return route.abort("blockedbyclient");
        return route.continue();
      } catch {
        return route.abort("blockedbyclient");
      }
    });
    await context.routeWebSocket("**/*", (socket) => socket.close());
    await context.addInitScript(({ origin, token }) => {
      if (globalThis.location?.origin === origin) {
        localStorage.setItem("bluey_access_token", token);
        localStorage.setItem("bluey_auth_persistence", "local");
      }
    }, { origin: gateway.origin, token: frame.accessToken });
    page = await context.newPage();
    page.on("popup", (popup) => { void popup.close(); });
    const observed = await witnessModule.observeReceiptInterviewPrepInPortal({ page, portalOrigin: gateway.origin, expected: frame.expected });
    const census = gateway.census();
    if (!isPlainRecord(observed)
      || observed.committedWorkspaceReceiptObserved !== true
      || observed.loginFormObserved !== false
      || observed.exactGenerateRequests !== 1
      || observed.structuredDraftDownloads !== 2
      || observed.textDraftDownloads !== 2
      || observed.scoped !== true
      || !isPlainRecord(census)
      || census.prepForwardAttempts !== 1 || !SHA256.test(census.prepRequestSha256)) fail();
    prepRequestSha256 = census.prepRequestSha256;
    completed = true;
  } finally {
    for (const value of [context, browser, gateway]) {
      try {
        await closeQuietly(value);
      } catch (error) {
        cleanupFailure ??= error;
      }
    }
  }
  if (!completed || cleanupFailure) fail();
  return Object.freeze({
    schema: RESULT_SCHEMA,
    nonce: frame.nonce,
    committedWorkspaceReceiptObserved: true,
    loginFormObserved: false,
    exactGenerateRequests: 1,
    structuredDraftDownloads: 2,
    textDraftDownloads: 2,
    prepForwardAttempts: 1,
    prepRequestSha256,
  });
}

try {
  const frame = validateFrame(await readFrame());
  const result = await run(frame);
  process.stdout.write(`${JSON.stringify(result)}\n`);
} catch {
  // The input can contain bearer material and draft text: never serialize errors.
  process.stderr.write("interview_prep_fixture_failed\n");
  process.exitCode = 1;
}
