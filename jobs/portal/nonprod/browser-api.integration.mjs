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
const result = { version: 1, tests: 1, passed: 0, failed: [], binaries: [], requests: { jobs: 0, account: 0 } };
let outputRoot;
let liveBrowser;
let liveProxy;
let stage = "preflight";

function fail(code) { throw new Error(code); }
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
function child(command, args, env, name) {
  const processHandle = spawn(command, args, { env, stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  processHandle.stderr.on("data", (chunk) => { stderr = `${stderr}${chunk}`.slice(-1024); });
  const completion = new Promise((resolveChild) => {
    processHandle.once("error", () => resolveChild({ code: null, signal: "spawn_error" }));
    processHandle.once("close", (code, signal) => resolveChild({ code, signal }));
  });
  children.push({ name, processHandle, completion, stderr: () => stderr.replace(/[^a-z0-9_:-]/gi, "_").slice(-120) });
  return processHandle;
}
async function stopChildren() {
  const active = children.filter(({ processHandle }) => processHandle.exitCode === null && processHandle.signalCode === null && !processHandle.killed);
  for (const { processHandle } of active) processHandle.kill("SIGTERM");
  const outcomes = await Promise.allSettled(children.map(async ({ name, processHandle, completion, stderr }) => {
    const { code, signal } = await withTimeout(completion, `stop_${name}`);
    result.binaries.push({ pid: processHandle.pid, code, signal });
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
async function seed(psql, database, accountId, email) {
  const sql = `
INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining, email_verified_at)
VALUES ('${accountId}', '${email}', 'fixture-only', 0, now());
UPDATE jobs_public_beta_cohorts SET state = 'closed_to_new', hard_cap = 1
WHERE id = 'public-v1';
INSERT INTO jobs_public_beta_enrollments (cohort_id, account_id, source, admitted_at_ms)
VALUES ('public-v1', '${accountId}', 'admin', 1);
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
  await stat(jobsBinary); await stat(sharedBinary); await stat(join(dist, "index.html"));
  stage = "pg_identity"; await postgresIdentity(psql, database, childHome, root);
  const jobsPort = await freePort(); const accountPort = await freePort();
  const secret = randomBytes(32).toString("base64url");
  const dataKey = randomBytes(32).toString("base64");
  const purgePolicy = fixturePurgeSigningPolicy();
  const accountId = `ui_fixture_${randomUUID()}`;
  const email = `ui-fixture-${randomUUID()}@example.test`;
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
  await seed(psql, database, accountId, email);
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
    assert.ok(result.requests.jobs >= 2, "workspace_requests"); assert.ok(result.requests.account >= 1, "account_request");
    assert.equal(pageErrors, 0, "portal_page_errors"); assert.equal(serverErrors, 0, "portal_server_errors");
    const workspace = await getJson(`http://127.0.0.1:${jobsPort}/api/jobs/workspace`, token);
    assert.equal(workspace.profile.full_name, "Synthetic Candidate", "profile_persisted");
    assert.equal(workspace.matches.length, 0, "empty_matches");
    await context.close();
  } finally { await browser.close(); liveBrowser = undefined; await new Promise((resolveClose) => proxy.close(resolveClose)); liveProxy = undefined; }
  stage = "cleanup"; await stopChildren();
  result.passed = 1;
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
  process.stdout.write(`${JSON.stringify({ version: 1, passed: 0, stage: brokenStage, failure: code })}\n`);
} finally {
  clearTimeout(totalTimer);
  if (result.failed.length === 0 && result.passed === result.tests && outputRoot) {
    const output = join(outputRoot, "browser-api-result.json");
    await writeFile(output, `${JSON.stringify(result)}\n`, { flag: "wx", mode: 0o600 }); await chmod(output, 0o600);
    process.stdout.write(`${JSON.stringify({ version: 1, passed: result.passed, tests: result.tests })}\n`);
  }
}
if (result.failed.length) process.exitCode = 1;
