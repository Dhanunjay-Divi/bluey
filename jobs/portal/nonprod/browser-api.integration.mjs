/*
 * Private non-production qualification only.  The caller supplies a freshly
 * created PostgreSQL cluster and an owned root; this script never falls back
 * to a developer database, browser profile, or external endpoint.
 */
import assert from "node:assert/strict";
import { createHmac, generateKeyPairSync, randomBytes, randomUUID } from "node:crypto";
import { createServer } from "node:http";
import { request as httpRequest } from "node:http";
import { lstat, mkdir, readFile, realpath, stat, writeFile, chmod } from "node:fs/promises";
import { extname, join, normalize, relative, resolve } from "node:path";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { chromium } from "playwright";

const LIMIT_MS = 180_000;
const OPERATION_MS = 10_000;
const children = [];
const EXPECTED_CASES = ["profile_reload", "onboarding_complete", "preference_track_reload", "auth_isolation", "effect_availability", "packet_review", "approval_fenced"];
const result = { version: 3, cases: [], tests: EXPECTED_CASES.length, passed: 0, failed: [], binaries: [], fixture: null, requests: { jobs: 0, account: 0 } };
let outputRoot;
let liveBrowser;
let liveProxy;
let stage = "preflight";

function fail(code) { throw new Error(code); }
function passCase(name) {
  assert.equal(EXPECTED_CASES[result.cases.length], name, "case_order");
  result.cases.push(name); result.passed += 1;
  process.stdout.write(`${JSON.stringify({ event: "browser_case_passed", case: name })}\n`);
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
async function withTimeout(promise, label) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`timeout_${label}`)), OPERATION_MS); })]); }
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
  const active = children.filter(({ processHandle }) => processHandle.exitCode === null && processHandle.signalCode === null && !processHandle.killed);
  for (const { processHandle } of active) processHandle.kill("SIGTERM");
  const outcomes = await Promise.allSettled(children.map(async ({ name, fixture, processHandle, completion, stderr }) => {
    const { code, signal } = await withTimeout(completion, `stop_${name}`);
    if (fixture) result.fixture = { pid: processHandle.pid, code, signal };
    else result.binaries.push({ pid: processHandle.pid, code, signal });
    if (code !== 0 || signal !== null) fail(`${name}_exit_${stderr() || "unexpected"}`);
  }));
  const rejected = outcomes.find((outcome) => outcome.status === "rejected");
  if (rejected) fail("cleanup_child_failure");
  return outcomes;
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
  const psqlChild = spawn(psql, ["-X", "-v", "ON_ERROR_STOP=1", "-q"], { env, stdio: ["pipe", "ignore", "ignore"] });
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
  const check = spawn(psql, ["-X", "-A", "-t", "-q", "-c", query], { env: pgEnv(database, home), stdio: ["ignore", "pipe", "ignore"] });
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
  const query = "SELECT json_build_object('attempts',(SELECT count(*) FROM jobs_attempt_reservations WHERE account_id=:'fixture_account'),'metering',(SELECT count(*) FROM jobs_packet_metering WHERE account_id=:'fixture_account'),'commands',(SELECT count(*) FROM jobs_workflow_commands WHERE account_id=:'fixture_account'),'balance_events',(SELECT count(*) FROM balance_ledger_entries WHERE account_id=:'fixture_account'),'balance_cents',(SELECT balance_cents FROM accounts WHERE id=:'fixture_account'),'used_packets',coalesce((SELECT used_packets FROM jobs_entitlements WHERE account_id=:'fixture_account'),0))::text";
  const command = spawn(psql, ["-X", "-A", "-t", "-q", "-v", "ON_ERROR_STOP=1", "-v", `fixture_account=${accountId}`], { env: pgEnv(database, home), stdio: ["pipe", "pipe", "ignore"] });
  let stdout = ""; command.stdout.on("data", (chunk) => { stdout += chunk; if (stdout.length > 4096) command.kill("SIGTERM"); });
  const completion = once(command, "close"); command.stdin.end(`${query};\n`);
  const [code, signal] = await withTimeout(completion, "ledger_snapshot");
  assert.equal(code, 0, "ledger_snapshot_exit"); assert.equal(signal, null, "ledger_snapshot_signal");
  return JSON.parse(stdout.trim());
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
  stage = "preflight";
  const requestedRoot = required("BLUEY_TEST_RUN_ROOT");
  const rootInfo = await lstat(requestedRoot);
  if (rootInfo.isSymbolicLink() || !rootInfo.isDirectory() || rootInfo.uid !== process.getuid() || (rootInfo.mode & 0o777) !== 0o700) fail("test_root_identity");
  const root = await realpath(requestedRoot);
  if (root !== requestedRoot || !(/^\/private\/tmp\/bluey-test-runs-/.test(root) || /^\/tmp\/bluey-test-runs-/.test(root))) fail("test_root_prefix");
  if (process.env.BLUEY_NONPROD_ROOT && await realpath(process.env.BLUEY_NONPROD_ROOT) !== root) fail("nonprod_root_mismatch");
  const database = onlyLoopbackPostgres(required("BLUEY_TEST_POSTGRES_URL"));
  const target = await ownedPath(root, required("CARGO_TARGET_DIR"), "cargo_target_dir");
  const dist = await ownedPath(root, required("BLUEY_PORTAL_DIST"), "portal_dist");
  await ownedPath(root, required("PLAYWRIGHT_BROWSERS_PATH"), "playwright_browsers");
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
    BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED: "0", RUST_LOG: "warn",
  };
  stage = "start_jobs_api";
  child(jobsBinary, [], { ...common, BLUEY_JOBS_API_HOST: "127.0.0.1", BLUEY_JOBS_API_PORT: String(jobsPort) }, "bluey-jobs-api");
  await waitFor(`http://127.0.0.1:${jobsPort}/health`);
  stage = "start_account_api";
  child(sharedBinary, [], { ...common, BLUEY_API_HOST: "127.0.0.1", BLUEY_PORT: String(accountPort) }, "bluey-server");
  await waitFor(`http://127.0.0.1:${accountPort}/health`);
  stage = "seed";
  await seed(psql, database, accountId, email, isolatedAccountId, isolatedEmail);
  const token = fixtureToken(accountId, secret);
  const proxy = proxyServer(dist, jobsPort, accountPort); liveProxy = proxy; const portalPort = await listen(proxy);
  const browser = await withTimeout(chromium.launch({ headless: true }), "browser_launch"); liveBrowser = browser;
  try {
    const context = await browser.newContext();
    await context.route("**/*", (route) => new URL(route.request().url()).origin === `http://127.0.0.1:${portalPort}` ? route.continue() : route.abort());
    await context.addInitScript(([access, persistence]) => { localStorage.setItem("bluey_access_token", access); localStorage.setItem("bluey_auth_persistence", persistence); }, [token, "local"]);
    const page = await context.newPage();
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
    const isolatedToken = fixtureToken(isolatedAccountId, secret);
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
    await writeFile(fixtureRequest, `${JSON.stringify({ version: 1, account_id: accountId, track_id: boundWorkspace.tracks[0].id })}\n`, { flag: "wx", mode: 0o600 });
    const fixtureProcess = child(fixtureBinary, [], { PATH: process.env.PATH, HOME: childHome, TMPDIR: childTmp, TMP: childTmp, TEMP: childTmp, SQLITE_TMPDIR: childTmp, BLUEY_TEST_RUN_ROOT: root, BLUEY_TEST_POSTGRES_URL: required("BLUEY_TEST_POSTGRES_URL"), BLUEY_JOBS_DATA_KEY: dataKey }, "packet-fixture", true);
    const fixtureOutcome = await withTimeout(children.find(({ processHandle }) => processHandle === fixtureProcess).completion, "packet_fixture");
    result.fixture = { pid: fixtureProcess.pid, code: fixtureOutcome.code, signal: fixtureOutcome.signal };
    if (fixtureOutcome.code !== 0) fail(`fixture_${children.find(({ processHandle }) => processHandle === fixtureProcess).fixtureFailure()}`);
    assert.equal(fixtureOutcome.code, 0, "packet_fixture_exit"); assert.equal(fixtureOutcome.signal, null, "packet_fixture_signal");
    const fixtureResult = await ownedJson(join(root, "browser-packet-fixture-result.json"), root, { version: "number", job_id: "string" });
    assert.equal(fixtureResult.version, 1, "packet_fixture_version"); assert.ok(fixtureResult.job_id.length > 0, "packet_fixture_job");
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
    assert.deepEqual({ attempts: ledgerBefore.attempts, metering: ledgerBefore.metering, commands: ledgerBefore.commands, balance_events: ledgerBefore.balance_events, used_packets: ledgerBefore.used_packets }, { attempts: 0, metering: 0, commands: 0, balance_events: 0, used_packets: 0 }, "ledger_zero_before_approval");
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
    await context.close();
  } finally { await browser.close(); liveBrowser = undefined; await new Promise((resolveClose) => proxy.close(resolveClose)); liveProxy = undefined; }
  stage = "cleanup"; await stopChildren();
  assert.equal(result.passed, result.tests, "all_cases_passed");
  assert.ok(Number.isInteger(result.requests.jobs) && result.requests.jobs > 0, "jobs_request_count");
  assert.ok(Number.isInteger(result.requests.account) && result.requests.account > 0, "account_request_count");
}

