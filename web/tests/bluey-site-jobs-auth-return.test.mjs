import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import vm from "node:vm";
import { buildPair } from "../../jobs/scripts/build-jobs-static-auth-pair.mjs";
import { startLocalQaGateway } from "../../jobs/scripts/local-qa-gateway.mjs";

const source = await readFile(new URL("../assets/bluey-site.js", import.meta.url), "utf8");

function functionSource(prefix, name, nextName, nextPrefix = prefix) {
  const start = source.indexOf(`${prefix} ${name}(`);
  const end = source.indexOf(`\n\n    ${nextPrefix} ${nextName}(`, start);
  assert.ok(start >= 0 && end > start, `${name} missing or ambiguous`);
  return source.slice(start, end);
}

const targetSource = functionSource("function", "jobsReturnTarget", "jobsSignupRequested");
const signupSource = functionSource("function", "jobsSignupRequested", "navigateToJobsAfterAuth");
const navigationStart = source.indexOf("function navigateToJobsAfterAuth(");
const navigationEnd = source.indexOf("\n\n    let pendingSignupEmail", navigationStart);
assert.ok(navigationStart >= 0 && navigationEnd > navigationStart, "navigation helper missing or ambiguous");
const navigationSource = source.slice(navigationStart, navigationEnd);
const passwordLoginSource = functionSource("async function", "accountAuth", "startSignupOtp");
const apiResponseSource = functionSource("async function", "apiJsonResponse", "apiJson");
const apiSource = functionSource("async function", "apiJson", "setTrialModal", "function");
const signupOtpSource = functionSource("async function", "confirmSignupOtp", "trialConvertMessage", "function");
const initAccountSource = functionSource("function", "initAccountApp", "renderRecoveryRoute");
const initAccountHarnessEnd = initAccountSource.indexOf(
  "\n      document.querySelectorAll('[data-password-toggle]')",
);
assert.ok(initAccountHarnessEnd > 0, "account signup-toggle initialization missing or ambiguous");
const initAccountHarnessSource = `${initAccountSource.slice(0, initAccountHarnessEnd)}\n    }`;

function targetFor(search) {
  const context = { URLSearchParams, location: { search } };
  vm.runInNewContext(`${targetSource}; result = jobsReturnTarget()`, context);
  return context.result;
}

function signupFor(search, { path = "/login", pendingDevice = false } = {}) {
  const context = {
    URLSearchParams,
    location: { search },
    currentPath: path,
    pendingDeviceCode: () => pendingDevice,
  };
  vm.runInNewContext(`${targetSource}; ${signupSource}; result = jobsSignupRequested()`, context);
  return context.result;
}

test("Jobs return target accepts only one exact local Jobs path", () => {
  assert.equal(targetFor("?next=%2Fjobs"), "/jobs/");
  assert.equal(targetFor("?next=%2Fjobs%2F"), "/jobs/");
  for (const search of [
    "", "?next=jobs", "?next=%2Fjobs%3Fpreview%3D1",
    "?next=https%3A%2F%2Fevil.example%2Fjobs", "?next=%2Fjobs&next=%2Fjobs",
    "?next=%2Fjobs&next=https%3A%2F%2Fevil.example", "?next=%252Fjobs",
  ]) assert.equal(targetFor(search), null, search);
});

test("signup mode is Jobs-login-only and cannot preempt a device handoff", () => {
  assert.equal(signupFor("?mode=signup&next=%2Fjobs"), true);
  for (const search of [
    "?next=%2Fjobs", "?mode=signup", "?mode=signup&next=https%3A%2F%2Fevil.example",
    "?mode=signup&mode=signup&next=%2Fjobs", "?mode=login&next=%2Fjobs",
    "?mode=signup&next=%2Fjobs&next=%2Fjobs",
  ]) assert.equal(signupFor(search), false, search);
  for (const path of ["/account", "/link", "/password-reset", "/reload"]) {
    assert.equal(signupFor("?mode=signup&next=%2Fjobs", { path }), false, path);
  }
  assert.equal(signupFor("?mode=signup&next=%2Fjobs", { pendingDevice: true }), false);
});

test("Jobs return navigation is a full fixed-path navigation", () => {
  const assigned = [];
  const context = {
    URLSearchParams,
    location: { search: "?next=%2Fjobs" },
    window: { location: { assign: target => assigned.push(target) } },
  };
  vm.runInNewContext(`${targetSource}; ${navigationSource}; result = navigateToJobsAfterAuth()`, context);
  assert.equal(context.result, true);
  assert.deepEqual(assigned, ["/jobs/"]);
});

function authContext({ search = "", path = "/login", pendingDevice = false } = {}) {
  const calls = { assigned: [], accountLoads: 0, replaces: 0, tokens: 0, otpModes: [] };
  const values = {
    accountEmail: { value: "synthetic@example.test" },
    accountPassword: { value: "synthetic-password" },
    signupOtp: { value: "123456" },
    rememberMe: { checked: true },
  };
  return {
    calls,
    values,
    context: {
      URLSearchParams,
      location: { search },
      window: { location: { assign: target => calls.assigned.push(target) } },
      currentPath: path,
      pendingSignupEmail: values.accountEmail.value,
      accountAuthMode: "signup",
      accountAuthEpoch: 0,
      jobsPasswordLoginAttempt: null,
      signupModeRevision: 0,
      signupStartInFlight: false,
      signupConfirmInFlight: false,
      document: { getElementById: id => values[id] },
      requireSignupTerms: () => true,
      browserTrialDeviceId: () => "synthetic-device",
      accountMessage: () => {},
      apiJson: async () => ({ access_token: "synthetic-access", refresh_token: "synthetic-refresh" }),
      setAccountToken: () => { calls.tokens += 1; },
      pendingDeviceCode: () => pendingDevice,
      history: { state: null, replaceState: () => { calls.replaces += 1; } },
      loadAccount: async () => { calls.accountLoads += 1; },
      setSignupOtpMode: enabled => { calls.otpModes.push(enabled); },
    },
  };
}

async function runAuth(sourceBody, name, options) {
  const { context, calls } = authContext(options);
  if (name === "accountAuth") context.accountAuthMode = "login";
  const argument = name === "accountAuth" ? "'login'" : "";
  vm.runInNewContext(`${targetSource}; ${navigationSource}; ${sourceBody}; promise = ${name}(${argument})`, context);
  await context.promise;
  return calls;
}

