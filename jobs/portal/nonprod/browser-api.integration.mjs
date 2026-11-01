/*
 * Private non-production qualification only.  The caller supplies a freshly
 * created PostgreSQL cluster and an owned root; this script never falls back
 * to a developer database, browser profile, or external endpoint.
 */
import assert from "node:assert/strict";
import { createHash, createHmac, generateKeyPairSync, randomBytes, randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { createServer } from "node:http";
import { request as httpRequest } from "node:http";
import { lstat, mkdir, readFile, realpath, stat, writeFile, chmod } from "node:fs/promises";
import { extname, join, normalize, relative, resolve } from "node:path";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { chromium } from "playwright";
import { startNonprodObjectStore } from "../../scripts/nonprod-object-store.mjs";

const HUMAN_HOLD = process.argv.length === 3 && process.argv[2] === "--human-hold";
assert(process.argv.length === 2 || HUMAN_HOLD,
  "expected no arguments or exact --human-hold");
const HUMAN_HOLD_MS = 600_000;
const HUMAN_CANARY_URL = "http://bluey-human-hold-canary.invalid/probe";
const LIMIT_MS = HUMAN_HOLD ? 720_000 : 180_000;
const OPERATION_MS = 10_000;
const children = [];
const transientChildren = new Set();
const lifecycle = new AbortController();
const EXPECTED_CASES = ["profile_reload", "onboarding_complete", "preference_track_reload", "auth_isolation", "effect_availability", "packet_review", "approval_fenced", "source_resume_readback", "review_first_queue"];
const result = { version: 4, cases: [], tests: EXPECTED_CASES.length, passed: 0, failed: [], binaries: [], fixture: null, object_store: null, requests: { jobs: 0, account: 0 } };
const HUMAN_CASES = ["bootstrap", "ready", "operator_release_received",
  "prepared_before_reload", "prepared_after_reload", "account_isolation",
  "zero_effects", "cleanup"];
const humanResult = { version: 1, mode: "human_hold", cases: [], passed: 0,
  ready: false, operator_release_received: false, hold_ms: 0,
  account_status_before: 0, account_status_after: 0, unauthenticated_status: 0,
  account_rows: 0, profile_rows: 0, application_rows: 0, prepared_rows: 0,
  submitted_rows: 0, browser_sessions: 0, workflow_commands: 0, run_events: 0,
  irreversible_receipts: 0, profile_stable: false, application_stable: false,
  resume_stable: false, isolated_application_rows: 0, network_canary_blocked: false,
  page_errors: 0, server_errors: 0, engine_ps: -1, engine_lsof: -1, ports_refused: 0,
  binaries: [], fixture: null,
  requests: { jobs: 0, account: 0, canary_blocked: 0, denied_non_origin: 0,
    denied_popups: 0, denied_downloads: 0, denied_websockets: 0 }, object_store: null };
let outputRoot;
let liveBrowser;
let liveProxy;
let stage = "preflight";
let objectStore;
let ownedBrowserRoot;
let browserClosePromise;
let proxyClosePromise;
let childrenStopPromise;
let transientsStopPromise;
let abortCode = "";
let publishedReportPath;
const cleanupPorts = [];

function fail(code) { throw new Error(code); }
function checkAbort() {
  if (lifecycle.signal.aborted) throw new Error(abortCode || "operation_aborted");
}
function closeLiveBrowser() {
  if (!browserClosePromise && liveBrowser) {
    const owned = liveBrowser;
    browserClosePromise = owned.close().finally(() => {
      if (liveBrowser === owned) liveBrowser = undefined;
    });
  }
  return browserClosePromise || Promise.resolve();
}
function closeLiveProxy() {
  if (!proxyClosePromise && liveProxy) {
    const owned = liveProxy;
    proxyClosePromise = new Promise((resolveClose, reject) => owned.close((error) => {
      if (error && error.code !== "ERR_SERVER_NOT_RUNNING") reject(error); else resolveClose();
    })).finally(() => { if (liveProxy === owned) liveProxy = undefined; });
  }
  return proxyClosePromise || Promise.resolve();
}
async function closeInteractive() {
  const outcomes = await Promise.allSettled([
    withTimeout(closeLiveBrowser(), "cleanup_browser", true),
    withTimeout(closeLiveProxy(), "cleanup_proxy", true),
  ]);
  if (outcomes.some((outcome) => outcome.status === "rejected")) {
    fail("cleanup_interactive_failure");
  }
}
function requestAbort(code) {
  if (lifecycle.signal.aborted) return;
  abortCode = code; result.failed.push(code); process.exitCode = 1;
  lifecycle.abort(new Error(code));
  if (publishedReportPath) {
    try { unlinkSync(publishedReportPath); } catch (error) {
      if (error.code !== "ENOENT") result.failed.push("report_revoke_failure");
    }
  }
  void closeLiveBrowser().catch(() => {});
}
async function withAbort(promise) {
  checkAbort();
  let onAbort;
  const aborted = new Promise((_, reject) => {
    onAbort = () => reject(lifecycle.signal.reason || new Error("operation_aborted"));
    lifecycle.signal.addEventListener("abort", onAbort, { once: true });
  });
  try { return await Promise.race([promise, aborted]); }
  finally { lifecycle.signal.removeEventListener("abort", onAbort); }
}
function passCase(name) {
  checkAbort();
  assert.equal(EXPECTED_CASES[result.cases.length], name, "case_order");
  result.cases.push(name); result.passed += 1;
  process.stdout.write(`${JSON.stringify({ event: "browser_case_passed", case: name })}\n`);
}
function passHumanCase(name) {
  checkAbort();
  assert.equal(HUMAN_CASES[humanResult.cases.length], name, "human_case_order");
  humanResult.cases.push(name); humanResult.passed += 1;
  process.stdout.write(`${JSON.stringify({ event: "human_browser_case_passed", case: name })}\n`);
}
function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") return `{${Object.keys(value).sort()
    .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
  return JSON.stringify(value);
}
function canonicalSha256(value) {
  return createHash("sha256").update(canonicalJson(value)).digest("hex");
}
function mountHumanBanner(expectedOrigin) {
  if (location.origin !== expectedOrigin || sessionStorage.getItem("bluey_human_finish_released")) return;
  const mount = () => {
    if (!document.body || document.getElementById("bluey-human-hold")) return;
    const banner = document.createElement("aside");
    banner.id = "bluey-human-hold"; banner.setAttribute("role", "region");
    banner.setAttribute("aria-label", "Non-production test control");
    banner.style.cssText = "position:fixed;z-index:2147483647;left:12px;bottom:12px;width:22rem;max-width:calc(100vw - 24px);box-sizing:border-box;padding:12px;background:#111827;color:white;border:2px solid #38bdf8;display:grid;gap:12px";
    const label = document.createElement("strong");
    label.textContent = "NON-PRODUCTION SYNTHETIC TEST — prepared Review First only. File imports and downloads are disabled; use no real data.";
    const button = document.createElement("button");
    button.type = "button"; button.textContent = "Finish synthetic test";
    button.style.cssText = "min-height:44px;width:100%;padding:8px 12px;background:white;color:#111827;border:2px solid #e5e7eb;outline:3px solid transparent;outline-offset:2px";
    button.addEventListener("focus", () => { button.style.outlineColor = "#38bdf8"; });
    button.addEventListener("blur", () => { button.style.outlineColor = "transparent"; });
    button.addEventListener("click", async () => {
      button.disabled = true; await window.__blueyHumanFinish();
      sessionStorage.setItem("bluey_human_finish_released", "1"); banner.remove();
    });
    banner.append(label, button); document.body.append(banner);
  };
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", mount, { once: true });
  } else mount();
}
function within(root, target) { const value = relative(root, target); return value === "" || (!value.startsWith("..") && !resolve(root, target).startsWith("..")); }
function required(name) { const value = process.env[name]; if (!value) fail(`missing_${name.toLowerCase()}`); return value; }
function onlyLoopbackPostgres(value) {
  const url = new URL(value);
  if (url.protocol !== "postgres:" && url.protocol !== "postgresql:") fail("postgres_url_scheme");
  if (!["127.0.0.1", "localhost", "::1"].includes(url.hostname)) fail("postgres_url_not_loopback");
  return url;
}
async function ownedPath(root, value, name) {
  const info = await lstat(value).catch(() => fail(`missing_${name}`));
  if (info.isSymbolicLink()) fail(`symlink_${name}`);
  const path = await realpath(value);
  if (!within(root, path) || info.uid !== process.getuid() || (info.mode & 0o077) !== 0) fail(`unowned_${name}`);
  return path;
}
async function withTimeout(promise, label, ignoreAbort = false) {
  let timer;
  const bounded = Promise.race([promise, new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`timeout_${label}`)), OPERATION_MS);
  })]);
  try { return await (ignoreAbort ? bounded : withAbort(bounded)); }
  finally { clearTimeout(timer); }
}
function fixtureToken(accountId, secret) {
  const encode = (value) => Buffer.from(JSON.stringify(value)).toString("base64url");
  const now = Math.floor(Date.now() / 1000);
  const body = `${encode({ alg: "HS256", typ: "JWT" })}.${encode({ sub: accountId, iat: now, exp: now + 900, kind: "access" })}`;
  return `${body}.${createHmac("sha256", secret).update(body).digest("base64url")}`;
}
function fixturePurgeSigningPolicy() {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  const privateDer = privateKey.export({ format: "der", type: "pkcs8" });
  const publicDer = publicKey.export({ format: "der", type: "spki" });
  const privatePrefix = Buffer.from("302e020100300506032b657004220420", "hex");
  const publicPrefix = Buffer.from("302a300506032b6570032100", "hex");
  assert.equal(privateDer.length, privatePrefix.length + 32, "fixture_ed25519_private_length");
  assert.equal(publicDer.length, publicPrefix.length + 32, "fixture_ed25519_public_length");
  assert.ok(privateDer.subarray(0, privatePrefix.length).equals(privatePrefix), "fixture_ed25519_private_prefix");
  assert.ok(publicDer.subarray(0, publicPrefix.length).equals(publicPrefix), "fixture_ed25519_public_prefix");
  const keyId = `fixture-purge-${randomUUID()}`;
  return { keyId, seed: privateDer.subarray(privatePrefix.length).toString("base64url"), publicKey: publicDer.subarray(publicPrefix.length).toString("base64url") };
}
function child(command, args, env, name, fixture = false) {
  checkAbort();
  const processHandle = spawn(command, args, { env, stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  processHandle.stderr.on("data", (chunk) => { stderr = `${stderr}${chunk}`.slice(-1024); });
  const completion = new Promise((resolveChild) => {
    processHandle.once("error", () => resolveChild({ code: null, signal: "spawn_error" }));
    processHandle.once("close", (code, signal) => resolveChild({ code, signal }));
  });
  const fixtureFailure = () => {
    try {
      const value = JSON.parse(stderr.trim().split("\n").at(-1));
      if (value.event === "browser_packet_fixture_failed" && /^[a-z_]{1,64}$/.test(value.code)) return value.code;
    } catch {}
    return "unclassified";
  };
  children.push({ name, fixture, processHandle, completion, fixtureFailure, stderr: () => stderr.replace(/[^a-z0-9_:-]/gi, "_").slice(-120) });
  return processHandle;
}
async function stopChildren() {
  if (childrenStopPromise) return childrenStopPromise;
  childrenStopPromise = (async () => {
    const active = children.filter(({ processHandle }) => processHandle.exitCode === null
      && processHandle.signalCode === null);
    for (const { processHandle } of active) processHandle.kill("SIGTERM");
    const outcomes = await Promise.allSettled(children.map(async ({ name, fixture,
      processHandle, completion, stderr }) => {
      let outcome;
      try { outcome = await withTimeout(completion, `stop_${name}`, true); }
      catch {
        if (processHandle.exitCode === null && processHandle.signalCode === null) {
          processHandle.kill("SIGKILL");
        }
        outcome = await withTimeout(completion, `kill_${name}`, true);
      }
      const { code, signal } = outcome;
      if (fixture) result.fixture = { pid: processHandle.pid, code, signal };
      else result.binaries.push({ pid: processHandle.pid, code, signal });
      if (!lifecycle.signal.aborted && (code !== 0 || signal !== null)) {
        fail(`${name}_exit_${stderr() || "unexpected"}`);
      }
    }));
    const rejected = outcomes.find((outcome) => outcome.status === "rejected");
    if (rejected) fail("cleanup_child_failure");
    return outcomes;
  })();
  return childrenStopPromise;
}
function trackTransient(processHandle) {
  transientChildren.add(processHandle);
  processHandle.once("close", () => transientChildren.delete(processHandle));
  return processHandle;
}
async function stopTransientChildren() {
  if (transientsStopPromise) return transientsStopPromise;
  transientsStopPromise = (async () => {
    const outcomes = await Promise.allSettled([...transientChildren].map(async (processHandle) => {
      const completion = once(processHandle, "close");
      if (processHandle.exitCode === null && processHandle.signalCode === null) {
        processHandle.kill("SIGTERM");
      }
      try { await withTimeout(completion, "stop_transient", true); }
      catch {
        if (processHandle.exitCode === null && processHandle.signalCode === null) {
          processHandle.kill("SIGKILL");
        }
        await withTimeout(completion, "kill_transient", true);
      }
    }));
    if (outcomes.some((outcome) => outcome.status === "rejected")) {
      fail("cleanup_transient_failure");
    }
    return outcomes;
  })();
  return transientsStopPromise;
}
function listen(server) { return new Promise((resolveListen) => server.listen(0, "127.0.0.1", () => resolveListen(server.address().port))); }
async function freePort() {
  const reservation = createServer();
  const port = await listen(reservation);
  await new Promise((resolveClose) => reservation.close(resolveClose));
  return port;
}
function get(url, token) {
  return new Promise((resolveRequest, reject) => {
    let settled = false;
    const finish = (callback, value) => { if (!settled) { settled = true; clearTimeout(timer); callback(value); } };
    const request = httpRequest(url, { headers: token ? { authorization: `Bearer ${token}` } : {} }, (response) => {
      response.resume(); response.on("end", () => finish(resolveRequest, response.statusCode));
    });
    const timer = setTimeout(() => { request.destroy(); finish(reject, new Error("http_timeout")); }, OPERATION_MS);
    request.on("error", (error) => finish(reject, error)); request.end();
  });
}
function getJson(url, token) {
  return new Promise((resolveRequest, reject) => {
    let settled = false;
    const finish = (callback, value) => { if (!settled) { settled = true; clearTimeout(timer); callback(value); } };
    const request = httpRequest(url, { headers: { authorization: `Bearer ${token}` } }, (response) => {
      let body = ""; response.on("data", (chunk) => { body += chunk; if (body.length > 1024 * 1024) { request.destroy(); finish(reject, new Error("workspace_body_limit")); } });
      response.on("end", () => {
        if (response.statusCode !== 200) return finish(reject, new Error("workspace_status"));
        try { finish(resolveRequest, JSON.parse(body)); } catch { finish(reject, new Error("workspace_json")); }
      });
    });
    const timer = setTimeout(() => { request.destroy(); finish(reject, new Error("http_timeout")); }, OPERATION_MS);
    request.on("error", (error) => finish(reject, error)); request.end();
  });
}
function putJson(url, token, body) {
  return new Promise((resolveRequest, reject) => {
    const encoded = JSON.stringify(body); let settled = false;
    const finish = (callback, value) => { if (!settled) { settled = true; clearTimeout(timer); callback(value); } };
    const request = httpRequest(url, { method: "PUT", headers: { authorization: `Bearer ${token}`, "content-type": "application/json", "content-length": Buffer.byteLength(encoded) } }, (response) => {
      response.resume(); response.on("end", () => finish(resolveRequest, response.statusCode));
    });
    const timer = setTimeout(() => { request.destroy(); finish(reject, new Error("http_timeout")); }, OPERATION_MS);
    request.on("error", (error) => finish(reject, error)); request.end(encoded);
  });
}
function postJson(url, token, body) {
  return new Promise((resolveRequest, reject) => {
    const encoded = JSON.stringify(body); let settled = false;
    const finish = (callback, value) => { if (!settled) { settled = true; clearTimeout(timer); callback(value); } };
    const request = httpRequest(url, { method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json", "content-length": Buffer.byteLength(encoded) } }, (response) => {
      let responseBody = ""; response.on("data", (chunk) => { responseBody += chunk; if (responseBody.length > 1024 * 1024) { request.destroy(); finish(reject, new Error("post_body_limit")); } });
      response.on("end", () => { let parsed = responseBody; try { parsed = responseBody ? JSON.parse(responseBody) : null; } catch {} finish(resolveRequest, { status: response.statusCode, body: parsed }); });
    });
    const timer = setTimeout(() => { request.destroy(); finish(reject, new Error("http_timeout")); }, OPERATION_MS);
    request.on("error", (error) => finish(reject, error)); request.end(encoded);
  });
}
async function ownedJson(path, root, expected) {
  const info = await lstat(path); if (!info.isFile() || info.isSymbolicLink() || info.uid !== process.getuid() || (info.mode & 0o777) !== 0o600 || info.size > 4096 || !within(root, path)) fail("fixture_file_identity");
  const value = JSON.parse(await readFile(path, "utf8")); assert.deepEqual(Object.keys(value).sort(), Object.keys(expected).sort(), "fixture_schema");
  for (const [key, type] of Object.entries(expected)) assert.equal(typeof value[key], type, "fixture_type");
  return value;
}
async function waitFor(url, token) {
  const deadline = Date.now() + OPERATION_MS;
  while (Date.now() < deadline) {
    checkAbort();
    const stopped = children.find(({ processHandle }) => processHandle.exitCode !== null || processHandle.signalCode !== null);
    if (stopped) {
      const marker = stopped.stderr();
      fail(/purge|signing|runner_volume/.test(marker) ? "purge_policy_required" : "child_early_exit");
    }
    try { if ((await get(url, token)) === 200) return; } catch {}
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 100));
  }
  fail("service_not_ready");
}
async function seed(psql, database, accountId, email, isolatedAccountId, isolatedEmail) {
  const sql = `
INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining, email_verified_at)
VALUES ('${accountId}', '${email}', 'fixture-only', 0, now());
INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining, email_verified_at)
VALUES ('${isolatedAccountId}', '${isolatedEmail}', 'fixture-only', 0, now());
UPDATE jobs_public_beta_cohorts SET state = 'closed_to_new', hard_cap = 2
WHERE id = 'public-v1';
INSERT INTO jobs_public_beta_enrollments (cohort_id, account_id, source, admitted_at_ms)
VALUES ('public-v1', '${accountId}', 'admin', 1);
INSERT INTO jobs_public_beta_enrollments (cohort_id, account_id, source, admitted_at_ms)
VALUES ('public-v1', '${isolatedAccountId}', 'admin', 1);
UPDATE jobs_public_beta_cohorts SET assigned_count = (
  SELECT COUNT(*) FROM jobs_public_beta_enrollments WHERE cohort_id = 'public-v1'
) WHERE id = 'public-v1';`;
  const env = pgEnv(database, process.env.BLUEY_NONPROD_CHILD_HOME);
  const psqlChild = trackTransient(spawn(psql, ["-X", "-v", "ON_ERROR_STOP=1", "-q"],
    { env, stdio: ["pipe", "ignore", "ignore"] }));
  psqlChild.stdin.end(sql);
  const [code, signal] = await withTimeout(once(psqlChild, "exit"), "seed");
  assert.equal(code, 0, "fixture_seed_exit_code"); assert.equal(signal, null, "fixture_seed_exit_signal");
}
function pgEnv(database, home) {
  return { PATH: process.env.PATH, HOME: home, TMPDIR: join(home, "tmp"), TMP: join(home, "tmp"), TEMP: join(home, "tmp"),
    PGHOST: database.hostname, PGPORT: database.port || "5432", PGUSER: decodeURIComponent(database.username),
    PGPASSWORD: decodeURIComponent(database.password), PGDATABASE: database.pathname.slice(1) };
}
async function postgresIdentity(psql, database, home, root) {
  const query = "SELECT json_build_object('data_directory',current_setting('data_directory'),'host',coalesce(host(inet_server_addr()),''),'port',inet_server_port(),'database',current_database(),'version',current_setting('server_version_num'))::text";
  const check = trackTransient(spawn(psql, ["-X", "-A", "-t", "-q", "-c", query],
    { env: pgEnv(database, home), stdio: ["ignore", "pipe", "ignore"] }));
  let stdout = ""; check.stdout.on("data", (chunk) => { stdout += chunk; });
  const [code, signal] = await withTimeout(once(check, "close"), "postgres_identity");
  assert.equal(code, 0, "postgres_identity_exit"); assert.equal(signal, null, "postgres_identity_signal");
  const identity = JSON.parse(stdout.trim());
  assert.equal(identity.data_directory, join(root, "nonprod-postgres", "data"), "postgres_data_directory");
  assert.ok(["127.0.0.1", "::1"].includes(identity.host), "postgres_server_host");
  assert.equal(String(identity.port), database.port || "5432", "postgres_server_port");
  assert.equal(identity.database, database.pathname.slice(1), "postgres_database");
  assert.match(String(identity.version), /^17\d{4,}$/, "postgres_version");
}
async function packetLedgerSnapshot(psql, database, home, accountId) {
  if (!/^ui_fixture_[0-9a-f-]{36}$/.test(accountId)) fail("fixture_account_id");
  const query = "SELECT json_build_object('attempts',(SELECT count(*) FROM jobs_attempt_reservations WHERE account_id=:'fixture_account'),'metering',(SELECT count(*) FROM jobs_packet_metering WHERE account_id=:'fixture_account'),'included_metering',(SELECT count(*) FROM jobs_packet_metering WHERE account_id=:'fixture_account' AND included=1),'metered_amount_cents',coalesce((SELECT sum(amount_cents) FROM jobs_packet_metering WHERE account_id=:'fixture_account'),0),'commands',(SELECT count(*) FROM jobs_workflow_commands WHERE account_id=:'fixture_account'),'balance_events',(SELECT count(*) FROM balance_ledger_entries WHERE account_id=:'fixture_account'),'balance_cents',(SELECT balance_cents FROM accounts WHERE id=:'fixture_account'),'used_packets',coalesce((SELECT used_packets FROM jobs_entitlements WHERE account_id=:'fixture_account'),0))::text";
  const command = trackTransient(spawn(psql, ["-X", "-A", "-t", "-q", "-v",
    "ON_ERROR_STOP=1", "-v", `fixture_account=${accountId}`],
  { env: pgEnv(database, home), stdio: ["pipe", "pipe", "ignore"] }));
  let stdout = ""; command.stdout.on("data", (chunk) => { stdout += chunk; if (stdout.length > 4096) command.kill("SIGTERM"); });
  const completion = once(command, "close"); command.stdin.end(`${query};\n`);
  const [code, signal] = await withTimeout(completion, "ledger_snapshot");
  assert.equal(code, 0, "ledger_snapshot_exit"); assert.equal(signal, null, "ledger_snapshot_signal");
  return JSON.parse(stdout.trim());
}
async function humanPgSnapshot(psql, database, home, accountId) {
  if (!/^ui_fixture_[0-9a-f-]{36}$/.test(accountId)) fail("fixture_account_id");
  const query = `SELECT json_build_object(
    'account_rows',(SELECT count(*) FROM accounts WHERE id=:'fixture_account'),
    'profile_rows',(SELECT count(*) FROM jobs_profiles WHERE account_id=:'fixture_account'),
    'profile_ciphertext',(SELECT profile_json FROM jobs_profiles WHERE account_id=:'fixture_account'),
    'application_rows',(SELECT count(*) FROM jobs_applications WHERE account_id=:'fixture_account'),
    'prepared_rows',(SELECT count(*) FROM jobs_applications WHERE account_id=:'fixture_account'
      AND state='awaiting_review' AND submitted_at_ms IS NULL),
    'submitted_rows',(SELECT count(*) FROM jobs_applications WHERE account_id=:'fixture_account'
      AND submitted_at_ms IS NOT NULL),
    'application',(SELECT json_build_object('account_id',account_id,'id',id,'job_id',job_id,
      'resume_version_id',resume_version_id,'state',state,
      'application_ciphertext',application_json,'created_at_ms',created_at_ms,
      'submitted_at_ms',submitted_at_ms) FROM jobs_applications
      WHERE account_id=:'fixture_account'),
    'resume',(SELECT json_build_object('account_id',resume.account_id,'id',resume.id,
      'job_id',resume.job_id,'version_no',resume.version_no,'mode',resume.mode,
      'content_ciphertext',resume.content_json,'diff_ciphertext',resume.diff_json,
      'claim_ids_ciphertext',resume.claim_ids_json,'checksum',resume.checksum,
      'created_at_ms',resume.created_at_ms)
      FROM jobs_resume_versions resume JOIN jobs_applications application
        ON application.resume_version_id=resume.id AND application.account_id=resume.account_id
      WHERE application.account_id=:'fixture_account'),
    'browser_sessions',(SELECT count(*) FROM jobs_browser_sessions
      WHERE account_id=:'fixture_account'),
    'workflow_commands',(SELECT count(*) FROM jobs_workflow_commands
      WHERE account_id=:'fixture_account'),
    'run_events',(SELECT count(*) FROM jobs_run_events WHERE account_id=:'fixture_account'),
    'irreversible_receipts',(SELECT count(*)
      FROM jobs_managed_cloud_irreversible_effect_receipts WHERE account_id=:'fixture_account')
  )::text`;
  const command = trackTransient(spawn(psql, ["-X", "-A", "-t", "-q", "-v",
    "ON_ERROR_STOP=1", "-v", `fixture_account=${accountId}`], {
    env: pgEnv(database, home), stdio: ["pipe", "pipe", "ignore"],
  }));
  let stdout = "";
  command.stdout.on("data", (chunk) => {
    stdout += chunk; if (stdout.length > 4 * 1024 * 1024) command.kill("SIGTERM");
  });
  const completion = once(command, "close"); command.stdin.end(`${query};\n`);
  const [code, signal] = await withTimeout(completion, "human_pg_snapshot");
  assert.equal(code, 0, "human_pg_snapshot_exit"); assert.equal(signal, null, "human_pg_snapshot_signal");
  return JSON.parse(stdout.trim());
}
async function boundedCommand(command, args, label, allowedCodes = [0]) {
  const processHandle = trackTransient(spawn(command, args,
    { stdio: ["ignore", "pipe", "ignore"] }));
  let stdout = "";
  processHandle.stdout.on("data", (chunk) => {
    stdout += chunk; if (stdout.length > 1024 * 1024) processHandle.kill("SIGTERM");
  });
  const [code, signal] = await withTimeout(once(processHandle, "close"), label, true);
  assert.equal(signal, null, `${label}_signal`); assert(allowedCodes.includes(code), `${label}_exit`);
  return stdout;
}
async function assertPortRefused(port) {
  assert(Number.isInteger(port) && port > 0 && port <= 65535, "cleanup_port");
  await new Promise((resolveRefused, reject) => {
    let settled = false;
    const finish = (callback, value) => {
      if (!settled) { settled = true; clearTimeout(timer); callback(value); }
    };
    const request = httpRequest({ host: "127.0.0.1", port, path: "/", method: "GET",
      agent: false }, (response) => {
      response.resume(); finish(reject, new Error("cleanup_port_still_listening"));
    });
    const timer = setTimeout(() => {
      request.destroy(); finish(reject, new Error("cleanup_port_timeout"));
    }, 1000);
    request.on("error", (error) => error.code === "ECONNREFUSED"
      ? finish(resolveRefused) : finish(reject, new Error("cleanup_port_ambiguous")));
    request.end();
  });
}
async function humanCleanupInventory() {
  assert(ownedBrowserRoot && cleanupPorts.length === 4
    && new Set(cleanupPorts).size === 4, "cleanup_inventory_scope");
  const ps = await boundedCommand("/bin/ps", ["-axo", "pid=,uid=,command="], "cleanup_ps");
  const owned = new RegExp(`^\\s*\\d+\\s+${process.getuid()}\\s+`);
  const enginePs = ps.split("\n").filter((line) => owned.test(line)
    && line.includes(ownedBrowserRoot)).length;
  const lsof = await boundedCommand("/usr/sbin/lsof",
    ["-nP", "-Fp", "+D", ownedBrowserRoot], "cleanup_lsof", [0, 1]);
  const engineLsof = new Set(lsof.split("\n").filter((line) => /^p\d+$/.test(line))).size;
  for (const port of cleanupPorts) await assertPortRefused(port);
  return { enginePs, engineLsof, portsRefused: cleanupPorts.length };
}
async function humanHoldQualification({ context, page, portalPort, jobsPort, accountPort,
  token, isolatedToken, tokenIssuedAtMs, psql, database, childHome, accountId, fixtureResult }) {
  const origin = `http://127.0.0.1:${portalPort}`;
  let release;
  const released = new Promise((resolveRelease) => { release = resolveRelease; });
  await page.exposeBinding("__blueyHumanFinish", ({ frame }) => {
    assert.equal(frame, page.mainFrame(), "human_finish_main_frame");
    assert.equal(new URL(frame.url()).origin, origin, "human_finish_own_origin");
    release(); return true;
  });
  await context.addInitScript(mountHumanBanner, origin);
  await page.evaluate(mountHumanBanner, origin);
  const canaryRejected = await page.evaluate(async (canary) => {
    try {
      await fetch(canary, { method: "GET", credentials: "omit", cache: "no-store",
        redirect: "error", referrerPolicy: "no-referrer" });
      return false;
    } catch { return true; }
  }, HUMAN_CANARY_URL);
  assert.equal(canaryRejected, true, "human_canary_transport_blocked");
  assert.equal(humanResult.requests.canary_blocked, 1, "human_canary_intercept_count");
  humanResult.network_canary_blocked = true;
  passHumanCase("bootstrap");
  humanResult.ready = true; passHumanCase("ready");
  const remaining = HUMAN_HOLD_MS - (Date.now() - tokenIssuedAtMs);
  assert(remaining > 0, "human_jwt_hold_expired_before_ready");
  process.stdout.write(`${JSON.stringify({ event: "human_browser_ready", mode: "human_hold",
    expires_within_seconds: Math.floor(remaining / 1000), sourceAccepted: false })}\n`);
  let timer;
  await withAbort(Promise.race([
    released,
    new Promise((_, reject) => { timer = setTimeout(() => reject(new Error("human_hold_timeout")), remaining); }),
    new Promise((_, reject) => page.once("close", () => reject(new Error("human_window_closed")))),
  ])).finally(() => clearTimeout(timer));
  humanResult.hold_ms = Date.now() - tokenIssuedAtMs;
  assert(humanResult.hold_ms <= HUMAN_HOLD_MS, "human_hold_timeout");
  humanResult.operator_release_received = true; passHumanCase("operator_release_received");

  const accountBefore = await getJson(`http://127.0.0.1:${accountPort}/account/me`, token);
  humanResult.account_status_before = 200;
  const workspaceBefore = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token);
  const pgBefore = await humanPgSnapshot(psql, database, childHome, accountId);
  assert.equal(workspaceBefore.applications.length, 1, "human_exactly_one_application");
  const application = workspaceBefore.applications[0];
  assert.equal(application.job_id, fixtureResult.job_id, "human_fixture_job_binding");
  assert.equal(application.submission_mode, "review_first", "human_review_first_required");
  assert.equal(application.state, "awaiting_review", "human_awaiting_review_required");
  assert.equal(application.submitted_at_ms, null, "human_submission_forbidden");
  assert.equal(pgBefore.account_rows, 1, "human_pg_account_row");
  assert.equal(pgBefore.profile_rows, 1, "human_pg_profile_row");
  assert.equal(pgBefore.application_rows, 1, "human_pg_application_row");
  assert.equal(pgBefore.prepared_rows, 1, "human_pg_prepared_row");
  assert.equal(pgBefore.submitted_rows, 0, "human_pg_submitted_row");
  const resumeBefore = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/resume-versions/${encodeURIComponent(application.resume_version_id)}`, token);
  assert.equal(pgBefore.application.account_id, accountId, "human_pg_application_account");
  assert.equal(pgBefore.application.id, application.id, "human_pg_application_id");
  assert.equal(pgBefore.application.job_id, fixtureResult.job_id, "human_pg_application_job");
  assert.equal(pgBefore.application.resume_version_id, application.resume_version_id,
    "human_pg_application_resume");
  assert.equal(pgBefore.resume.account_id, pgBefore.application.account_id,
    "human_pg_resume_account");
  assert.equal(pgBefore.resume.id, pgBefore.application.resume_version_id, "human_pg_resume_id");
  assert.equal(pgBefore.resume.job_id, pgBefore.application.job_id, "human_pg_resume_job");
  for (const value of [pgBefore.profile_ciphertext, pgBefore.application.application_ciphertext,
    pgBefore.resume.content_ciphertext, pgBefore.resume.diff_ciphertext,
    pgBefore.resume.claim_ids_ciphertext]) {
    assert.equal(typeof value, "string", "human_pg_opaque_type");
    assert(value.length > 0 && value.length <= 4 * 1024 * 1024, "human_pg_opaque_bound");
  }
  const profileSha = canonicalSha256(pgBefore.profile_ciphertext);
  const applicationSha = canonicalSha256(pgBefore.application);
  const resumeSha = canonicalSha256(pgBefore.resume);
  assert.match(profileSha, /^[a-f0-9]{64}$/); assert.match(applicationSha, /^[a-f0-9]{64}$/);
  assert.match(resumeSha, /^[a-f0-9]{64}$/); assert.match(canonicalSha256(resumeBefore), /^[a-f0-9]{64}$/);
  passHumanCase("prepared_before_reload");

  await page.goto(`${origin}/jobs/applications`, { waitUntil: "networkidle", timeout: OPERATION_MS });
  const row = page.locator(".application-row");
  assert.equal(await row.count(), 1, "human_visible_single_application");
  const packetPolicy = row.locator(".application-packet small");
  await packetPolicy.getByText(/^(?:v[1-9]\d*|Prepared) · Review First$/)
    .waitFor({ timeout: OPERATION_MS });
  await row.click();
  await page.getByRole("heading", { name: "Application kit" }).waitFor({ timeout: OPERATION_MS });
  await page.reload({ waitUntil: "networkidle", timeout: OPERATION_MS });
  assert.equal(await page.locator(".application-row").count(), 1, "human_reload_single_application");
  await page.locator(".application-row").click();
  await page.getByRole("heading", { name: "Application kit" }).waitFor({ timeout: OPERATION_MS });
  const accountAfter = await getJson(`http://127.0.0.1:${accountPort}/account/me`, token);
  humanResult.account_status_after = 200;
  const workspaceAfter = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token);
  const pgAfter = await humanPgSnapshot(psql, database, childHome, accountId);
  const resumeAfter = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/resume-versions/${encodeURIComponent(application.resume_version_id)}`, token);
  assert.deepEqual(accountAfter, accountBefore, "human_account_reload_stable");
  assert.deepEqual(workspaceAfter.profile, workspaceBefore.profile, "human_profile_reload_stable");
  assert.deepEqual(workspaceAfter.applications, workspaceBefore.applications,
    "human_application_reload_stable");
  assert.deepEqual(resumeAfter, resumeBefore, "human_resume_reload_stable");
  assert.equal(pgAfter.application.id, application.id, "human_pg_reload_application_id");
  assert.equal(pgAfter.application.job_id, fixtureResult.job_id, "human_pg_reload_application_job");
  assert.equal(pgAfter.application.resume_version_id, application.resume_version_id,
    "human_pg_reload_application_resume");
  assert.equal(pgAfter.resume.account_id, pgAfter.application.account_id,
    "human_pg_reload_resume_account");
  assert.equal(canonicalSha256(pgAfter.profile_ciphertext), profileSha,
    "human_pg_profile_hash_stable");
  assert.equal(canonicalSha256(pgAfter.application), applicationSha,
    "human_pg_application_hash_stable");
  assert.equal(canonicalSha256(pgAfter.resume), resumeSha, "human_pg_resume_hash_stable");
  humanResult.profile_stable = true; humanResult.application_stable = true;
  humanResult.resume_stable = true; passHumanCase("prepared_after_reload");

  humanResult.unauthenticated_status = await get(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`);
  assert.equal(humanResult.unauthenticated_status, 401, "human_unauthenticated_exact_denial");
  const isolatedWorkspace = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, isolatedToken);
  humanResult.isolated_application_rows = isolatedWorkspace.applications.length;
  assert.equal(humanResult.isolated_application_rows, 0, "human_isolated_application_rows");
  assert.notDeepEqual(isolatedWorkspace.profile, workspaceAfter.profile, "human_profile_isolation");
  passHumanCase("account_isolation");

  for (const field of ["account_rows", "profile_rows", "application_rows", "prepared_rows",
    "submitted_rows", "browser_sessions", "workflow_commands", "run_events",
    "irreversible_receipts"]) humanResult[field] = pgAfter[field];
  assert.deepEqual({ submitted_rows: pgAfter.submitted_rows,
    browser_sessions: pgAfter.browser_sessions, workflow_commands: pgAfter.workflow_commands,
    run_events: pgAfter.run_events, irreversible_receipts: pgAfter.irreversible_receipts },
  { submitted_rows: 0, browser_sessions: 0, workflow_commands: 0,
    run_events: 0, irreversible_receipts: 0 }, "human_effect_fence");
  assert.deepEqual(workspaceAfter.browser_sessions, [], "human_no_browser_sessions");
  assert.deepEqual(workspaceAfter.interventions, [], "human_no_interventions");
  assert.deepEqual(workspaceAfter.auto_submit_authorizations, [], "human_no_auto_submit_authority");
  passHumanCase("zero_effects");
}
function mime(path) { return ({ ".js": "text/javascript", ".css": "text/css", ".svg": "image/svg+xml", ".json": "application/json" })[extname(path)] || "text/html"; }
function proxyServer(dist, jobsPort, accountPort) {
  return createServer(async (request, response) => {
    const url = new URL(request.url, "http://127.0.0.1");
    let port;
    if (url.pathname.startsWith("/api/jobs")) { result.requests.jobs += 1; port = jobsPort; }
    else if (url.pathname === "/account/me") { result.requests.account += 1; port = accountPort; }
    if (port) {
      const upstream = httpRequest({ host: "127.0.0.1", port, path: `${url.pathname}${url.search}`, method: request.method, headers: request.headers }, (upstreamResponse) => { response.writeHead(upstreamResponse.statusCode || 502, upstreamResponse.headers); upstreamResponse.pipe(response); });
      upstream.on("error", () => { response.writeHead(502).end(); }); request.pipe(upstream); return;
    }
    if (url.pathname.startsWith("/api/") || !url.pathname.startsWith("/jobs/")) { response.writeHead(404).end(); return; }
    if (request.method !== "GET" && request.method !== "HEAD") { response.writeHead(405).end(); return; }
    const requested = normalize(url.pathname.slice("/jobs/".length)).replace(/^[/\\]+/, "");
    const file = resolve(dist, requested || "index.html");
    if (!within(dist, file)) { response.writeHead(400).end(); return; }
    try {
      const info = await lstat(file); const resolved = await realpath(file);
      if (!info.isFile() || info.isSymbolicLink() || !within(dist, resolved)) throw new Error();
      response.writeHead(200, { "content-type": mime(file) }); response.end(await readFile(file));
    } catch {
      if (extname(requested)) { response.writeHead(404).end(); return; }
      response.writeHead(200, { "content-type": "text/html" }); response.end(await readFile(join(dist, "index.html")));
    }
  });
}
async function main() {
  checkAbort(); stage = "preflight";
  const requestedRoot = required("BLUEY_TEST_RUN_ROOT");
  const rootInfo = await lstat(requestedRoot);
  if (rootInfo.isSymbolicLink() || !rootInfo.isDirectory() || rootInfo.uid !== process.getuid() || (rootInfo.mode & 0o777) !== 0o700) fail("test_root_identity");
  const root = await realpath(requestedRoot);
  if (root !== requestedRoot || !(/^\/private\/tmp\/bluey-test-runs-/.test(root) || /^\/tmp\/bluey-test-runs-/.test(root))) fail("test_root_prefix");
  if (process.env.BLUEY_NONPROD_ROOT && await realpath(process.env.BLUEY_NONPROD_ROOT) !== root) fail("nonprod_root_mismatch");
  const database = onlyLoopbackPostgres(required("BLUEY_TEST_POSTGRES_URL"));
  const target = await ownedPath(root, required("CARGO_TARGET_DIR"), "cargo_target_dir");
  const dist = await ownedPath(root, required("BLUEY_PORTAL_DIST"), "portal_dist");
  ownedBrowserRoot = await ownedPath(root, required("PLAYWRIGHT_BROWSERS_PATH"),
    "playwright_browsers");
  const privateRuntime = join(root, "nonprod-browser-api");
  await mkdir(privateRuntime, { mode: 0o700 });
  const childHome = join(privateRuntime, "child-home"); const childTmp = join(childHome, "tmp");
  await mkdir(childHome, { mode: 0o700 }); await mkdir(childTmp, { mode: 0o700 });
  process.env.BLUEY_NONPROD_CHILD_HOME = childHome;
  outputRoot = root;
  const psql = join(required("BLUEY_POSTGRES_BIN"), "psql");
  if (!psql.startsWith("/opt/homebrew/opt/postgresql@17/bin/")) fail("unapproved_psql");
  const psqlPath = await realpath(psql); const psqlInfo = await lstat(psqlPath);
  if (!psqlPath.startsWith("/opt/homebrew/Cellar/postgresql@17/") || !psqlInfo.isFile()
    || (psqlInfo.uid !== process.getuid() && psqlInfo.uid !== 0)) fail("psql_identity");
  const jobsBinary = join(target, "debug", "bluey-jobs-api");
  const sharedBinary = join(target, "debug", "bluey-server");
  const fixtureConfigured = required("BLUEY_NONPROD_PACKET_FIXTURE_BIN");
  const fixtureInfo = await lstat(fixtureConfigured); const fixtureBinary = await realpath(fixtureConfigured);
  if (fixtureInfo.isSymbolicLink() || !fixtureInfo.isFile() || fixtureInfo.uid !== process.getuid() || (fixtureInfo.mode & 0o022) !== 0 || (fixtureInfo.mode & 0o100) === 0 || fixtureBinary !== join(target, "debug", "examples", "jobs-nonprod-browser-fixture")) fail("packet_fixture_path");
  await stat(jobsBinary); await stat(sharedBinary); await stat(join(dist, "index.html"));
  stage = "pg_identity"; await postgresIdentity(psql, database, childHome, root);
  const jobsPort = await freePort(); const accountPort = await freePort();
  const secret = randomBytes(32).toString("base64url");
  const dataKey = randomBytes(32).toString("base64");
  const purgePolicy = fixturePurgeSigningPolicy();
  const accountId = `ui_fixture_${randomUUID()}`;
  const email = `ui-fixture-${randomUUID()}@example.test`;
  const isolatedAccountId = `ui_isolated_${randomUUID()}`;
  const isolatedEmail = `ui-isolated-${randomUUID()}@example.test`;
  objectStore = await startNonprodObjectStore({ accountId });
  cleanupPorts.push(Number(new URL(objectStore.endpoint).port)); checkAbort();
  const common = {
    PATH: process.env.PATH, HOME: childHome, TMPDIR: childTmp, TMP: childTmp, TEMP: childTmp,
    SQLITE_TMPDIR: childTmp, LANG: "C", TZ: "UTC",
    BLUEY_SERVER_DB_BACKEND: "postgres", BLUEY_DATABASE_URL: required("BLUEY_TEST_POSTGRES_URL"),
    BLUEY_JWT_SECRET: secret, BLUEY_JOBS_DATA_KEY: dataKey,
    BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY_ID: purgePolicy.keyId,
    BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY: purgePolicy.seed,
    BLUEY_JOBS_RUNNER_PURGE_VERIFYING_KEYS_JSON: JSON.stringify([
      { keyId: purgePolicy.keyId, publicKeyBase64url: purgePolicy.publicKey },
    ]),
    BLUEY_JOBS_RUNNER_MINIMUM_BUILD_ID: "runner-630.0",
    BLUEY_JOBS_BETA_ENABLED: "1", BLUEY_JOBS_MODEL_GENERATION_ENABLED: "0",
    BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED: "0",
    BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED: "0", BLUEY_JOBS_MAILBOX_SYNC_ENABLED: "0",
    BLUEY_JOBS_GLOBAL_ARCHIVE_ENABLED: "0", BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED: "0",
    BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED: "0", BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED: "0",
    BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED: "0",
    BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED: "0", ...objectStore.env, RUST_LOG: "warn",
  };
  stage = "start_jobs_api";
  child(jobsBinary, [], { ...common, BLUEY_JOBS_API_HOST: "127.0.0.1", BLUEY_JOBS_API_PORT: String(jobsPort) }, "bluey-jobs-api");
  await waitFor(`http://127.0.0.1:${jobsPort}/health`);
  stage = "start_account_api";
  child(sharedBinary, [], { ...common, BLUEY_API_HOST: "127.0.0.1", BLUEY_PORT: String(accountPort) }, "bluey-server");
  await waitFor(`http://127.0.0.1:${accountPort}/health`);
  stage = "seed";
  await seed(psql, database, accountId, email, isolatedAccountId, isolatedEmail);
  const tokenIssuedAtMs = Date.now();
  const token = fixtureToken(accountId, secret);
  const humanIsolatedToken = HUMAN_HOLD ? fixtureToken(isolatedAccountId, secret) : null;
  checkAbort();
  const proxy = proxyServer(dist, jobsPort, accountPort); liveProxy = proxy;
  const portalPort = await withTimeout(listen(proxy), "proxy_listen", true);
  cleanupPorts.push(jobsPort, accountPort, portalPort); checkAbort();
  const browser = await withTimeout(chromium.launch({ headless: !HUMAN_HOLD }),
    "browser_launch", true); liveBrowser = browser; checkAbort();
  try {
    const context = HUMAN_HOLD
      ? await browser.newContext({ serviceWorkers: "block", acceptDownloads: false }) : await browser.newContext();
    const portalOrigin = `http://127.0.0.1:${portalPort}`;
    await context.route("**/*", (route) => {
      const request = route.request();
      if (HUMAN_HOLD && request.url() === HUMAN_CANARY_URL) {
        assert.equal(request.method(), "GET", "human_canary_method");
        assert.equal(request.postData(), null, "human_canary_body");
        assert.equal(request.headers().authorization, undefined, "human_canary_authorization");
        humanResult.requests.canary_blocked += 1; return route.abort();
      }
      if (new URL(request.url()).origin === portalOrigin) return route.continue();
      if (HUMAN_HOLD) humanResult.requests.denied_non_origin += 1;
      return route.abort();
    });
    if (HUMAN_HOLD) await context.routeWebSocket("**/*", (socket) => {
      humanResult.requests.denied_websockets += 1; socket.close();
    });
    await context.addInitScript(([access, persistence]) => { localStorage.setItem("bluey_access_token", access); localStorage.setItem("bluey_auth_persistence", persistence); }, [token, "local"]);
    const page = await context.newPage();
    if (HUMAN_HOLD) {
      page.on("popup", (popup) => {
        humanResult.requests.denied_popups += 1; void popup.close();
      });
      page.on("download", (download) => {
        humanResult.requests.denied_downloads += 1; void download.cancel();
      });
      page.on("filechooser", (chooser) => {
        result.failed.push("human_file_chooser_forbidden"); void chooser.setFiles([]);
      });
    }
    let pageErrors = 0; let serverErrors = 0;
    page.on("pageerror", () => { pageErrors += 1; });
    page.on("response", (response) => { if (response.status() >= 500) serverErrors += 1; });
    stage = "portal_load";
    await withTimeout(page.goto(`http://127.0.0.1:${portalPort}/jobs/`, { waitUntil: "networkidle" }), "portal_load");
    await page.getByRole("heading", { name: "Import once, then tailor every job" }).waitFor({ timeout: OPERATION_MS });
    stage = "manual_profile";
    await page.getByRole("button", { name: "I do not have a resume" }).click();
    await page.getByLabel("Full name").fill("Synthetic Candidate");
    await page.getByLabel("Resume contact email").fill(email);
    await page.getByLabel("Phone").fill("555-0100");
    await page.getByLabel("Current location").fill("New York, NY");
    await page.getByLabel("Professional headline").fill("Software Engineer");
    await page.getByRole("button", { name: "Continue" }).click();
    await page.getByRole("heading", { name: "Work history" }).waitFor({ timeout: OPERATION_MS });
    stage = "persistence";
    await page.reload({ waitUntil: "networkidle", timeout: OPERATION_MS });
    await page.getByRole("heading", { name: "Work history" }).waitFor({ timeout: OPERATION_MS });
    passCase("profile_reload");
    stage = "employment_entry";
    await page.getByRole("button", { name: "Add role" }).click();
    await page.getByLabel("Company").fill("Synthetic Systems");
    await page.getByLabel("Title").fill("Software Engineer");
    await page.getByLabel("Location").fill("New York, NY");
    await page.getByLabel("Start").fill("2022-01");
    await page.getByLabel("Highlights (one per line)").fill("Built synthetic test systems");
    await page.getByRole("button", { name: "Continue" }).click();
    await page.getByRole("heading", { name: "Education and skills" }).waitFor({ timeout: OPERATION_MS });
    stage = "skills_entry";
    await page.getByLabel("Skills").fill("TypeScript"); await page.getByLabel("Skills").press("Enter");
    await page.getByRole("button", { name: "Continue" }).click();
    await page.getByRole("heading", { name: "Where should Bluey look?" }).waitFor({ timeout: OPERATION_MS });
    stage = "search_preferences";
    await page.getByLabel("Target roles").fill("Software Engineer"); await page.getByLabel("Target roles").press("Enter");
    await page.getByLabel("Target locations").fill("New York, NY"); await page.getByLabel("Target locations").press("Enter");
    await page.getByRole("checkbox", { name: "Full-time" }).setChecked(true);
    await page.getByRole("checkbox", { name: "W-2" }).setChecked(true); await page.getByRole("checkbox", { name: "C2C" }).setChecked(true);
    await page.getByRole("button", { name: "Continue" }).click();
    await page.getByRole("heading", { name: "How should Bluey work?" }).waitFor({ timeout: OPERATION_MS });
    stage = "review_defaults";
    await page.getByRole("button", { name: /Review first/ }).click();
    await page.getByRole("button", { name: "Continue" }).click();
    await page.getByRole("heading", { name: "Your first Career Track is ready" }).waitFor({ timeout: OPERATION_MS });
    stage = "onboarding_completion";
    await page.getByRole("button", { name: "Find my matches" }).click();
    await page.waitForURL(/\/jobs\/matches/, { timeout: OPERATION_MS });
    passCase("onboarding_complete");
    stage = "completed_workspace_reload";
    await page.reload({ waitUntil: "networkidle", timeout: OPERATION_MS });
    await page.getByRole("heading", { name: "Matches" }).waitFor({ timeout: OPERATION_MS });
    assert.ok(result.requests.jobs >= 2, "workspace_requests"); assert.ok(result.requests.account >= 1, "account_request");
    assert.equal(pageErrors, 0, "portal_page_errors"); assert.equal(serverErrors, 0, "portal_server_errors");
    const workspace = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token);
    assert.equal(workspace.profile.full_name, "Synthetic Candidate", "profile_persisted");
    assert.equal(workspace.matches.length, 0, "empty_matches");
    assert.equal(workspace.profile.onboarding_complete, true, "onboarding_persisted");
    assert.equal(workspace.profile.employment.length, 1, "employment_count_persisted");
    const employment = workspace.profile.employment[0];
    assert.equal(employment.company, "Synthetic Systems", "employment_company_persisted"); assert.equal(employment.title, "Software Engineer", "employment_title_persisted");
    assert.equal(employment.location, "New York, NY", "employment_location_persisted"); assert.equal(employment.start_date, "2022-01", "employment_start_persisted");
    assert.equal(employment.end_date, "", "employment_end_persisted"); assert.deepEqual(employment.highlights, ["Built synthetic test systems"], "employment_highlights_persisted");
    assert.deepEqual(workspace.preferences.desired_roles, ["Software Engineer"], "target_role_persisted");
    assert.deepEqual(workspace.preferences.desired_locations, ["New York, NY"], "target_location_persisted");
    assert.equal(workspace.profile.default_submission_mode, "review_first", "review_first_persisted");
    assert.deepEqual(workspace.preferences.employment_types, ["full_time"], "employment_type_persisted");
    assert.deepEqual([...workspace.preferences.engagement_types].sort(), ["c2c", "w2"], "engagement_preferences_persisted");
    assert.equal(workspace.tracks.length, 1, "track_persisted");
    assert.equal(workspace.tracks[0].role, "Software Engineer", "track_role_binding");
    assert.deepEqual(workspace.tracks[0].locations, ["New York, NY"], "track_location_binding"); passCase("preference_track_reload");
    stage = "account_isolation";
    assert.equal(await get(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`), 401, "unauthenticated_denied");
    const firstSnapshot = structuredClone({ profile: workspace.profile, preferences: workspace.preferences, tracks: workspace.tracks, applications: workspace.applications });
    const isolatedToken = HUMAN_HOLD ? humanIsolatedToken : fixtureToken(isolatedAccountId, secret);
    const isolatedWorkspace = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, isolatedToken);
    assert.notEqual(isolatedWorkspace.profile.full_name, "Synthetic Candidate", "isolated_profile_no_leak");
    assert.deepEqual(isolatedWorkspace.profile.employment, [], "isolated_employment_no_leak");
    assert.deepEqual(isolatedWorkspace.preferences.desired_roles, [], "isolated_roles_no_leak");
    assert.deepEqual(isolatedWorkspace.preferences.desired_locations, [], "isolated_locations_no_leak");
    assert.equal(isolatedWorkspace.tracks.length, 0, "isolated_tracks_no_leak"); assert.equal(isolatedWorkspace.applications.length, 0, "isolated_applications_no_leak");
    const isolatedProfile = { ...isolatedWorkspace.profile, full_name: "Isolated Candidate", email: isolatedEmail, phone: "555-0101", current_location: "Boston, MA", headline: "QA Engineer" };
    assert.equal(await putJson(`http://127.0.0.1:${jobsPort}/api/jobs/profile`, isolatedToken, isolatedProfile), 200, "isolated_profile_update");
    const isolatedAfterUpdate = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, isolatedToken);
    assert.equal(isolatedAfterUpdate.profile.full_name, "Isolated Candidate", "isolated_profile_readback");
    const firstAfterIsolation = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token);
    assert.deepEqual({ profile: firstAfterIsolation.profile, preferences: firstAfterIsolation.preferences, tracks: firstAfterIsolation.tracks, applications: firstAfterIsolation.applications }, firstSnapshot, "first_tenant_families_unchanged"); passCase("auth_isolation");
    assert.deepEqual(workspace.applications, [], "no_fabricated_applications"); assert.deepEqual(workspace.interventions, [], "no_fabricated_interventions");
    assert.deepEqual(workspace.browser_sessions, [], "no_fabricated_browser_sessions"); assert.deepEqual(workspace.auto_submit_authorizations, [], "no_fabricated_auto_submit_authorizations");
    assert.equal(workspace.profile.source_resume_name, "", "no_source_resume");
    assert.equal(workspace.runner_availability.cloud.available, false, "cloud_unavailable");
    assert.equal(workspace.runner_availability.cloud.distribution_enabled, false, "cloud_distribution_disabled");
    assert.equal(workspace.runner_availability.auto_submit_available, false, "auto_submit_unavailable");
    for (const route of ["applications", "automation", "settings"]) {
      stage = `view_${route}`;
      await page.goto(`http://127.0.0.1:${portalPort}/jobs/${route}`, { waitUntil: "networkidle" });
      assert.equal(pageErrors, 0, `${route}_page_errors`); assert.equal(serverErrors, 0, `${route}_server_errors`);
      if (route === "applications") await page.getByRole("heading", { name: "No applications here" }).waitFor({ timeout: OPERATION_MS });
      if (route === "automation") {
        const locked = page.locator(".runner-option.locked");
        assert.equal(await locked.count(), 1, "cloud_effect_locked");
        await locked.getByText(workspace.runner_availability.cloud.reason, { exact: true }).waitFor({ timeout: OPERATION_MS });
        assert.equal(await page.getByRole("button", { name: "Start cloud automation" }).count(), 0, "no_start_action");
        assert.equal(await page.locator(".active-browser-run").count(), 0, "no_active_run");
      }
      if (route === "settings") {
        await page.getByText("Source resume required", { exact: true }).waitFor({ timeout: OPERATION_MS });
        await page.getByText("Canonical policy review required", { exact: true }).waitFor({ timeout: OPERATION_MS });
        assert.equal(await page.getByRole("button", { name: "Enable Auto-submit" }).isDisabled(), true, "auto_submit_disabled");
      }
    }
    const afterNavigation = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token);
    assert.deepEqual(afterNavigation.applications, [], "navigation_created_no_application");
    assert.deepEqual(afterNavigation.browser_sessions, [], "navigation_created_no_run");
    assert.deepEqual(afterNavigation.interventions, [], "navigation_created_no_intervention");
    assert.deepEqual(afterNavigation.auto_submit_authorizations, [], "navigation_created_no_authorization");
    passCase("effect_availability");
    stage = "track_identity_binding";
    const identityWorkspace = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token);
    const verifiedIdentity = identityWorkspace.application_identities.find((identity) => identity.verification_status === "verified");
    assert.ok(verifiedIdentity, "verified_application_identity");
    await page.goto(`http://127.0.0.1:${portalPort}/jobs/settings`, { waitUntil: "networkidle" });
    await page.locator(".track-settings-main").first().click();
    await page.getByRole("heading", { name: "Edit Career Track" }).waitFor({ timeout: OPERATION_MS });
    await page.getByLabel("Application email").selectOption(verifiedIdentity.id);
    await page.getByRole("button", { name: "Save track" }).click();
    await page.getByRole("status").getByText("Career Track updated.", { exact: true }).waitFor({ timeout: OPERATION_MS });
    const boundWorkspace = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token);
    assert.equal(boundWorkspace.tracks[0].application_identity_id, verifiedIdentity.id, "track_identity_persisted");
    assert.equal(boundWorkspace.application_identities.find((identity) => identity.id === verifiedIdentity.id)?.verification_status, "verified", "track_identity_verified");
    assert.equal(boundWorkspace.profile.source_resume_name, "", "track_binding_no_source_resume"); assert.equal(boundWorkspace.tracks[0].policy.authority.review_state, "needs_review", "track_binding_policy_needs_review");
    stage = "packet_fixture";
    const fixtureRequest = join(root, "browser-packet-fixture-request.json");
    await writeFile(fixtureRequest, `${JSON.stringify({ version: 1, account_id: accountId, track_id: boundWorkspace.tracks[0].id, queue_entitlement: true })}\n`, { flag: "wx", mode: 0o600 });
    const fixtureProcess = child(fixtureBinary, [], { PATH: process.env.PATH, HOME: childHome, TMPDIR: childTmp, TMP: childTmp, TEMP: childTmp, SQLITE_TMPDIR: childTmp, BLUEY_TEST_RUN_ROOT: root, BLUEY_TEST_POSTGRES_URL: required("BLUEY_TEST_POSTGRES_URL"), BLUEY_JOBS_DATA_KEY: dataKey }, "packet-fixture", true);
    const fixtureOutcome = await withTimeout(children.find(({ processHandle }) => processHandle === fixtureProcess).completion, "packet_fixture");
    result.fixture = { pid: fixtureProcess.pid, code: fixtureOutcome.code, signal: fixtureOutcome.signal };
    if (fixtureOutcome.code !== 0) fail(`fixture_${children.find(({ processHandle }) => processHandle === fixtureProcess).fixtureFailure()}`);
    assert.equal(fixtureOutcome.code, 0, "packet_fixture_exit"); assert.equal(fixtureOutcome.signal, null, "packet_fixture_signal");
    const fixtureResult = await ownedJson(join(root, "browser-packet-fixture-result.json"), root, { version: "number", job_id: "string" });
    assert.equal(fixtureResult.version, 1, "packet_fixture_version"); assert.ok(fixtureResult.job_id.length > 0, "packet_fixture_job");
    if (HUMAN_HOLD) {
      stage = "human_hold";
      await humanHoldQualification({ context, page, portalPort, jobsPort, accountPort, token,
        isolatedToken, tokenIssuedAtMs, psql, database, childHome, accountId, fixtureResult });
      assert.equal(pageErrors, 0, "human_final_page_errors");
      assert.equal(serverErrors, 0, "human_final_server_errors");
      humanResult.page_errors = pageErrors; humanResult.server_errors = serverErrors;
    } else {
    stage = "packet_review";
    await page.goto(`http://127.0.0.1:${portalPort}/jobs/matches`, { waitUntil: "networkidle" });
    await page.getByRole("heading", { name: "Matches" }).waitFor({ timeout: OPERATION_MS });
    await page.locator(".job-row").filter({ hasText: "Synthetic Qualification Employer" }).click({ timeout: OPERATION_MS });
    const [preparationResponse] = await Promise.all([
      page.waitForResponse((response) => new URL(response.url()).pathname === "/api/jobs/applications"
        && response.request().method() === "POST", { timeout: OPERATION_MS }),
      page.getByRole("button", { name: "Prepare application", exact: true }).click({ timeout: OPERATION_MS }),
    ]);
    assert.deepEqual(JSON.parse(preparationResponse.request().postData()),
      { job_id: fixtureResult.job_id, mode: "factual", submission_mode: "review_first" }, "browser_preparation_authority");
    const preparationBody = await preparationResponse.text();
    assert.ok(Buffer.byteLength(preparationBody) <= 1024 * 1024, "browser_preparation_body_limit");
    const prepared = { status: preparationResponse.status(), body: JSON.parse(preparationBody) };
    assert.equal(prepared.status, 200, "packet_prepare_status"); assert.equal(prepared.body.application.state, "awaiting_review", "packet_awaiting_review");
    assert.equal(prepared.body.resume_version.mode, "factual", "packet_factual_mode");
    assert.equal(prepared.body.resume_version.content.provenance.mode, "factual", "packet_factual_provenance");
    assert.equal(prepared.body.resume_version.job_id, fixtureResult.job_id, "packet_resume_job_binding");
    assert.equal(prepared.body.resume_version.content.provenance.generated_for_job_id, fixtureResult.job_id, "packet_provenance_job_binding");
    const packetWorkspace = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token);
    assert.equal(packetWorkspace.applications.length, 1, "packet_application_reloaded"); assert.equal(packetWorkspace.applications[0].state, "awaiting_review", "packet_state_reloaded"); assert.equal(packetWorkspace.applications[0].submission_mode, "review_first", "packet_review_first");
    await page.goto(`http://127.0.0.1:${portalPort}/jobs/applications`, { waitUntil: "networkidle" });
    assert.ok((await page.locator(".application-row").innerText()).includes("Review First"), "visible_review_first_policy");
    await page.locator(".application-row").click();
    const kit = page.locator(".review-panel");
    await kit.getByRole("heading", { name: "Application kit" }).waitFor({ timeout: OPERATION_MS });
    await kit.getByText(`v${prepared.body.resume_version.version_no} · Factual`, { exact: true }).waitFor({ timeout: OPERATION_MS });
    await kit.getByText("Auto-submit is unavailable", { exact: true }).waitFor({ timeout: OPERATION_MS });
    const paper = page.locator(".resume-paper");
    await paper.getByRole("heading", { name: "Synthetic Candidate" }).waitFor({ timeout: OPERATION_MS });
    const renderedResume = await paper.innerText();
    for (const fact of ["Synthetic Systems", "Software Engineer", "Built synthetic test systems"]) {
      assert.ok(renderedResume.includes(fact), "packet_visible_factual_content");
    }
    await page.reload({ waitUntil: "networkidle", timeout: OPERATION_MS }); await page.locator(".application-row").click(); await page.getByRole("heading", { name: "Application kit" }).waitFor({ timeout: OPERATION_MS });
    const persistedResume = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/resume-versions/${encodeURIComponent(prepared.body.resume_version.id)}`, token);
    assert.equal(persistedResume.mode, "factual", "persisted_factual_mode");
    assert.equal(persistedResume.content.provenance.mode, "factual", "persisted_factual_provenance");
    assert.deepEqual(persistedResume, prepared.body.resume_version, "immutable_resume_reload");
    passCase("packet_review");
    stage = "approval_fenced";
    const beforeApproval = structuredClone({ applications: packetWorkspace.applications, browser_sessions: packetWorkspace.browser_sessions, auto_submit_authorizations: packetWorkspace.auto_submit_authorizations });
    stage = "packet_ledger";
    const ledgerBefore = await packetLedgerSnapshot(psql, database, childHome, accountId);
    assert.deepEqual({ attempts: ledgerBefore.attempts, metering: ledgerBefore.metering, included_metering: ledgerBefore.included_metering, metered_amount_cents: ledgerBefore.metered_amount_cents, commands: ledgerBefore.commands, balance_events: ledgerBefore.balance_events, used_packets: ledgerBefore.used_packets }, { attempts: 0, metering: 0, included_metering: 0, metered_amount_cents: 0, commands: 0, balance_events: 0, used_packets: 0 }, "ledger_zero_before_approval");
    assert.equal(packetWorkspace.applications[0].receipt.approved_execution, undefined, "no_approved_execution_before");
    assert.equal(packetWorkspace.profile.source_resume_name, "", "approval_missing_source_resume");
    const packetPosting = packetWorkspace.matches.find((posting) => posting.id === fixtureResult.job_id);
    assert.ok(packetPosting, "packet_posting_present"); assert.equal(packetPosting.eligibility.can_queue_local, false, "packet_local_not_queueable"); assert.equal(packetPosting.eligibility.can_queue_cloud, false, "packet_cloud_not_queueable");
    assert.deepEqual(packetPosting.eligibility.hard_failures, [], "signed_posting_no_hard_failures");
    const policyReason = packetPosting.eligibility.review_reasons.find((reason) => reason.code === "career_track_policy_review_required");
    assert.ok(policyReason, "exact_track_review_reason");
    assert.equal(packetPosting.eligibility.review_reasons[0].code, policyReason.code, "first_queue_fence_is_track_review");
    assert.equal(packetWorkspace.tracks[0].policy.authority.review_state, "needs_review", "track_policy_needs_review");
    stage = "approval_fenced";
    const approval = await postJson(`http://127.0.0.1:${jobsPort}/api/jobs/applications/${encodeURIComponent(prepared.body.application.id)}/approve`, token, {});
    assert.equal(approval.status, 409, "approval_fenced_status");
    assert.equal(approval.body, policyReason.message, "approval_denied_for_track_review");
    const afterApproval = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token);
    const ledgerAfter = await packetLedgerSnapshot(psql, database, childHome, accountId);
    assert.deepEqual(ledgerAfter, ledgerBefore, "ledger_unchanged_after_approval"); assert.equal(afterApproval.entitlement.used_packets, ledgerAfter.used_packets, "workspace_ledger_packets");
    assert.deepEqual({ applications: afterApproval.applications, browser_sessions: afterApproval.browser_sessions, auto_submit_authorizations: afterApproval.auto_submit_authorizations }, beforeApproval, "approval_no_effects");
    assert.equal(afterApproval.applications[0].receipt.approved_execution, undefined, "no_approved_execution_after");
    assert.equal(pageErrors, 0, "packet_page_errors"); assert.equal(serverErrors, 0, "packet_server_errors");
    passCase("approval_fenced");
    stage = "source_resume_upload";
    const manualProfileBeforeSource = structuredClone(afterApproval.profile); const preferencesBeforeSource = structuredClone(afterApproval.preferences);
    const sourceBytes = Buffer.from("Synthetic Candidate\nSoftware Engineer\nNew York, NY\nSynthetic Systems\nSoftware Engineer, 2022-01\nBuilt synthetic test systems\nSkills: TypeScript\n", "utf8");
    const sourceRequest = { request_id: randomUUID(), file_name: "synthetic-source.txt", media_type: "text/plain", bytes_base64: sourceBytes.toString("base64") };
    const uploaded = await postJson(`http://127.0.0.1:${jobsPort}/api/jobs/resume-source`, token, sourceRequest);
    assert.equal(uploaded.status, 200, "source_upload_status"); assert.equal(uploaded.body.asset.file_type, "txt", "source_text_only"); assert.equal(uploaded.body.asset.template_status, "text_only", "source_template_text_only"); assert.equal(uploaded.body.asset.size_bytes, sourceBytes.length, "source_bytes"); assert.equal(uploaded.body.asset.sha256, createHash("sha256").update(sourceBytes).digest("hex"), "source_hash");
    const sourceMetadata = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/resume-source`, token);
    assert.equal(sourceMetadata.id, uploaded.body.asset.id, "source_metadata_id"); assert.equal(sourceMetadata.sha256, uploaded.body.asset.sha256, "source_metadata_hash"); assert.equal(sourceMetadata.size_bytes, sourceBytes.length, "source_metadata_size");
    const replay = await postJson(`http://127.0.0.1:${jobsPort}/api/jobs/resume-source`, token, sourceRequest);
    assert.equal(replay.status, 200, "source_replay_status"); assert.equal(replay.body.asset.id, uploaded.body.asset.id, "source_replay_asset");
    const sourceWorkspace = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token); assert.equal(sourceWorkspace.profile.full_name, "Synthetic Candidate", "source_preserves_manual_name"); assert.equal(sourceWorkspace.profile.source_resume_asset_id, uploaded.body.asset.id, "source_profile_pointer"); assert.equal(sourceWorkspace.profile.source_resume_sha256, uploaded.body.asset.sha256, "source_profile_hash");
    assert.equal(sourceWorkspace.profile.source_resume_name, "synthetic-source.txt", "source_profile_name"); assert.equal(sourceWorkspace.profile.source_resume_media_type, "text/plain", "source_profile_media_type"); assert.equal(sourceWorkspace.profile.source_resume_template_status, "text_only", "source_profile_template_status"); const manualProfileAfterSource = structuredClone(sourceWorkspace.profile); for (const key of ["source_resume_asset_id", "source_resume_name", "source_resume_sha256", "source_resume_media_type", "source_resume_template_status", "updated_at_ms"]) { delete manualProfileBeforeSource[key]; delete manualProfileAfterSource[key]; } assert.deepEqual(manualProfileAfterSource, manualProfileBeforeSource, "source_preserves_manual_profile"); assert.deepEqual(sourceWorkspace.preferences, preferencesBeforeSource, "source_preserves_preferences");
    await objectStore.assertStoredBytes(sourceBytes); result.object_store = objectStore.counters(); assert.equal(result.object_store.put, 1, "source_single_put"); passCase("source_resume_readback");
    stage = "source_policy_review";
    await page.goto(`http://127.0.0.1:${portalPort}/jobs/settings`, { waitUntil: "networkidle" }); await page.locator(".track-settings-main").first().click();
    await page.getByRole("heading", { name: "Edit Career Track" }).waitFor({ timeout: OPERATION_MS }); await page.getByRole("button", { name: "Save track" }).click(); await page.getByRole("status").getByText("Career Track updated.", { exact: true }).waitFor({ timeout: OPERATION_MS });
    const sourceBoundWorkspace = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token);
    const sourceAuthority = sourceBoundWorkspace.tracks[0].policy.authority;
    assert.equal(sourceAuthority.review_state, "approved", "source_policy_approved"); assert.equal(sourceAuthority.source_resume_asset_id, uploaded.body.asset.id, "source_policy_asset_binding"); assert.equal(sourceAuthority.source_resume_sha256, uploaded.body.asset.sha256, "source_policy_hash_binding");
    stage = "review_first_reprepare";
    await page.goto(`http://127.0.0.1:${portalPort}/jobs/matches`, { waitUntil: "networkidle" }); await page.locator(".job-row").filter({ hasText: "Synthetic Qualification Employer" }).click({ timeout: OPERATION_MS });
    const [reprepareResponse] = await Promise.all([page.waitForResponse((response) => new URL(response.url()).pathname === "/api/jobs/applications" && response.request().method() === "POST", { timeout: OPERATION_MS }), page.getByRole("button", { name: "Prepare application", exact: true }).click()]);
    const reprepareText = await reprepareResponse.text();
    assert.ok(Buffer.byteLength(reprepareText) <= 1024 * 1024, "reprepare_body_limit");
    const reprepare = { status: reprepareResponse.status(), body: JSON.parse(reprepareText) };
    stage = "review_first_packet_binding";
    assert.deepEqual(JSON.parse(reprepareResponse.request().postData()), { job_id: fixtureResult.job_id, mode: "factual", submission_mode: "review_first" }, "reprepare_browser_authority");
    assert.equal(reprepare.status, 200, "reprepare_status"); assert.equal(reprepare.body.application.id, prepared.body.application.id, "same_application_reprepared"); assert.equal(reprepare.body.application.state, "awaiting_review", "reprepare_awaiting_review"); assert.notEqual(reprepare.body.resume_version.id, prepared.body.resume_version.id, "fresh_resume_version"); assert.equal(reprepare.body.resume_version.mode, "factual", "fresh_factual_mode"); assert.equal(reprepare.body.resume_version.content.provenance.generated_for_job_id, fixtureResult.job_id, "fresh_generated_job_binding"); assert.equal(reprepare.body.resume_version.content.provenance.career_track_policy_authority.source_resume_asset_id, uploaded.body.asset.id, "fresh_resume_source_binding"); assert.equal(reprepare.body.resume_version.content.provenance.career_track_policy_authority.source_resume_sha256, uploaded.body.asset.sha256, "fresh_resume_hash_binding"); assert.equal(reprepare.body.application.receipt.approved_execution, undefined, "fresh_not_approved");
    assert.equal(reprepare.body.resume_version.content.provenance.mode, "factual", "fresh_factual_provenance");
    const oldResume = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/resume-versions/${encodeURIComponent(prepared.body.resume_version.id)}`, token); assert.deepEqual(oldResume, prepared.body.resume_version, "old_resume_immutable_after_source_change");
    const freshResume = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/resume-versions/${encodeURIComponent(reprepare.body.resume_version.id)}`, token); assert.deepEqual(freshResume, reprepare.body.resume_version, "fresh_resume_immutable_reload");
    assert.deepEqual(freshResume.content.employment, prepared.body.resume_version.content.employment, "fresh_resume_preserves_employment");
    assert.deepEqual(freshResume.content.skills, prepared.body.resume_version.content.skills, "fresh_resume_preserves_skills");
    stage = "review_first_queue_eligibility";
    const queueBefore = await packetLedgerSnapshot(psql, database, childHome, accountId); const queueWorkspace = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token); const queuePosting = queueWorkspace.matches.find((posting) => posting.id === fixtureResult.job_id);
    assert.equal(queueWorkspace.applications.length, 1, "reprepare_no_duplicate_application");
    assert.equal(queueWorkspace.applications[0].id, reprepare.body.application.id, "reprepare_workspace_application");
    assert.equal(queueWorkspace.applications[0].job_id, fixtureResult.job_id, "reprepare_workspace_job");
    assert.equal(queueWorkspace.entitlement.plan, "pro", "guarded_fixture_plan");
    assert.equal(queueWorkspace.entitlement.used_packets, 0, "no_packet_consumed_before_approval");
    assert.ok(queuePosting, "queue_posting_present");
    assert.deepEqual(queuePosting.eligibility.hard_failures, [], "queue_no_hard_failures");
    for (const check of ["career_track_policy_current", "role_family_aligned", "location_allowed", "employment_type_allowed", "engagement_type_allowed", "job_recently_verified", "discovery_source_healthy", "ats_certified"]) {
      assert.ok(queuePosting.eligibility.passed_checks.includes(check), `queue_gate_${check}`);
    }
    assert.equal(queuePosting.eligibility.ats_certification.status, "active", "queue_ats_active");
    assert.equal(queuePosting.eligibility.can_queue_local, true, "queue_local_allowed"); assert.equal(queuePosting.eligibility.can_queue_cloud, false, "queue_cloud_disabled");
    stage = "review_first_queue_approval";
    const queued = await postJson(`http://127.0.0.1:${jobsPort}/api/jobs/applications/${encodeURIComponent(reprepare.body.application.id)}/approve`, token, {}); assert.equal(queued.status, 200, "queue_approval_status"); assert.equal(queued.body.application.state, "queued", "queue_state");
    assert.equal(queued.body.metering.newly_metered, true, "queue_metered_once");
    assert.equal(queued.body.metering.included, true, "queue_included_packet");
    assert.equal(queued.body.metering.amount_cents, 0, "queue_no_overage_charge");
    assert.equal(queued.body.metering.used_packets, queueBefore.used_packets + 1, "queue_metered_usage");
    const queuedWorkspace = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token); assert.equal(queuedWorkspace.applications[0].state, "queued", "queued_workspace_state"); const approvalEvidence = queuedWorkspace.applications[0].receipt.approved_execution; assert.equal(approvalEvidence.schema_version, 2, "queued_approval_schema"); assert.equal(approvalEvidence.admission.kind, "review_approval", "queued_approval_kind"); assert.match(approvalEvidence.checksum, /^[a-f0-9]{64}$/, "queued_approval_checksum"); assert.equal(approvalEvidence.packet.applicationId, reprepare.body.application.id, "queued_approval_application_binding"); assert.equal(approvalEvidence.packet.jobId, fixtureResult.job_id, "queued_approval_job_binding"); assert.equal(approvalEvidence.packet.applicationIdentityId, verifiedIdentity.id, "queued_approval_identity_binding"); assert.equal(approvalEvidence.packet.resumeVersionId, reprepare.body.resume_version.id, "queued_approval_resume_binding"); assert.deepEqual(approvalEvidence.packet.resumeContent, freshResume.content, "queued_approval_resume_content"); assert.equal(approvalEvidence.packet.resumeContent.provenance.career_track_policy_authority.source_resume_asset_id, uploaded.body.asset.id, "queued_approval_source_binding"); assert.equal(approvalEvidence.packet.resumeContent.provenance.career_track_policy_authority.source_resume_sha256, uploaded.body.asset.sha256, "queued_approval_source_hash_binding"); assert.deepEqual(queuedWorkspace.auto_submit_authorizations, [], "queued_no_auto_submit_authority"); assert.equal(queuedWorkspace.applications[0].submitted_at_ms, null, "queued_not_submitted"); assert.deepEqual(queuedWorkspace.browser_sessions, [], "queued_no_browser_session"); assert.deepEqual(queuedWorkspace.interventions, [], "queued_no_intervention");
    const queueAfter = await packetLedgerSnapshot(psql, database, childHome, accountId); assert.equal(queueAfter.attempts, queueBefore.attempts + 1, "queue_attempt"); assert.equal(queueAfter.metering, queueBefore.metering + 1, "queue_metering"); assert.equal(queueAfter.used_packets, queueBefore.used_packets + 1, "queue_used_packet"); assert.equal(queueAfter.balance_events, queueBefore.balance_events, "queue_no_balance_event"); assert.equal(queueAfter.commands, queueBefore.commands, "queue_no_dispatch");
    assert.equal(queuedWorkspace.applications.length, 1, "queue_no_duplicate_application");
    assert.equal(queuedWorkspace.applications[0].id, reprepare.body.application.id, "queue_workspace_application");
    assert.equal(queuedWorkspace.applications[0].job_id, fixtureResult.job_id, "queue_workspace_job");
    assert.equal(queuedWorkspace.entitlement.used_packets, queueAfter.used_packets, "queue_workspace_usage");
    assert.equal(queueAfter.included_metering, queueBefore.included_metering + 1, "queue_included_ledger");
    assert.equal(queueAfter.metered_amount_cents, 0, "queue_ledger_zero_charge");
    assert.equal(queueAfter.balance_cents, queueBefore.balance_cents, "queue_ledger_balance_unchanged");
    const repeatApprove = await postJson(`http://127.0.0.1:${jobsPort}/api/jobs/applications/${encodeURIComponent(reprepare.body.application.id)}/approve`, token, {}); assert.equal(repeatApprove.status, 409, "queue_repeat_conflict"); assert.deepEqual(await packetLedgerSnapshot(psql, database, childHome, accountId), queueAfter, "queue_repeat_no_double_debit"); assert.equal(pageErrors, 0, "final_page_errors"); assert.equal(serverErrors, 0, "final_server_errors"); passCase("review_first_queue");
    }
    await withTimeout(context.close(), "context_close", true);
  } finally { await closeInteractive(); }
  stage = "cleanup"; await stopTransientChildren(); await stopChildren();
  if (HUMAN_HOLD) {
    humanResult.binaries = result.binaries; humanResult.fixture = result.fixture;
    humanResult.requests.jobs = result.requests.jobs;
    humanResult.requests.account = result.requests.account;
  } else assert.equal(result.passed, result.tests, "all_cases_passed");
  assert.ok(Number.isInteger(result.requests.jobs) && result.requests.jobs > 0, "jobs_request_count");
  assert.ok(Number.isInteger(result.requests.account) && result.requests.account > 0, "account_request_count");
  checkAbort();
}

