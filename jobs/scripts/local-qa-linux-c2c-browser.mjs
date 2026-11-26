// Browser-only C2C handoff for an already-admitted Linux local-QA context.
// It never creates a browser, server, account, or employer request.
import { observeLocalQaC2cHandoff } from "./local-qa-c2c-handoff.mjs";

const REFUSAL = "local_qa_linux_c2c_browser_refused";
const UNCLOSED = "local_qa_linux_c2c_browser_unclosed";
const EMAIL = "candidate@bluey-qa.test";
const PASSWORD = /^Qa-[0-9a-f]{48}$/;
const ORIGIN = /^http:\/\/127\.0\.0\.1:[1-9][0-9]{3,4}$/;
const MAX_LOGIN_BODY = 1024;
const LOGIN_TIMEOUT = 15_000;

function need(value) { if (!value) throw new Error(REFUSAL); }
function plain(value) { return value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype; }
function exact(value, names) { return plain(value) && Object.keys(value).sort().join(",") === names; }
function localOrigin(value) {
  need(typeof value === "string" && ORIGIN.test(value));
  const parsed = new URL(value);
  need(parsed.origin === value && parsed.username === "" && parsed.password === "" && Number(parsed.port) !== 5187);
  return value;
}
function credentials(value) {
  need(exact(value, "email,password") && value.email === EMAIL && typeof value.password === "string" && PASSWORD.test(value.password));
  return Object.freeze({ email: value.email, password: value.password });
}
function cancel(signal) { signal?.throwIfAborted(); }

async function exactLoginRequest(request, origin, candidate) {
  const target = new URL(request.url());
  need(target.origin === origin && target.username === "" && target.password === "" && target.pathname === "/auth/login"
    && target.search === "" && request.method() === "POST");
  const headers = typeof request.allHeaders === "function" ? await request.allHeaders() : request.headers();
  need(plain(headers) && typeof headers["content-type"] === "string"
    && /^application\/json(?:\s*;|$)/i.test(headers["content-type"])
    && headers.authorization === undefined && headers.cookie === undefined);
  const raw = request.postData();
  need(typeof raw === "string" && Buffer.byteLength(raw, "utf8") > 0 && Buffer.byteLength(raw, "utf8") <= MAX_LOGIN_BODY);
  const body = JSON.parse(raw);
  need(exact(body, "email,password") && body.email === candidate.email && body.password === candidate.password);
}

async function freshContext(context) {
  need(context && typeof context === "object" && typeof context.pages === "function" && typeof context.newPage === "function"
    && typeof context.route === "function" && typeof context.routeWebSocket === "function" && typeof context.close === "function"
    && typeof context.storageState === "function" && context.pages().length === 0);
  const state = await context.storageState();
  need(plain(state) && Array.isArray(state.cookies) && Array.isArray(state.origins) && state.cookies.length === 0 && state.origins.length === 0);
  if (typeof context.serviceWorkers === "function") need(Array.isArray(context.serviceWorkers()) && context.serviceWorkers().length === 0);
}

function jobsLanding(value, origin) {
  const target = new URL(value);
  return target.origin === origin && (target.pathname === "/jobs" || target.pathname === "/jobs/")
    && target.search === "" && target.hash === "";
}

function delegatedContext(context, closeOnce) {
  return Object.freeze({
    pages: () => context.pages(),
    route: (...args) => context.route(...args),
    routeWebSocket: (...args) => context.routeWebSocket(...args),
    close: closeOnce,
  });
}

/**
 * Signs the synthetic candidate in through the current root account form, then
 * delegates the C2C employer-handoff assertion to the existing witness.  The
 * caller must have independently admitted the browser and denied service workers.
 */
export async function exerciseLocalQaLinuxC2cBrowser({ context, origin, credentials: supplied, preparedDraft, employerUrl,
  assertOwned, readCensus, readWorkspace, readMailAttempts, signal }) {
  let closePromise;
  const closeOnce = () => closePromise ??= Promise.resolve().then(() => context.close());
  try {
    need((signal === undefined || signal instanceof AbortSignal) && typeof assertOwned === "function"
      && typeof readCensus === "function" && typeof readWorkspace === "function" && typeof readMailAttempts === "function");
    const local = localOrigin(origin), candidate = credentials(supplied);
    let rejected = 0, webSockets = 0, loginAttempts = 0;
    cancel(signal);
    await freshContext(context);
    need(await assertOwned() === true);
    await context.routeWebSocket("**/*", async socket => {
      webSockets++;
      try { await socket.close(); } catch {}
    });
    await context.route("**/*", async route => {
      try {
        const request = route.request(), target = new URL(request.url());
        if (target.username || target.password || target.origin !== local) { rejected++; return await route.abort("blockedbyclient"); }
        if (request.method() === "GET") return await route.fallback();
        need(loginAttempts++ === 0);
        await exactLoginRequest(request, local, candidate);
        return await route.fallback();
      } catch { rejected++; return route.abort("blockedbyclient"); }
    });
    cancel(signal);
    const page = await context.newPage();
    need(context.pages().length === 1 && context.pages()[0] === page);
    await page.goto(`${local}/login?next=%2Fjobs`, { waitUntil: "domcontentloaded", timeout: LOGIN_TIMEOUT });
    await page.locator("#accountEmail").fill(candidate.email, { timeout: LOGIN_TIMEOUT });
    await page.locator("#accountPassword").fill(candidate.password, { timeout: LOGIN_TIMEOUT });
    const navigation = page.waitForURL(value => jobsLanding(value, local), { timeout: LOGIN_TIMEOUT });
    try {
      await page.locator("#accountPrimaryButton").click({ timeout: LOGIN_TIMEOUT });
      await navigation;
    } catch (error) {
      void navigation.catch(() => {});
      throw error;
    }
    await page.goto(`${local}/jobs/applications`, { waitUntil: "domcontentloaded", timeout: LOGIN_TIMEOUT });
    cancel(signal);
    need(page.url() === `${local}/jobs/applications` && context.pages().length === 1 && context.pages()[0] === page);
    if (typeof context.serviceWorkers === "function") need(Array.isArray(context.serviceWorkers()) && context.serviceWorkers().length === 0);
    need(await assertOwned() === true);
    need(loginAttempts === 1 && rejected === 0 && webSockets === 0);
    const rows = page.locator(".application-list .application-row");
    await rows.first().waitFor({ state: "visible", timeout: LOGIN_TIMEOUT });
    need(await rows.count() === 1);
    await rows.first().click({ timeout: LOGIN_TIMEOUT });
    need(page.url() === `${local}/jobs/applications`);
    // The delegated close is idempotent here, so the witness owns its documented
    // close-on-all-paths behavior without a second physical context close.
    return await observeLocalQaC2cHandoff({ context: delegatedContext(context, closeOnce), page, origin: local,
      preparedDraft, employerUrl, assertOwned, readCensus, readWorkspace, readMailAttempts }).then(result => {
      need(rejected === 0 && webSockets === 0);
      return result;
    });
  } catch {
    try { await closeOnce(); } catch { throw new Error(UNCLOSED); }
    throw new Error(REFUSAL);
  }
}