test("actual copied password login returns to Jobs without loading Account", async () => {
  const calls = await runAuth(passwordLoginSource, "accountAuth", { search: "?next=%2Fjobs" });
  assert.deepEqual(calls.assigned, ["/jobs/"]);
  assert.equal(calls.accountLoads, 0);
  assert.equal(calls.replaces, 0);
  assert.equal(calls.tokens, 1);
});

function passwordLoginHarness() {
  const { context, calls } = authContext({ search: "?next=%2Fjobs" });
  context.accountAuthMode = "login";
  const requests = [];
  context.apiJson = (...args) => new Promise((resolve, reject) => { requests.push({ args, resolve, reject }); });
  vm.runInNewContext(`${targetSource}; ${navigationSource}; ${passwordLoginSource}`, context);
  return { context, calls, requests };
}

test("stale Jobs password success cannot replace the signup view or a newer session", async () => {
  for (const change of [
    context => { context.accountAuthMode = "signup"; context.signupModeRevision++; },
    context => { context.accountAuthEpoch++; },
    context => { context.location.search = ""; },
  ]) {
    const { context, calls, requests } = passwordLoginHarness();
    const first = context.accountAuth("login");
    assert.equal(requests.length, 1);
    change(context);
    requests[0].resolve({ access_token: "obsolete-synthetic-access" });
    await first;
    assert.equal(calls.tokens, 0);
    assert.deepEqual(calls.assigned, []);
    assert.equal(calls.accountLoads, 0);
  }
});

test("a Jobs password request from a previous view cannot clear or duplicate a new attempt", async () => {
  const { context, calls, requests } = passwordLoginHarness();
  const old = context.accountAuth("login");
  const firstDuplicate = context.accountAuth("login");
  assert.equal(requests.length, 1);
  context.signupModeRevision += 2; // Login -> signup -> login during the old request.
  const fresh = context.accountAuth("login");
  assert.equal(requests.length, 2);
  requests[0].reject(new Error("obsolete login error"));
  await old;
  const secondDuplicate = context.accountAuth("login");
  assert.equal(requests.length, 2);
  requests[1].resolve({ access_token: "fresh-synthetic-access" });
  await Promise.all([fresh, firstDuplicate, secondDuplicate]);
  assert.deepEqual(requests.map(request => request.args[0]), ["/auth/login", "/auth/login"]);
  assert.equal(calls.tokens, 1);
  assert.deepEqual(calls.assigned, ["/jobs/"]);
});

test("the current Jobs password error is shown and the candidate can explicitly retry", async () => {
  const { context, calls, requests } = passwordLoginHarness();
  const first = context.accountAuth("login");
  requests[0].reject(new Error("invalid credentials"));
  await assert.rejects(first, /invalid credentials/);
  assert.equal(requests.length, 1);
  assert.equal(calls.tokens, 0);
  assert.deepEqual(calls.assigned, []);
  const retry = context.accountAuth("login");
  assert.equal(requests.length, 2);
  requests[1].resolve({ access_token: "fresh-synthetic-access" });
  await retry;
  assert.equal(calls.tokens, 1);
  assert.deepEqual(calls.assigned, ["/jobs/"]);
});

test("password login through the real QA gateway does not send another session's credentials", async t => {
  const root = await realpath(await mkdtemp(join(tmpdir(), "bluey-web-auth-")));
  t.after(() => rm(root, { recursive: true }));
  await mkdir(join(root, "jobs"));
  const documents = { "/index.html": "login", "/jobs/index.html": "jobs" };
  for (const [path, body] of Object.entries(documents)) await writeFile(join(root, path.slice(1)), body);
  const accountId = "11111111-1111-4111-8111-111111111111";
  const email = "candidate@bluey-qa.test";
  const password = `Qa-${"a".repeat(48)}`;
  const upstream = [];
  const backend = createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    upstream.push({ path: request.url, auth: request.headers.authorization, cookie: request.headers.cookie,
      body: Buffer.concat(chunks).toString() });
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ account: { id: accountId, email, is_admin: false, is_temporary: false },
      access_token: "dummy.candidate.signature" }));
  });
  await new Promise((resolve, reject) => backend.once("error", reject).listen(0, "127.0.0.1", resolve));
  let gateway;
  t.after(async () => {
    await gateway?.close();
    await new Promise(resolve => backend.close(resolve));
  });
  gateway = await startLocalQaGateway({ webDirectory: root,
    manifest: Object.fromEntries(Object.entries(documents).map(([path, body]) => [path, createHash("sha256").update(body).digest("hex")])),
    backendPort: backend.address().port, verifyBackend: () => backend.listening,
    candidateReadback: { accountId, resumeId: null }, expiresAtMs: Date.now() + 30_000 });

  const headers = { origin: gateway.origin, "content-type": "application/json" };
  const body = JSON.stringify({ email, password });
  for (const extra of [{ authorization: "Bearer dummy.other.signature" }, { cookie: "qa=dummy-other" }]) {
    const refused = await fetch(`${gateway.origin}/auth/login`, { method: "POST", headers: { ...headers, ...extra }, body });
    assert.equal(refused.status, 400);
    assert.equal(await refused.text(), "Request refused\n");
  }
  assert.deepEqual(upstream, []);

  const { context, calls, values } = authContext({ search: "?next=%2Fjobs" });
  context.accountAuthMode = "login";
  values.accountEmail.value = email;
  values.accountPassword.value = password;
  const attempts = [];
  Object.assign(context, { Headers, accountToken: () => "dummy.other.signature",
    refreshAccountToken: () => assert.fail("password login must not refresh another session"),
    fetch: async (path, options) => {
      assert.equal(path, "/auth/login");
      const forwarded = new Headers(options.headers);
      forwarded.set("origin", gateway.origin);
      // Node has no browser cookie jar; simulate the existing same-origin cookie.
      if (options.credentials !== "omit") forwarded.set("cookie", "qa=dummy-other");
      const reply = await fetch(`${gateway.origin}/auth/login`, { ...options, headers: forwarded, redirect: "error" });
      attempts.push(reply.status);
      return reply;
    } });
  vm.runInNewContext(`${targetSource}; ${navigationSource}; ${passwordLoginSource}; ${apiResponseSource}; ${apiSource}`, context);
  await context.accountAuth("login");
  assert.deepEqual(attempts, [200]);
  assert.deepEqual(upstream, [{ path: "/auth/login", auth: undefined, cookie: undefined, body }]);
  assert.equal(calls.tokens, 1);
  assert.deepEqual(calls.assigned, ["/jobs/"]);
});

