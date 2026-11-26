import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { once } from "node:events";
import { createServer } from "node:http";
import test from "node:test";
import { authenticateLocalQaLinuxApi, bindLocalQaLinuxImportedCandidate } from "./local-qa-linux-api-auth.mjs";
import { validateLocalQaLinuxApiEnvironment } from "./local-qa-linux-api-process.mjs";
import { parseLocalQaLinuxC2cNativeSpec, runLocalQaLinuxBrowserSamplePrepareNative,
  runLocalQaLinuxBrowserSampleResumeNative, runLocalQaLinuxBrowserSignupNative,
  runLocalQaLinuxC2cNative } from "./local-qa-linux-c2c-native.mjs";
import { expectedLocalQaLinuxC2cPgCapsule } from "./local-qa-linux-c2c-pg-capsule.mjs";
import { parseLocalQaLinuxC2cChromiumInventory } from "./local-qa-linux-c2c-chromium.mjs";
import { LOCAL_QA_LINUX_EFFECT_FLAGS } from "./local-qa-linux-c2c-session.mjs";
import {
  buildLocalQaLinuxApiEnvironment, createLocalQaLinuxApiTransport, createLocalQaLinuxOwnedApiSession,
  validateLocalQaLinuxApiRequest, validateLocalQaLinuxOwnedApiBinding,
} from "./local-qa-linux-owned-api-session.mjs";

const root = "/tmp/bluey-test-runs-a1b2c3d4e5f6/run.1795250653.12345.32615190360";
const pgCapsule = expectedLocalQaLinuxC2cPgCapsule("1".repeat(32));
const unitName = `bluey-jobs-qa-${pgCapsule.run}.service`;
const binding = {
  root, apiPort: 43107, postgresPort: 43108, smtpPort: 43109, objectPort: 43110,
  unitCgroup: `/system.slice/${unitName}`, networkNamespace: "net:[4026532555]",
  assertOwnedUnit: async () => true,
  buildBinding: { version: 1, qualificationOnly: true, artifactPath: "/run/admitted/bluey-api",
    artifactSha256: "a".repeat(64), manifestFile: "/run/admitted/build.json", manifestSha256: "b".repeat(64),
    sourceSha256: "c".repeat(64) },
  pgCapsule,
  pidfdToolchain: { python: { path: "/usr/bin/python3.12", sha256: "e".repeat(64) },
    helper: { path: "/run/admitted/local-qa-linux-postmaster-pidfd.py", sha256: "f".repeat(64) } },
};
const operatorId = "11111111-1111-4111-8111-111111111111";
const candidateId = "22222222-2222-4222-8222-222222222222";
const sessionConfig = { operatorEmail: "operator@bluey-qa.test", candidateEmail: "candidate@bluey-qa.test" };
const sessionSecrets = { operatorPassword: `Qa-${"1".repeat(48)}`, candidatePassword: `Qa-${"2".repeat(48)}` };
const taxonomy = { "x-bluey-jobs-taxonomy-version": "jobs-v1", "x-bluey-jobs-taxonomy-sha256": "a".repeat(64) };
const token = "synthetic.candidate.test";
const draft = { job_id: "synthetic-job-1", mode: "factual", submission_mode: "review_first" };

function replyAccount(email) {
  const admin = email === sessionConfig.operatorEmail;
  return { access_token: admin ? "synthetic.operator.test" : token, refresh_token: "synthetic-refresh-fixture", expires_in: 3_600,
    account: { balance_cents: 0, email, id: admin ? operatorId : candidateId, is_admin: admin, is_temporary: false,
      temporary_expires_at: null, trial_seconds_remaining: 0 } };
}