let totalTimer;
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(signal, () => requestAbort(`signal_${signal.toLowerCase()}`));
}
try {
  totalTimer = setTimeout(() => requestAbort("timeout_total"), LIMIT_MS);
  await main();
} catch (error) {
  const brokenStage = stage;
  const assertionLabel = error?.code === "ERR_ASSERTION" && typeof error.message === "string"
    ? error.message.split("\n", 1)[0] : "";
  const candidate = /^[a-z0-9_]{1,120}$/.test(assertionLabel)
    ? assertionLabel : error instanceof Error ? error.message : "unknown_failure";
  const code = /^[a-z0-9_:-]{1,120}$/i.test(candidate) ? candidate : "generic_failure";
  if (!result.failed.includes(code)) result.failed.push(code);
  stage = "cleanup";
  const cleanup = await Promise.allSettled([
    closeInteractive(), stopTransientChildren(), stopChildren(),
  ]);
  if (cleanup.some((outcome) => outcome.status === "rejected")) {
    result.failed.push("cleanup_failure");
  }
  process.stdout.write(`${JSON.stringify({ version: 4, passed: 0, stage: brokenStage, failure: code })}\n`);
} finally {
  clearTimeout(totalTimer);
  if (objectStore) {
    result.object_store = objectStore.counters();
    if (HUMAN_HOLD) humanResult.object_store = result.object_store;
    await objectStore.close(); objectStore = undefined;
  }
  if (HUMAN_HOLD && result.failed.length === 0
      && humanResult.passed === HUMAN_CASES.length - 1 && outputRoot) {
    const inventory = await humanCleanupInventory();
    humanResult.engine_ps = inventory.enginePs; humanResult.engine_lsof = inventory.engineLsof;
    humanResult.ports_refused = inventory.portsRefused;
    assert.deepEqual(inventory, { enginePs: 0, engineLsof: 0, portsRefused: 4 },
      "human_cleanup_inventory");
    passHumanCase("cleanup");
    assert.equal(humanResult.passed, HUMAN_CASES.length, "all_human_cases_passed");
    const output = join(outputRoot, "human-browser-api-result.json");
    publishedReportPath = output; checkAbort();
    try {
      await writeFile(output, `${JSON.stringify(humanResult)}\n`, { flag: "wx", mode: 0o600 });
      await chmod(output, 0o600); checkAbort();
    } catch (error) {
      try { unlinkSync(output); } catch (unlinkError) {
        if (unlinkError.code !== "ENOENT") throw unlinkError;
      }
      throw error;
    }
    process.stdout.write(`${JSON.stringify({ version: 1, mode: "human_hold",
      passed: humanResult.passed, cases: humanResult.cases.length, sourceAccepted: false })}\n`);
  } else if (!HUMAN_HOLD && result.failed.length === 0 && result.passed === result.tests && outputRoot) {
    const output = join(outputRoot, "browser-api-result.json");
    publishedReportPath = output; checkAbort();
    try {
      await writeFile(output, `${JSON.stringify(result)}\n`, { flag: "wx", mode: 0o600 });
      await chmod(output, 0o600); checkAbort();
    } catch (error) {
      try { unlinkSync(output); } catch (unlinkError) {
        if (unlinkError.code !== "ENOENT") throw unlinkError;
      }
      throw error;
    }
    process.stdout.write(`${JSON.stringify({ version: 4, passed: result.passed, tests: result.tests })}\n`);
  }
}
if (result.failed.length) process.exitCode = 1;