test("password 401 never refreshes or replays, but authenticated account reads still can", async () => {
  const { context, calls } = authContext({ search: "?next=%2Fjobs" });
  context.accountAuthMode = "login";
  const requests = [];
  let token = "dummy.old.signature", refreshes = 0;
  Object.assign(context, { Headers, accountToken: () => token,
    refreshAccountToken: async () => { refreshes++; token = "dummy.new.signature"; return token; },
    fetch: async (path, options) => {
      requests.push({ path, auth: options.headers.get("authorization"), credentials: options.credentials });
      return path === "/account/me" && options.headers.get("authorization") === "Bearer dummy.new.signature"
        ? { status: 200, ok: true, json: async () => ({ ok: true }) }
        : { status: 401, ok: false, json: async () => ({ error: "invalid credentials" }) };
    } });
  vm.runInNewContext(`${targetSource}; ${passwordLoginSource}; ${apiResponseSource}; ${apiSource}`, context);
  await assert.rejects(context.accountAuth("login"), /invalid credentials/);
  assert.equal(calls.tokens, 0);
  assert.equal(refreshes, 0);
  assert.deepEqual(requests, [{ path: "/auth/login", auth: null, credentials: "omit" }]);
  assert.deepEqual(await context.apiJson("/account/me"), { ok: true });
  assert.equal(refreshes, 1);
  assert.deepEqual(requests.slice(1), [
    { path: "/account/me", auth: "Bearer dummy.old.signature", credentials: undefined },
    { path: "/account/me", auth: "Bearer dummy.new.signature", credentials: undefined },
  ]);
});

test("public signup OTP writes omit other sessions and never refresh or replay", async () => {
  for (const path of ["/auth/signup/start", "/auth/signup/confirm"]) {
    const requests = [];
    let refreshes = 0;
    const context = {
      Headers,
      accountToken: () => "dummy.old.signature",
      refreshAccountToken: async () => { refreshes++; return "dummy.new.signature"; },
      fetch: async (requestPath, options) => {
        const cookie = options.headers.get("cookie") ?? (options.credentials !== "omit" ? "qa=stale" : null);
        const authorization = options.headers.get("authorization");
        requests.push({ path: requestPath, authorization, cookie, credentials: options.credentials });
        // Match the synthetic signup gateway's refusal of ambient session credentials.
        if (options.method === "POST" && (cookie || authorization)) {
          return { status: 400, ok: false, json: async () => ({ error: "Request refused" }) };
        }
        return { status: 401, ok: false, json: async () => ({ error: "current signup error" }) };
      },
    };
    vm.runInNewContext(`${apiResponseSource}; ${apiSource}`, context);
    await assert.rejects(context.apiJson(path, {
      method: "POST",
      credentials: "include",
      headers: { Authorization: "Bearer dummy.explicit.signature", Cookie: "qa=explicit" },
      body: "{}",
    }), /current signup error/);
    assert.deepEqual(requests, [{ path, authorization: null, cookie: null, credentials: "omit" }]);
    assert.equal(refreshes, 0);
  }
});

test("an authenticated trial conversion remains outside the public signup write boundary", async () => {
  const path = "/auth/trial/convert/start";
  const context = {
    Headers,
    accountToken: () => "dummy.trial.signature",
    refreshAccountToken: () => assert.fail("unexpected refresh"),
    fetch: async (requestPath, options) => {
      assert.equal(requestPath, path);
      assert.equal(options.headers.get("authorization"), "Bearer dummy.trial.signature");
      assert.equal(options.credentials, undefined);
      return { status: 200, ok: true, json: async () => ({ ok: true }) };
    },
  };
  vm.runInNewContext(`${apiResponseSource}; ${apiSource}`, context);
  assert.deepEqual(await context.apiJson(path, { method: "POST" }), { ok: true });
});

test("actual copied OTP signup returns to Jobs without loading Account", async () => {
  const calls = await runAuth(signupOtpSource, "confirmSignupOtp", { search: "?mode=signup&next=%2Fjobs" });
  assert.deepEqual(calls.assigned, ["/jobs/"]);
  assert.equal(calls.accountLoads, 0);
  assert.equal(calls.replaces, 0);
  assert.deepEqual(calls.otpModes, [false]);
  assert.equal(calls.tokens, 1);
});

function confirmHarness() {
  const { context, calls, values } = authContext({ search: "?mode=signup&next=%2Fjobs" });
  vm.runInNewContext(`${targetSource}; ${navigationSource}; ${signupOtpSource}`, context);
  const requests = [];
  context.apiJson = (...args) => new Promise((resolve, reject) => { requests.push({ args, resolve, reject }); });
  return { context, calls, values, requests };
}

test("verification confirmation sends one POST and never asks auth refresh to replay it", async () => {
  const { context, calls, requests } = confirmHarness();
  const first = context.confirmSignupOtp();
  const duplicate = context.confirmSignupOtp();
  const issued = requests.slice();
  for (const request of issued) request.resolve({ access_token: "synthetic-access" });
  await Promise.all([first, duplicate]);
  assert.equal(issued.length, 1);
  assert.equal(issued[0].args[0], "/auth/signup/confirm");
  assert.equal(issued[0].args[1].skipAuthRefresh, true);
  assert.deepEqual(JSON.parse(issued[0].args[1].body), {
    email: "synthetic@example.test", otp: "123456", device_fingerprint: "synthetic-device",
  });
  assert.equal(calls.tokens, 1);
  assert.deepEqual(calls.assigned, ["/jobs/"]);
});

test("the real API wrapper leaves confirmation 401 to the form without refresh or replay", async () => {
  const { context, calls } = confirmHarness();
  const requests = [];
  let refreshes = 0;
  Object.assign(context, {
    Headers,
    accountToken: () => "existing-synthetic-token",
    refreshAccountToken: async () => { refreshes++; return "refreshed-synthetic-token"; },
    fetch: async (path, options) => {
      requests.push({ path, method: options.method });
      return { status: 401, ok: false, json: async () => ({ error: "invalid verification code" }) };
    },
  });
  const response = functionSource("async function", "apiJsonResponse", "apiJson");
  const api = functionSource("async function", "apiJson", "setTrialModal", "function");
  vm.runInNewContext(`${response}; ${api}`, context);
  await assert.rejects(context.confirmSignupOtp(), /invalid verification code/);
  assert.deepEqual(requests, [{ path: "/auth/signup/confirm", method: "POST" }]);
  assert.equal(refreshes, 0);
  assert.equal(calls.tokens, 0);
});