let totalTimer;
try {
  await Promise.race([main(), new Promise((_, reject) => { totalTimer = setTimeout(() => reject(new Error("timeout_total")), LIMIT_MS); })]);
} catch (error) {
  const brokenStage = stage;
  const candidate = error instanceof Error ? error.message : "unknown_failure";
  const code = /^[a-z0-9_:-]{1,120}$/i.test(candidate) ? candidate : "generic_failure";
  result.failed.push(code);
  if (liveBrowser) await liveBrowser.close().catch(() => {});
  if (liveProxy) await new Promise((resolveClose) => liveProxy.close(resolveClose));
  try { stage = "cleanup"; await stopChildren(); } catch { result.failed.push("cleanup_failure"); }
  process.stdout.write(`${JSON.stringify({ version: 3, passed: 0, stage: brokenStage, failure: code })}\n`);
} finally {
  clearTimeout(totalTimer);
  if (result.failed.length === 0 && result.passed === result.tests && outputRoot) {
    const output = join(outputRoot, "browser-api-result.json");
    await writeFile(output, `${JSON.stringify(result)}\n`, { flag: "wx", mode: 0o600 }); await chmod(output, 0o600);
    process.stdout.write(`${JSON.stringify({ version: 3, passed: result.passed, tests: result.tests })}\n`);
  }
}
if (result.failed.length) process.exitCode = 1;