async function loopback(t, callback, allowPrepareCommunications = false) {
  const requests = [];
  const server = createServer(async (req, res) => {
    try {
      const chunks = [];
      for await (const chunk of req) chunks.push(chunk);
      const raw = Buffer.concat(chunks).toString("utf8");
      const entry = { method: req.method, path: req.url, auth: req.headers.authorization,
        body: raw ? JSON.parse(raw) : undefined, headers: req.headers };
      requests.push(entry);
      const { status = 200, body = {} } = callback(entry);
      res.writeHead(status, { "content-type": "application/json" });
      res.end(JSON.stringify(body));
    } catch { res.destroy(); }
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  t.after(async () => {
    await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  });
  let proofs = 0;
  const call = createLocalQaLinuxApiTransport({ apiPort: server.address().port, allowPrepareCommunications,
    async assertCurrent() { proofs++; return true; } });
  return { call, requests, proofs: () => proofs };
}

function router(entry) {
  if (["/auth/signup/confirm", "/auth/login"].includes(entry.path)) return { body: replyAccount(entry.body.email) };
  if (entry.path === "/admin/jobs/public-beta") return { body: { revision: 7 } };
  if (entry.path === "/api/jobs/beta-access") return { body: { schemaVersion: 1, access: "admitted", reason: "admitted" } };
  if (entry.path === "/api/jobs/workspace") return { body: { profile: { email: sessionConfig.candidateEmail } } };
  return { body: { ok: true } };
}

test("imported candidate binds a private login, allows one factual Prepare, and reads only empty communications", async t => {
  const path = "/api/jobs/communication-actions?limit=100";
  const applicationId = "33333333-3333-4333-8333-333333333333";
  const { call, requests } = await loopback(t, entry => {
    if (entry.path.startsWith(path)) return { body: [] };
    if (entry.path === "/api/jobs/applications") return { body: { application: { id: applicationId, job_id: draft.job_id } } };
    return router(entry);
  }, true);
  await assert.rejects(bindLocalQaLinuxImportedCandidate({ call, candidatePassword: sessionSecrets.candidatePassword,
    accountId: operatorId }), /auth_refused/);
  const bound = await bindLocalQaLinuxImportedCandidate({ call, candidatePassword: sessionSecrets.candidatePassword, accountId: candidateId });
  await assert.rejects(bound.prepareDraft(draft.job_id));
  await assert.rejects(bound.readEmptyCommunicationActions());
  await bound.saveOnboarding({ profile: {}, preferences: {}, track: {} }, taxonomy);
  await bound.prepareDraft(draft.job_id);
  await assert.rejects(bound.prepareDraft(draft.job_id));
  assert.deepEqual(await bound.readEmptyCommunicationActions(), []);
  assert.deepEqual(await bound.readEmptyCommunicationActions(applicationId), []);
  await assert.rejects(bound.readEmptyCommunicationActions(candidateId));
  const writes = requests.filter(row => row.path === "/api/jobs/applications");
  assert.equal(writes.length, 1);
  assert.deepEqual(writes[0].body, draft);
  assert.equal(writes[0].auth, `Bearer ${token}`);
  assert.throws(() => validateLocalQaLinuxApiRequest(path, { token }));
  await assert.rejects(call(`${path}&application_id=${candidateId}&ignored=1`, { token }));
  await assert.rejects(call(path, { token, method: "POST", body: {} }));
  await assert.rejects(runLocalQaLinuxBrowserSamplePrepareNative({ specFile: "invalid", specSha256: "invalid" }));
});

test("opt-in imported candidate resume reader binds one prepared factual resume to its private login", async t => {
  const applicationId = "33333333-3333-4333-8333-333333333333";
  const resumeId = "44444444-4444-4444-8444-444444444444";
  const resume = Object.freeze({ id: resumeId, job_id: draft.job_id, mode: "factual", content: {} });
  const callbackCalls = [];
  const { call, requests } = await loopback(t, entry => {
    if (entry.path === "/api/jobs/applications") {
      return { body: { application: { id: applicationId, job_id: draft.job_id, resume_version_id: resumeId }, resume_version: resume } };
    }
    return router(entry);
  });
  const bound = await bindLocalQaLinuxImportedCandidate({
    call,
    candidatePassword: sessionSecrets.candidatePassword,
    accountId: candidateId,
    async readBoundResume(id, ownerLoginToken) {
      callbackCalls.push({ id, ownerLoginToken });
      assert.equal(id, resumeId);
      assert.equal(ownerLoginToken, token);
      return resume;
    },
  });
  assert.deepEqual(bound.account, {
    id: candidateId,
    email: sessionConfig.candidateEmail,
    is_admin: false,
    is_temporary: false,
  });
  assert(Object.isFrozen(bound.account));
  assert.equal(Object.hasOwn(bound, "token"), false);
  await assert.rejects(bound.readResume(resumeId));
  await bound.saveOnboarding({ profile: {}, preferences: {}, track: {} }, taxonomy);
  await bound.prepareDraft(draft.job_id);
  await assert.rejects(bound.readResume(applicationId));
  assert.deepEqual(await bound.readResume(resumeId), resume);
  assert.deepEqual(callbackCalls, [{ id: resumeId, ownerLoginToken: token }]);
  assert.equal(requests.filter(row => row.path === "/api/jobs/applications").length, 1);
});

test("an owned API session refuses colliding ports, arbitrary executable callbacks, and unbound launch", async () => {
  let calls = 0;
  const good = { ...binding, async assertOwnedUnit() { calls++; return true; } };
  const bound = validateLocalQaLinuxOwnedApiBinding(good);
  assert(Object.isFrozen(bound.pgBinding.pidfdToolchain));
  for (const change of [{ smtpPort: binding.apiPort }, { objectPort: 5432 }, { buildBinding: { ...binding.buildBinding, qualificationOnly: false } },
    { launch: async () => ({}) }, { root: "/tmp" }]) assert.throws(() => validateLocalQaLinuxOwnedApiBinding({ ...good, ...change }));
  const controller = createLocalQaLinuxOwnedApiSession(good);
  await assert.rejects(controller.startOwnedSession({ root, stateDirectory: `${root}/bluey-jobs-qa-draft`, signal: undefined }));
  assert.equal(calls, 0);
});

test("a native actor can consume only the digest-bound closed spec for its sealed Linux unit", async () => {
  const { assertOwnedUnit: _, ...sealed } = binding;
  const spec = {
    version: 1, sourceTree: "1".repeat(40), sourceInventorySha256: sealed.buildBinding.sourceSha256, binding: sealed,
    fixture: { artifactPath: `${root}/fixtures/jobs-nonprod-browser-fixture`, artifactSha256: "2".repeat(64) },
    unit: { unitName, bootId: "3".repeat(32), invocationId: "4".repeat(32), mainPid: 12345,
      systemctl: { path: "/usr/bin/systemctl", sha256: "5".repeat(64) } },
    browser: { port: 43111, webDirectory: "/run/admitted/portal", closureRoot: "/run/admitted/browser-toolchain",
      inventoryFile: "/run/admitted/browser-inventory.json", inventorySha256: "6".repeat(64) },
  };
  const encode = value => Buffer.from(JSON.stringify(value) + "\n");
  const digest = bytes => createHash("sha256").update(bytes).digest("hex");
  const bytes = encode(spec);
  assert.deepEqual(parseLocalQaLinuxC2cNativeSpec(bytes, digest(bytes)), spec);
  assert.throws(() => parseLocalQaLinuxC2cNativeSpec(bytes, "0".repeat(64)));
  for (const drift of [{ sourceInventorySha256: "7".repeat(64) },
    { browser: { ...spec.browser, closureRoot: `${root}/mutable` } },
    { browser: { ...spec.browser, port: sealed.apiPort } }, { credentials: "out-of-scope" },
    { unit: { ...spec.unit, unitName: "unrelated.service" } },
    { binding: { ...sealed, pgCapsule: expectedLocalQaLinuxC2cPgCapsule("2".repeat(32)) } },
    { binding: { ...sealed, toolchain: sealed.pgCapsule.tools } },
    { fixture: { ...spec.fixture, artifactPath: "/run/unbound-fixture" } }]) {
    const wrong = encode({ ...spec, ...drift });
    assert.throws(() => parseLocalQaLinuxC2cNativeSpec(wrong, digest(wrong)));
  }
  const duplicate = Buffer.concat([Buffer.from('{"version":1,'), bytes.subarray(1)]);
  assert.throws(() => parseLocalQaLinuxC2cNativeSpec(duplicate, digest(duplicate)));
  await assert.rejects(runLocalQaLinuxC2cNative({ specFile: `${root}/.local-qa-linux-c2c-native.json`,
    specSha256: digest(bytes) }), /c2c_native_refused/);
});

test("native Chromium inventory admits only complete immutable tool roots and exact entry roles", () => {
  const chrome = "receipt-prep-browser/chromium/chrome", node = "receipt-journey-node/toolchain/bin/node";
  const driver = "receipt-prep-browser/node_modules/playwright-core";
  const paths = [chrome, node, `${driver}/index.mjs`, `${driver}/package.json`, `${driver}/browsers.json`].sort();
  const files = paths.map(path => ({ path, sha256: "8".repeat(64), size: 4,
    mode: path === chrome || path === node ? 0o555 : 0o444 }));
  const encode = value => Buffer.from(JSON.stringify(value) + "\n");
  const value = { version: 1, files };
  const parsed = parseLocalQaLinuxC2cChromiumInventory(encode(value));
  assert.deepEqual(parsed, value);
  assert(Object.isFrozen(parsed.files[0]));
  for (const corrupt of [
    { ...value, executablePath: "/usr/bin/chromium" },
    { ...value, files: files.filter(entry => entry.path !== node) },
    { ...value, files: [files[0], ...files] },
    { ...value, files: files.map(row => row.path === chrome ? { ...row, mode: 0o755 } : row) },
    { ...value, files: files.map(row => row.path === node ? { ...row, sha256: `${row.sha256}\n` } : row) },
    { ...value, files: [...files, { ...files[0], path: `${driver}/../foreign.mjs` }].sort((a, b) =>
      a.path < b.path ? -1 : a.path > b.path ? 1 : 0) },
  ]) assert.throws(() => parseLocalQaLinuxC2cChromiumInventory(encode(corrupt)), /CHROMIUM_REFUSED/);
});

test("the composed API environment has only local dependencies and all effectful Jobs flags off", () => {
  const stateDirectory = `${root}/bluey-jobs-qa-draft`;
  const { apiPort, postgresPort, objectPort, smtpPort } = binding;
  const env = buildLocalQaLinuxApiEnvironment({
    stateDirectory, root, apiPort, config: { ...sessionConfig, keyId: "synthetic-signing-fixture",
      minimumBuild: "runner-630.0", publicKey: "synthetic-public-fixture" },
    sessionSecrets: { purgeSeed: "synthetic-signing-fixture" },
    secrets: { jwtSecret: "synthetic-jwt-fixture", jobsDataKey: "synthetic-data-fixture" },
    pg: { databaseUrl: `postgresql://bluey_qa_local:${"a".repeat(64)}@127.0.0.1:${postgresPort}/bluey_jobs_qa?sslmode=disable` },
    store: { env: { BLUEY_OBJECT_ENDPOINT_URL: `http://127.0.0.1:${objectPort}`, BLUEY_OBJECT_BUCKET: `qa-${"1".repeat(32)}`,
      BLUEY_OBJECT_KEY_PREFIX: `qa-${"2".repeat(32)}`, BLUEY_OBJECT_ACCESS_KEY_ID: "synthetic-store-fixture",
      BLUEY_OBJECT_SECRET_ACCESS_KEY: "synthetic-store-fixture", BLUEY_OBJECT_REGION: "us-east-1", BLUEY_OBJECT_MAX_BYTES: "1048576" } },
    smtp: { env: { BLUEY_MAIL_TRANSPORT: "smtp", BLUEY_SMTP_HOST: "127.0.0.1", BLUEY_SMTP_PORT: String(smtpPort),
      BLUEY_SMTP_STARTTLS: "false", BLUEY_SMTP_FROM: "Bluey Local QA <no-reply@bluey-qa.test>", BLUEY_DEV_LOG_AUTH_LINKS: "0" } },
  });
  assert.deepEqual(validateLocalQaLinuxApiEnvironment(env, root, stateDirectory, apiPort), env);
  assert(LOCAL_QA_LINUX_EFFECT_FLAGS.every(flag => env[flag] === "0"));
  for (const drift of [{ BLUEY_JOBS_MODEL_GENERATION_ENABLED: "1" }, { LD_PRELOAD: "/tmp/helper.so" },
    { BLUEY_SMTP_HOST: "192.0.2.12" }, { BLUEY_OBJECT_ENDPOINT_URL: "https://example.invalid" },
    { BLUEY_OBJECT_ENDPOINT_URL: `http://127.0.0.1:${apiPort}` }, { BLUEY_API_HOST: "0.0.0.0" },
    { BLUEY_DATABASE_URL: env.BLUEY_DATABASE_URL.replace("127.0.0.1", "192.0.2.13") }]) {
    assert.throws(() => validateLocalQaLinuxApiEnvironment({ ...env, ...drift }, root, stateDirectory, apiPort));
  }
});

test("synthetic loopback signup uses separate accounts, local OTP routing, and candidate-only Prepare", async t => {
  const { call, requests, proofs } = await loopback(t, router);
  const switches = [];
  let codes = 0;
  const auth = await authenticateLocalQaLinuxApi({ mode: "bootstrap", sessionConfig, sessionSecrets, call,
    async takeSignupCode(kind) { assert.equal(kind, "signup"); codes++; return "638402"; },
    async switchSignupRecipient(email) { switches.push(email); return true; },
  });
  assert.equal(auth.operatorId, operatorId);
  assert.equal(auth.candidateId, candidateId);
  assert.equal(codes, 2);
  assert.deepEqual(switches, [sessionConfig.candidateEmail]);
  await auth.saveOnboarding({ profile: {} }, taxonomy);
  await auth.prepareDraft(draft);
  await assert.rejects(auth.prepareDraft({ ...draft, submission_mode: "auto_submit" }));
  assert.deepEqual(requests.map(({ method, path }) => `${method} ${path}`), [
    "POST /auth/signup/start", "POST /auth/signup/confirm", "GET /admin/jobs/public-beta", "PUT /admin/jobs/public-beta",
    "POST /auth/signup/start", "POST /auth/signup/confirm", "GET /api/jobs/beta-access", "GET /api/jobs/workspace",
    "POST /api/jobs/onboarding/complete", "POST /api/jobs/applications",
  ]);
  assert.equal(requests[3].body.expectedRevision, 7);
  assert.equal(requests[3].auth, "Bearer synthetic.operator.test");
  assert.deepEqual(requests[9].body, draft);
  assert.equal(requests[9].auth, `Bearer ${token}`);
  assert.equal(requests[8].headers["x-bluey-jobs-taxonomy-version"], "jobs-v1");
  assert.equal(proofs(), requests.length * 2);
});

test("restart checks exact candidate receipt IDs and refuses mutations without issuing requests", async t => {
  const { call, requests } = await loopback(t, router);
  const auth = await authenticateLocalQaLinuxApi({ mode: "restart", sessionConfig, sessionSecrets, call,
    expectedIds: { operatorId, candidateId } });
  await assert.rejects(auth.saveOnboarding({ profile: {} }, taxonomy));
  await assert.rejects(auth.prepareDraft(draft));
  await assert.rejects(auth.readResumeSource());
  assert.deepEqual(requests.map(({ path }) => path), [
    "/auth/login", "/auth/login", "/api/jobs/beta-access", "/api/jobs/workspace",
  ]);
  await assert.rejects(authenticateLocalQaLinuxApi({ mode: "restart", sessionConfig, sessionSecrets, call,
    expectedIds: { operatorId: candidateId, candidateId: operatorId } }));
  assert.deepEqual(requests.slice(4).map(({ path }) => path), ["/auth/login", "/auth/login"]);
});

test("both fresh browser entries refuse malformed unowned inputs", async () => {
  for (const entry of [runLocalQaLinuxBrowserSignupNative, runLocalQaLinuxBrowserSampleResumeNative]) {
    await assert.rejects(entry({ specFile: null, specSha256: null }), /local_qa_linux_c2c_native_refused/);
  }
});

test("browser signup bootstrap re-reads candidate identity before returning the authenticated workspace", async t => {
  const sampleAsset = { id: "9ed9cb09-328d-40b7-8263-3d393496cc35" };
  const { call, requests } = await loopback(t, incoming => incoming.path === "/api/jobs/resume-source"
    ? { body: sampleAsset } : router(incoming));
  let browserCalls = 0;
  let operatorCodes = 0;
  const options = { mode: "bootstrap", sessionConfig, sessionSecrets, call,
    async takeSignupCode() { operatorCodes++; return "638402"; },
    async switchSignupRecipient(email) { return email === sessionConfig.candidateEmail; },
    async signupCandidateInBrowser(password) {
      assert.equal(password, sessionSecrets.candidatePassword);
      assert.equal(requests.at(-1).path, "/admin/jobs/public-beta");
      browserCalls++;
      return { accountId: candidateId };
    },
  };
  const auth = await authenticateLocalQaLinuxApi(options);
  assert.equal(auth.candidateId, candidateId);
  assert.equal(browserCalls, 1);
  assert.equal(operatorCodes, 1);
  assert.deepEqual(requests.map(({ path }) => path), [
    "/auth/signup/start", "/auth/signup/confirm", "/admin/jobs/public-beta", "/admin/jobs/public-beta",
    "/auth/login", "/api/jobs/beta-access", "/api/jobs/workspace",
  ]);
  assert.equal(requests[4].body.email, sessionConfig.candidateEmail);
  assert.equal(requests[6].auth, `Bearer ${token}`);
  assert.deepEqual(await auth.readResumeSource(), sampleAsset);
  assert.equal(requests[7].path, "/api/jobs/resume-source");
  assert.equal(requests[7].auth, `Bearer ${token}`);

  const before = requests.length;
  await assert.rejects(authenticateLocalQaLinuxApi({ ...options, async signupCandidateInBrowser() {
    return { accountId: operatorId };
  } }), /local_qa_linux_api_auth_refused/);
  assert.deepEqual(requests.slice(before).map(({ path }) => path), [
    "/auth/signup/start", "/auth/signup/confirm", "/admin/jobs/public-beta", "/admin/jobs/public-beta", "/auth/login",
  ]);
  const after = requests.length;
  await assert.rejects(authenticateLocalQaLinuxApi({ ...options, mode: "restart", expectedIds: { candidateId, operatorId } }),
    /local_qa_linux_api_auth_refused/);
  assert.equal(requests.length, after);
});

test("browser bootstrap commit is bound once to independently admitted synthetic accounts", async t => {
  const { call, requests } = await loopback(t, router);
  const commits = [];
  let browserBinds = 0;
  const auth = await authenticateLocalQaLinuxApi({
    mode: "bootstrap",
    sessionConfig,
    sessionSecrets,
    call,
    async takeSignupCode() { return "638402"; },
    async switchSignupRecipient(email) { return email === sessionConfig.candidateEmail; },
    async readPreparedResume() { return { id: "44444444-4444-4444-8444-444444444444", job_id: draft.job_id, mode: "factual" }; },
    async commitBrowserBootstrap(frame) {
      assert.deepEqual(frame, { operatorId, candidateId });
      assert(Object.isFrozen(frame));
      assert.equal(Object.hasOwn(frame, "token"), false);
      commits.push(frame);
      return true;
    },
    async signupCandidateInBrowser(password, bindBootstrap) {
      assert.equal(password, sessionSecrets.candidatePassword);
      assert.equal(typeof bindBootstrap, "function");
      browserBinds++;
      assert.equal(await bindBootstrap(candidateId), true);
      return { accountId: candidateId };
    },
  });
  assert.equal(auth.operatorId, operatorId);
  assert.equal(auth.candidateId, candidateId);
  assert.equal(browserBinds, 1);
  assert.deepEqual(commits, [{ operatorId, candidateId }]);
  assert.equal(requests.filter(row => row.path === "/auth/login" && row.body.email === sessionConfig.candidateEmail).length, 1);
  assert.equal(requests.filter(row => row.path === "/api/jobs/beta-access" && row.auth === `Bearer ${token}`).length, 2);
});

test("browser bootstrap commit rejects missing, duplicate, failed, and drifted bindings", async t => {
  const base = async ({ commitBrowserBootstrap, signupCandidateInBrowser }) => {
    const { call } = await loopback(t, router);
    return authenticateLocalQaLinuxApi({
      mode: "bootstrap",
      sessionConfig,
      sessionSecrets,
      call,
      async takeSignupCode() { return "638402"; },
      async switchSignupRecipient(email) { return email === sessionConfig.candidateEmail; },
      async readPreparedResume() { return { id: "44444444-4444-4444-8444-444444444444", job_id: draft.job_id, mode: "factual" }; },
      commitBrowserBootstrap,
      signupCandidateInBrowser,
    });
  };
  await assert.rejects(base({
    async commitBrowserBootstrap() { return true; },
    async signupCandidateInBrowser() { return { accountId: candidateId }; },
  }), /auth_refused/);
  await assert.rejects(base({
    async commitBrowserBootstrap() { return false; },
    async signupCandidateInBrowser(_, bindBootstrap) {
      await bindBootstrap(candidateId);
      return { accountId: candidateId };
    },
  }), /auth_refused/);
  await assert.rejects(base({
    async commitBrowserBootstrap() { return true; },
    async signupCandidateInBrowser(_, bindBootstrap) {
      await bindBootstrap(candidateId);
      await bindBootstrap(candidateId);
      return { accountId: candidateId };
    },
  }), /auth_refused/);
  await assert.rejects(base({
    async commitBrowserBootstrap() { return true; },
    async signupCandidateInBrowser(_, bindBootstrap) {
      await bindBootstrap(operatorId);
      return { accountId: candidateId };
    },
  }), /auth_refused/);
  await assert.rejects(base({
    async commitBrowserBootstrap() { return true; },
    async signupCandidateInBrowser(_, bindBootstrap) {
      await bindBootstrap(candidateId);
      return { accountId: operatorId };
    },
  }), /auth_refused/);
});

test("transport refuses hidden routes and header overrides and never follows a redirect", async t => {
  const { call, requests, proofs } = await loopback(t, () => ({ status: 302 }));
  for (const path of ["https://example.invalid/", "//example.invalid/api/jobs/workspace", "/api/jobs/workspace?q=x",
    "/api/jobs/applications/fixture/submit", "__proto__"]) assert.throws(() => validateLocalQaLinuxApiRequest(path, { token }));
  assert.throws(() => validateLocalQaLinuxApiRequest("/api/jobs/workspace", { token, headers: { authorization: "override" } }));
  assert.throws(() => validateLocalQaLinuxApiRequest("/auth/login", { token, method: "POST", body: {} }));
  assert.throws(() => validateLocalQaLinuxApiRequest("/api/jobs/resume-source", { token, method: "POST", body: {} }));
  await assert.rejects(call("/api/jobs/workspace", { token }));
  assert.equal(requests.length, 1);
  assert.equal(proofs(), 2);
});