test("late signup confirmation cannot install tokens for a stale view or sign-in", async () => {
  for (const change of [
    context => { context.signupModeRevision += 2; },
    context => { context.accountAuthEpoch++; },
    context => { context.pendingSignupEmail = "another@example.test"; },
  ]) {
    const { context, calls, requests } = confirmHarness();
    const first = context.confirmSignupOtp();
    assert.equal(requests.length, 1);
    change(context);
    requests[0].resolve({ access_token: "obsolete-synthetic-access" });
    await first;
    assert.equal(calls.tokens, 0);
    assert.deepEqual(calls.assigned, []);
    assert.deepEqual(calls.otpModes, []);
  }
});

test("a current verification error allows explicit retry with a corrected code", async () => {
  const { context, calls, values, requests } = confirmHarness();
  const first = context.confirmSignupOtp();
  requests[0].reject(new Error("invalid verification code"));
  await assert.rejects(first, /invalid verification code/);
  assert.equal(requests.length, 1);
  assert.equal(calls.tokens, 0);
  values.signupOtp.value = "234567";
  const retry = context.confirmSignupOtp();
  assert.equal(requests.length, 2);
  assert.equal(JSON.parse(requests[1].args[1].body).otp, "234567");
  requests[1].resolve({ access_token: "synthetic-access" });
  await retry;
  assert.equal(calls.tokens, 1);
});

test("a failed confirmation for a departed mode does not show an obsolete error", async () => {
  const { context, calls, requests } = confirmHarness();
  const first = context.confirmSignupOtp();
  context.accountAuthMode = "login";
  context.signupModeRevision++;
  requests[0].reject(new Error("obsolete verification error"));
  await first;
  assert.equal(calls.tokens, 0);
  assert.deepEqual(calls.assigned, []);
});

test("confirmation sends nothing during resends, outside verification, or for malformed codes", async () => {
  for (const change of [
    ({ context }) => { context.signupStartInFlight = true; },
    ({ context }) => { context.pendingSignupEmail = ""; },
    ({ context }) => { context.accountAuthMode = "login"; },
    ({ values }) => { values.signupOtp.value = "12345"; },
    ({ values }) => { values.signupOtp.value = "12345x"; },
  ]) {
    const harness = confirmHarness();
    change(harness);
    const attempt = harness.context.confirmSignupOtp();
    for (const request of harness.requests) request.resolve({ access_token: "dummy-unexpected" });
    await attempt;
    assert.equal(harness.requests.length, 0);
    assert.equal(harness.calls.tokens, 0);
  }
});

test("actual copied auth branches retain Account behavior for rejected, device, and link cases", async () => {
  const absent = await runAuth(passwordLoginSource, "accountAuth");
  assert.deepEqual(absent.assigned, []);
  assert.equal(absent.replaces, 1);
  assert.equal(absent.accountLoads, 1);

  const rejected = await runAuth(signupOtpSource, "confirmSignupOtp", { search: "?next=https%3A%2F%2Fevil.example" });
  assert.deepEqual(rejected.assigned, []);
  assert.equal(rejected.accountLoads, 1);

  const device = await runAuth(passwordLoginSource, "accountAuth", { search: "?next=%2Fjobs", pendingDevice: true });
  assert.deepEqual(device.assigned, []);
  assert.equal(device.replaces, 0);
  assert.equal(device.accountLoads, 1);

  const signupDevice = await runAuth(signupOtpSource, "confirmSignupOtp", { search: "?next=%2Fjobs", pendingDevice: true });
  assert.deepEqual(signupDevice.assigned, []);
  assert.deepEqual(signupDevice.otpModes, [false]);
  assert.equal(signupDevice.accountLoads, 1);
  assert.equal(signupDevice.tokens, 1);

  const link = await runAuth(passwordLoginSource, "accountAuth", { search: "?next=%2Fjobs", path: "/link" });
  assert.deepEqual(link.assigned, []);
  assert.equal(link.replaces, 0);
  assert.equal(link.accountLoads, 1);
});

test("Jobs login startup skips desktop release metadata without skipping account initialization", () => {
  const start = source.lastIndexOf("\n    initFilePreview();");
  const end = source.lastIndexOf("\n}");
  assert.ok(start >= 0 && end > start);
  for (const [currentPath, search, expectedReleaseLoads] of [
    ["/login", "?mode=signup&next=%2Fjobs", 0],
    ["/login", "?next=%2Fjobs%2F", 0],
    ["/login", "?next=%2Fjobs&next=%2Fjobs", 1],
    ["/login", "", 1],
    ["/download", "?next=%2Fjobs", 1],
    ["/", "", 1],
  ]) {
    let releaseLoads = 0;
    let accountInits = 0;
    const noop = () => {};
    vm.runInNewContext(`${targetSource}; ${source.slice(start, end)}`, {
      currentPath, URLSearchParams, location: { search },
      initFilePreview: noop, initSiteTheme: noop, initProfileMenus: noop, initProductJoinForm: noop,
      syncAccountNav: noop, initDownloadInstructions: noop,
      loadReleaseManifest() { releaseLoads++; },
      initAccountApp() { accountInits++; },
    });
    assert.equal(releaseLoads, expectedReleaseLoads, `${currentPath}${search}`);
    assert.equal(accountInits, 1);
  }
});

function captchaFunctionSource(name, nextName) {
  const start = source.indexOf(`    ${name}(`);
  const end = source.indexOf(`\n\n    ${nextName}(`, start);
  assert.ok(start >= 0 && end > start, `${name} missing or ambiguous`);
  return source.slice(start, end);
}

const loadCaptchaConfigSource = captchaFunctionSource("async function loadCaptchaConfig", "function loadTurnstileScript");
const loadTurnstileScriptSource = captchaFunctionSource("function loadTurnstileScript", "async function refreshSignupCaptcha");
const refreshSignupCaptchaSource = captchaFunctionSource("async function refreshSignupCaptcha", "function signupTurnstilePayload");
const signupTurnstilePayloadSource = captchaFunctionSource("function signupTurnstilePayload", "function setAccountAuthMode");
const setAccountAuthModeSource = captchaFunctionSource("function setAccountAuthMode", "function setRailCommand");
const startSignupOtpSource = captchaFunctionSource("async function startSignupOtp", "async function confirmSignupOtp");
const ensureTrialHumanCheckSource = captchaFunctionSource("async function ensureTrialHumanCheck", "async function startTrial");
const captchaControlStart = source.indexOf("    async function loadCaptchaConfig(");
const captchaControlEnd = source.indexOf("\n\n    function setRailCommand(", captchaControlStart);
assert.ok(captchaControlStart >= 0 && captchaControlEnd > captchaControlStart,
  "captcha controls missing or ambiguous");
const captchaControlSource = source.slice(captchaControlStart, captchaControlEnd);

function captchaHarness({ fetchImpl, captcha = true, turnstile, postImpl } = {}) {
  const calls = { fetch: 0, posts: 0, requests: [], messages: [], scripts: [], otp: [], resets: 0, renders: 0 };
  const elements = {
    signupCaptcha: captcha ? { hidden: true } : null,
    accountEmail: { value: "candidate@bluey-qa.test" },
    accountPassword: { value: "Qa-0123456789abcdef0123456789abcdef0123456789abcdef" },
    accountPasswordConfirm: { value: "Qa-0123456789abcdef0123456789abcdef0123456789abcdef", focus() {} },
  };
  const head = {
    appendChild(script) { calls.scripts.push(script); },
  };
  const context = {
    Promise,
    Error,
    window: { turnstile },
    document: {
      head,
      createElement() {
        return { remove() { this.removed = true; } };
      },
      getElementById(id) { return elements[id] || null; },
    },
    fetch: async (...args) => {
      calls.fetch += 1;
      return fetchImpl(...args);
    },
    accountMessage(message) { calls.messages.push(message); },
    requireSignupTerms: () => true,
    apiJson: async (...args) => {
      calls.posts += 1;
      calls.requests.push(args);
      if (postImpl) return postImpl(...args);
      return { email: "candidate@bluey-qa.test" };
    },
    browserTrialDeviceId: () => "qa-device",
    setSignupOtpMode(...args) { calls.otp.push(args); },
    calls,
  };
  const body = `
    let captchaConfigPromise = null;
    let captchaConfig = { provider: null, site_key: null };
    let accountAuthMode = 'signup';
    let pendingSignupEmail = '';
    let signupTurnstileToken = '';
    let signupTurnstileWidgetId = null;
    let signupStartInFlight = false;
    let signupConfirmInFlight = false;
    let signupModeRevision = 0;
    ${loadCaptchaConfigSource}
    ${loadTurnstileScriptSource}
    ${refreshSignupCaptchaSource}
    ${signupTurnstilePayloadSource}
    ${setAccountAuthModeSource}
    ${startSignupOtpSource}
    function simulateConfirmInFlight() { signupConfirmInFlight = true; }
  `;
  vm.runInNewContext(body, context);
  return { context, calls, elements };
}

async function settleCaptchaVm() {
  await new Promise((resolve) => setImmediate(resolve));
}

test("a second signup click during a pending request sends no second POST", async () => {
  let complete;
  const pending = new Promise((resolve) => { complete = resolve; });
  const { context, calls } = captchaHarness({
    fetchImpl: async () => ({ ok: true, json: async () => ({ provider: null, site_key: null }) }),
    postImpl: () => pending,
  });
  const first = context.startSignupOtp();
  const second = context.startSignupOtp();
  await settleCaptchaVm();
  try {
    assert.equal(calls.posts, 1);
    assert.equal(calls.requests[0][1].skipAuthRefresh, true);
  } finally {
    complete({ email: "candidate@bluey-qa.test" });
    await Promise.all([first, second]);
  }
  assert.equal(calls.otp.length, 1);
});

test("a resend cannot replace the code during confirmation", async () => {
  const { context, calls } = captchaHarness({
    fetchImpl: async () => ({ ok: true, json: async () => ({ provider: null, site_key: null }) }),
  });
  context.simulateConfirmInFlight();
  await context.startSignupOtp();
  assert.equal(calls.posts, 0);
});

test("a failed signup POST needs a fresh security check before another POST", async () => {
  const { context, calls } = captchaHarness({
    fetchImpl: async () => ({ ok: true, json: async () => ({ provider: "turnstile", site_key: "fixture" }) }),
    turnstile: { render() { calls.renders += 1; return 1; } },
    postImpl: async () => { throw new Error("400 Bad Request"); },
  });
  vm.runInContext("signupTurnstileToken = 'initial-token'", context);
  await assert.rejects(context.startSignupOtp(), /400 Bad Request/);
  assert.equal(calls.posts, 1);
  assert.equal(context.signupTurnstilePayload(), null);
  await context.startSignupOtp();
  assert.equal(calls.posts, 1);
  assert.equal(calls.renders, 1);
  assert.match(calls.messages.at(-1), /Complete the security check/);
});

test("a user can retry a rejected security check with a replacement token, never an automatic POST", async () => {
  let widget;
  const { context, calls } = captchaHarness({
    fetchImpl: async () => ({ ok: true, json: async () => ({ provider: "turnstile", site_key: "fixture" }) }),
    turnstile: {
      render(_element, options) { widget = options; return 1; },
      reset() { calls.resets += 1; },
    },
    postImpl: async () => {
      if (calls.posts === 1) throw new Error("captcha verification failed");
      return { email: "candidate@bluey-qa.test" };
    },
  });
  await context.refreshSignupCaptcha();
  widget.callback("first-token");
  await assert.rejects(context.startSignupOtp(), /captcha verification failed/);
  assert.equal(calls.posts, 1);
  assert.equal(calls.otp.length, 0);
  await context.startSignupOtp();
  assert.equal(calls.posts, 1);
  assert.equal(calls.resets, 1);
  widget.callback("replacement-token");
  await context.startSignupOtp();
  assert.deepEqual(calls.requests.map(([, options]) => JSON.parse(options.body).turnstile_token),
    ["first-token", "replacement-token"]);
  assert.equal(calls.otp.length, 1);
});

test("switching away and back while a signup POST settles does not reopen the old OTP view", async () => {
  let complete;
  const pending = new Promise((resolve) => { complete = resolve; });
  const { context, calls } = captchaHarness({
    fetchImpl: async () => ({ ok: true, json: async () => ({ provider: null, site_key: null }) }),
    postImpl: () => pending,
  });
  const first = context.startSignupOtp();
  await settleCaptchaVm();
  assert.equal(calls.posts, 1);
  context.setAccountAuthMode("login");
  context.setAccountAuthMode("signup");
  complete({ email: "candidate@bluey-qa.test" });
  await first;
  assert.deepEqual(calls.otp, [[false]]);
  await context.startSignupOtp();
  assert.equal(calls.posts, 2);
  assert.deepEqual(calls.otp, [[false], [true, "candidate@bluey-qa.test"]]);
});

function accountCaptchaHarness({ search = "?next=%2Fjobs", fetchImpl } = {}) {
  const calls = { fetch: [], messages: [], renders: 0, resets: 0 };
  const listeners = new Map();
  const elements = new Map([["signupCaptcha", { hidden: true }]]);
  function element(id) {
    if (!elements.has(id)) {
      elements.set(id, {
        id,
        hidden: id === "signupCaptcha",
        dataset: {},
        classList: { remove() {}, toggle() {} },
        addEventListener(name, listener) { listeners.set(`${id}:${name}`, listener); },
        focus() {},
        getAttribute() { return "password"; },
        setAttribute() {},
      });
    }
    return elements.get(id);
  }
  const context = vm.createContext({
    Promise,
    Error,
    URLSearchParams,
    location: { search },
    currentPath: "/login",
    isPolicyRoute: false,
    isDownloadRoute: false,
    isAccountRoute: true,
    productSite: element("productSite"),
    accountApp: element("accountApp"),
    downloadApp: element("downloadApp"),
    policyApp: element("policyApp"),
    window: {
      turnstile: {
        render(_mount, options) {
          calls.renders += 1;
          calls.renderOptions = options;
          return "synthetic-widget";
        },
        reset() { calls.resets += 1; },
      },
    },
    document: {
      head: { appendChild() { throw new Error("unexpected captcha script request"); } },
      createElement() { return { remove() {} }; },
      getElementById: element,
      querySelectorAll: () => [],
      addEventListener() {},
    },
    fetch: async (...args) => {
      calls.fetch.push(args);
      return fetchImpl(...args);
    },
    pendingDeviceCode: () => false,
    renderDeviceLinkHint() {},
    initDashboardTabs() {},
    installBillingMoneyInputGuards() {},
    initProfileMenus() {},
    setSignupOtpMode() {},
    accountMessage(message) { calls.messages.push(message); },
    loadAccount: async () => {},
  });
  vm.runInContext(`
    let captchaConfigPromise = null;
    let captchaConfig = { provider: null, site_key: null };
    let accountAuthMode = 'login';
    let signupModeRevision = 0;
    let signupTurnstileToken = '';
    let signupTurnstileWidgetId = null;
    let pendingSignupEmail = '';
    ${targetSource}
    ${signupSource}
    ${captchaControlSource}
    ${initAccountHarnessSource}
  `, context);
  return {
    calls,
    context,
    elements,
    listener(id, name) {
      const listener = listeners.get(`${id}:${name}`);
      assert.equal(typeof listener, "function", `${id}:${name} listener`);
      return listener;
    },
  };
}

const disabledCaptchaResponse = async () => ({
  ok: true,
  json: async () => ({ provider: null, site_key: null }),
});
const enabledCaptchaResponse = async () => ({
  ok: true,
  json: async () => ({ provider: "turnstile", site_key: "synthetic-site-key" }),
});

test("plain Jobs login initialization does not request signup captcha configuration", async () => {
  const harness = accountCaptchaHarness({ fetchImpl: disabledCaptchaResponse });
  vm.runInContext("initAccountApp()", harness.context);
  await settleCaptchaVm();
  assert.deepEqual(harness.calls.fetch, []);
  assert.equal(harness.elements.get("signupCaptcha").hidden, true);
  assert.equal(vm.runInContext("accountAuthMode", harness.context), "login");
});

test("explicit Jobs signup and a signup toggle request captcha configuration", async () => {
  const explicit = accountCaptchaHarness({
    search: "?mode=signup&next=%2Fjobs",
    fetchImpl: enabledCaptchaResponse,
  });
  vm.runInContext("initAccountApp()", explicit.context);
  await settleCaptchaVm();
  assert.equal(explicit.calls.fetch.length, 1);
  assert.equal(explicit.calls.fetch[0][0], "/auth/captcha/config");
  assert.equal(explicit.calls.renders, 1);
  assert.equal(explicit.calls.renderOptions.sitekey, "synthetic-site-key");
  assert.equal(explicit.elements.get("signupCaptcha").hidden, false);
  assert.equal(vm.runInContext("accountAuthMode", explicit.context), "signup");

  const toggled = accountCaptchaHarness({ fetchImpl: enabledCaptchaResponse });
  vm.runInContext("initAccountApp()", toggled.context);
  await settleCaptchaVm();
  assert.deepEqual(toggled.calls.fetch, []);
  toggled.listener("createAccountButton", "click")();
  await settleCaptchaVm();
  assert.equal(toggled.calls.fetch.length, 1);
  assert.equal(toggled.calls.fetch[0][0], "/auth/captcha/config");
  assert.equal(toggled.calls.renders, 1);
  assert.equal(toggled.calls.renderOptions.sitekey, "synthetic-site-key");
  assert.equal(toggled.elements.get("signupCaptcha").hidden, false);
  assert.equal(vm.runInContext("accountAuthMode", toggled.context), "signup");
});

test("leaving signup clears captcha without lookup and fences pending login or OTP state", async () => {
  const direct = accountCaptchaHarness({ fetchImpl: disabledCaptchaResponse });
  direct.elements.get("signupCaptcha").hidden = false;
  vm.runInContext("signupTurnstileToken = 'synthetic-token'; setAccountAuthMode('login')", direct.context);
  assert.deepEqual(direct.calls.fetch, []);
  assert.equal(direct.elements.get("signupCaptcha").hidden, true);
  assert.equal(vm.runInContext("signupTurnstileToken", direct.context), "");

  for (const nextState of ["login", "otp"]) {
    let resolveConfig;
    const pending = accountCaptchaHarness({
      fetchImpl: () => new Promise((resolve) => { resolveConfig = resolve; }),
    });
    pending.elements.get("signupCaptcha").hidden = false;
    vm.runInContext(
      "accountAuthMode = 'signup'; signupTurnstileToken = 'synthetic-token'; pendingRefresh = refreshSignupCaptcha()",
      pending.context,
    );
    await settleCaptchaVm();
    assert.equal(pending.calls.fetch.length, 1);
    if (nextState === "login") {
      vm.runInContext("setAccountAuthMode('login')", pending.context);
    } else {
      vm.runInContext("pendingSignupEmail = 'candidate@bluey-qa.test'", pending.context);
      pending.elements.get("signupCaptcha").hidden = true;
    }
    assert.equal(pending.calls.fetch.length, 1, `${nextState} must not start another captcha lookup`);
    assert.equal(pending.elements.get("signupCaptcha").hidden, true);
    if (nextState === "login") {
      assert.equal(vm.runInContext("signupTurnstileToken", pending.context), "");
    }
    resolveConfig({
      ok: true,
      json: async () => ({ provider: "turnstile", site_key: "synthetic-site-key" }),
    });
    await vm.runInContext("pendingRefresh", pending.context);
    await settleCaptchaVm();
    assert.equal(pending.calls.renders, 0, nextState);
    assert.equal(pending.calls.resets, 0, nextState);
    assert.equal(pending.elements.get("signupCaptcha").hidden, true, nextState);
  }
});

test("the shared browser trial fails closed on malformed captcha configuration and can retry", async () => {
  let attempts = 0;
  let rendered;
  const resumed = [];
  const mount = { hidden: true };
  const { context, calls } = captchaHarness({
    fetchImpl: async () => ({
      ok: true,
      json: async () => (++attempts === 1
        ? { provider: null, site_key: "unexpected" }
        : { provider: "turnstile", site_key: "synthetic-site-key" }),
    }),
    turnstile: {
      render(element, options) {
        assert.equal(element, mount);
        rendered = options;
        return "synthetic-widget";
      },
    },
  });
  const originalGet = context.document.getElementById;
  context.document.getElementById = (id) => id === "trialTurnstile" ? mount : originalGet(id);
  Object.assign(context, {
    renderTrialModal() {},
    setTrialModal() {},
    storedSiteTheme: () => "light",
    startTrial: (button, acceptedTerms) => resumed.push([button, acceptedTerms]),
  });
  vm.runInContext(`
    let trialTurnstileToken = '';
    let trialTurnstileWidgetId = null;
    let pendingTrialButton = null;
    ${ensureTrialHumanCheckSource}
  `, context);

  const button = {};
  await assert.rejects(context.ensureTrialHumanCheck(button), /Could not load the security check/);
  assert.equal(rendered, undefined);
  assert.deepEqual(resumed, []);
  assert.equal(await context.ensureTrialHumanCheck(button), false);
  assert.equal(calls.fetch, 2);
  assert.equal(mount.hidden, false);
  assert.equal(rendered.action, "try_us");
  assert.deepEqual(resumed, []);
  rendered.callback("synthetic-token");
  assert.deepEqual(resumed, [[button, true]]);
});

test("captcha configuration refuses malformed HTTP or schema replies without poisoning a retry", async () => {
  for (const response of [
    { ok: false, json: async () => ({}) },
    { ok: true, json: async () => ({ provider: "turnstile", site_key: "" }) },
    { ok: true, json: async () => ({ provider: null, site_key: "unexpected" }) },
  ]) {
    let attempts = 0;
    const { context, calls } = captchaHarness({
      fetchImpl: async () => (++attempts === 1 ? response : {
        ok: true, json: async () => ({ provider: null, site_key: null }),
      }),
    });
    await assert.rejects(context.loadCaptchaConfig(), /Could not load the security check/);
    await assert.doesNotReject(context.loadCaptchaConfig());
    assert.equal(calls.fetch, 2, "a later user retry fetches a fresh configuration");
  }
});

test("captcha accepts explicit disabled config and shares a single Turnstile script request", async () => {
  const disabled = captchaHarness({
    fetchImpl: async () => ({ ok: true, json: async () => ({ provider: null, site_key: null }) }),
  });
  await disabled.context.refreshSignupCaptcha();
  assert.equal(disabled.elements.signupCaptcha.hidden, true);
  assert.equal(disabled.calls.scripts.length, 0);

  const turnstile = {
    render() { return "widget-1"; },
    reset() {},
  };
  const enabled = captchaHarness({
    turnstile,
    fetchImpl: async () => ({ ok: true, json: async () => ({ provider: "turnstile", site_key: "site-key" }) }),
  });
  await Promise.all([enabled.context.loadTurnstileScript(), enabled.context.loadTurnstileScript()]);
  assert.equal(enabled.calls.scripts.length, 0, "an already available API never appends a script");

  enabled.context.window.turnstile = undefined;
  const first = enabled.context.loadTurnstileScript();
  const second = enabled.context.loadTurnstileScript();
  assert.strictEqual(first, second);
  assert.equal(enabled.calls.scripts.length, 1);
  enabled.calls.scripts[0].onerror();
  await assert.rejects(first, /Could not load the security check/);
  assert.equal(enabled.calls.scripts[0].removed, true);
  assert.equal(enabled.context.window.__blueyTurnstileScriptPromise, null);
});

test("captcha script retries only after an explicit later refresh and rejects a load without the API", async () => {
  const { context, calls } = captchaHarness({
    fetchImpl: async () => ({ ok: true, json: async () => ({ provider: "turnstile", site_key: "site-key" }) }),
  });
  const first = context.refreshSignupCaptcha();
  await settleCaptchaVm();
  assert.equal(calls.scripts.length, 1);
  calls.scripts[0].onload();
  await assert.rejects(first, /Could not load the security check/);
  assert.equal(calls.posts, 0);
  assert.equal(calls.scripts[0].removed, true);
  const second = context.refreshSignupCaptcha();
  await settleCaptchaVm();
  assert.equal(calls.scripts.length, 2, "only an explicit refresh starts a later script request");
  context.window.turnstile = { render() { calls.renders += 1; return "widget-1"; }, reset() {} };
  calls.scripts[1].onload();
  await second;
  assert.equal(calls.renders, 1);
});

test("start signup waits for captcha refresh and never posts without a required token", async () => {
  const { context, calls } = captchaHarness({
    fetchImpl: async () => ({ ok: true, json: async () => ({ provider: "turnstile", site_key: "site-key" }) }),
  });
  const start = context.startSignupOtp();
  await settleCaptchaVm();
  assert.equal(calls.messages.length, 0, "feedback waits until the required security refresh finishes");
  assert.equal(calls.posts, 0);
  assert.equal(calls.scripts.length, 1);
  context.window.turnstile = { render() { return "widget-1"; }, reset() {} };
  calls.scripts[0].onload();
  await start;
  assert.equal(calls.posts, 0);
  assert.deepEqual(calls.messages, ["Complete the security check to create an account."]);
});

test("start signup does not post when signup mode changes while captcha configuration loads", async () => {
  let resolveConfig;
  const { context, calls } = captchaHarness({
    fetchImpl: () => new Promise((resolve) => { resolveConfig = resolve; }),
  });
  const start = context.startSignupOtp();
  await settleCaptchaVm();
  vm.runInContext("accountAuthMode = 'login'", context);
  resolveConfig({ ok: true, json: async () => ({ provider: null, site_key: null }) });
  await start;
  assert.equal(calls.posts, 0);
  assert.deepEqual(calls.messages, []);
});

const signupPath = "/login?mode=signup&next=%2Fjobs";
function authAssetRoute(html) {
  const entries = [...html.matchAll(/<script\b[^>]*\bsrc="(\/assets\/bluey-site\.js\?v=[0-9]{10})"[^>]*>/g)];
  assert.equal(entries.length, 1, "expected one versioned shared login script");
  return entries[0][1];
}

const hash = (value) => createHash("sha256").update(value).digest("hex");

function assertPublishedArtifact(label, bytes, expected) {
  assert.equal(bytes.byteLength, expected.bytes, `public ${label} size differs from this candidate`);
  assert.equal(hash(bytes), expected.sha256, `public ${label} differs from this candidate`);
}

async function verifyPublishedSignup(readPublished, sourcePair) {
  const { manifest } = buildPair(sourcePair);
  const expected = `/assets/bluey-site-${manifest.legacySource.sha256}.js`;
  const html = await readPublished(signupPath);
  // Bind the shell as well as the script so a stale script tag cannot piggyback
  // on an otherwise valid link, including via absolute URLs or comments.
  assertPublishedArtifact("login shell", html, manifest.files["web/index.html"]);
  assert.ok(html.includes('id="signupOtpLabel"'), "public login is missing the signup form");
  assertPublishedArtifact("login script", await readPublished(expected), manifest.files[`web${expected}`]);
}

async function readSignupSource() {
  const [html, javascript] = await Promise.all([
    readFile(new URL("../index.html", import.meta.url)),
    readFile(new URL("../assets/bluey-site.js", import.meta.url)),
  ]);
  return { html, javascript };
}

function jobsAssetRoute(html) {
  const entries = [...html.matchAll(/<script\b[^>]*\bsrc="(\/jobs\/assets\/index-[A-Za-z0-9_-]+\.js)"[^>]*>/g)];
  assert.equal(entries.length, 1, "expected one versioned Jobs entry bundle");
  return entries[0][1];
}

test("the source Jobs signup link and versioned shared shell are packaged together", async () => {
  const portal = await readFile(new URL("../../jobs/portal/src/components/AuthGate.tsx", import.meta.url), "utf8");
  const shell = await readFile(new URL("../index.html", import.meta.url), "utf8");
  assert.ok(portal.includes(`href="${signupPath}"`));
  assert.ok(shell.includes('id="signupOtpLabel"'));
  assert.doesNotThrow(() => authAssetRoute(shell));
  assert.throws(() => authAssetRoute(shell.replace(/\?v=[0-9]{10}/g, "")));
  const jobs = await readFile(new URL("../jobs/index.html", import.meta.url), "utf8");
  assert.doesNotThrow(() => jobsAssetRoute(jobs));
});

test("published signup verification accepts the builder's immutable pair", async () => {
  const sourcePair = await readSignupSource();
  const { files, manifest } = buildPair(sourcePair);
  const expected = `/assets/bluey-site-${manifest.legacySource.sha256}.js`;
  const seen = [];
  await verifyPublishedSignup(async (path) => {
    seen.push(path);
    return files.get(path === signupPath ? "web/index.html" : `web${path}`);
  }, sourcePair);
  assert.deepEqual(seen, [signupPath, expected]);
});

test("published signup refuses stale, mismatched, and mixed routes without fetching a script", async () => {
  const sourcePair = await readSignupSource();
  const { files, manifest } = buildPair(sourcePair);
  const expected = `/assets/bluey-site-${manifest.legacySource.sha256}.js`;
  const deployed = files.get("web/index.html").toString("utf8");
  for (const html of [
    sourcePair.html,
    Buffer.from(deployed.replace(expected, `/assets/bluey-site-${"0".repeat(64)}.js`)),
    Buffer.from(deployed.replace(expected, `${expected}?v=0000000000`)),
    Buffer.from(deployed.replace("</body>", `<script src="${manifest.legacySource.versionedPath}"></script></body>`)),
    Buffer.from(deployed.replace("</body>", `<script src="https://bluey.sh${manifest.legacySource.versionedPath}"></script></body>`)),
  ]) {
    const seen = [];
    await assert.rejects(verifyPublishedSignup(async (path) => {
      seen.push(path);
      return html;
    }, sourcePair), /public login shell (?:size )?differs from this candidate/);
    assert.deepEqual(seen, [signupPath]);
  }
});

test("published signup refuses same-size changed script bytes at the correct URL", async () => {
  const sourcePair = await readSignupSource();
  const { files } = buildPair(sourcePair);
  const changed = Buffer.from(sourcePair.javascript);
  changed[0] ^= 1;
  await assert.rejects(verifyPublishedSignup(async (path) => (
    path === signupPath ? files.get("web/index.html") : changed
  ), sourcePair), /public login script differs from this candidate/);
});

// Anonymous GET-only artifact verification. Never runs in normal CI and never
// submits signup data. Enable after the intended static release is published.
if (process.env.BLUEY_JOBS_VERIFY_PUBLIC_AUTH === "1") {
  async function readPublic(path) {
    const response = await fetch(new URL(path, "https://bluey.sh"), {
      redirect: "error",
      signal: AbortSignal.timeout(8_000),
    });
    assert.equal(response.status, 200, `GET ${path}`);
    assert.ok(response.body, `missing body for ${path}`);
    let bytes = 0;
    const chunks = [];
    for await (const chunk of response.body) {
      bytes += chunk.byteLength;
      assert.ok(bytes <= 3 * 1024 * 1024, `oversized response for ${path}`);
      chunks.push(chunk);
    }
    return Buffer.concat(chunks);
  }

  test("public Jobs and shared signup shell use the same released contract", async () => {
    const jobs = (await readPublic("/jobs/")).toString("utf8");
    const jobsRoute = jobsAssetRoute(jobs);
    const bundle = await readPublic(jobsRoute);
    assert.ok(bundle.includes(signupPath), "live Jobs bundle missing the signup link");
    await verifyPublishedSignup(readPublic, await readSignupSource());
    const expectedJobs = await readFile(new URL("../jobs/index.html", import.meta.url), "utf8");
    assert.equal(jobsRoute, jobsAssetRoute(expectedJobs), "public Jobs references a different entry bundle");
    const expectedBundle = await readFile(new URL(`..${jobsRoute}`, import.meta.url));
    assert.equal(hash(bundle), hash(expectedBundle), "public Jobs entry bundle differs from this candidate");
  });
}
