// Linux-only, synthetic candidate signup around the generic local-QA gateway.
// This is deliberately a one-context lifetime: it is not a reusable portal or
// a general authenticated browser proxy.
import { createHash } from "node:crypto";
import { startLocalQaGateway } from "./local-qa-gateway.mjs";
import { loadManifest } from "./local-qa-linux-c2c-restart-gateway.mjs";
import { completeLocalQaBrowserSignup } from "./local-qa-browser-signup.mjs";
import { completeLocalQaBrowserSampleResume, LOCAL_QA_SAMPLE_RESUME }
  from "./local-qa-browser-sample-resume.mjs";

const REFUSED = "local_qa_linux_signup_gateway_refused";
const UNCLOSED = "local_qa_linux_signup_gateway_unclosed";
const EMAIL = "candidate@bluey-qa.test";
const PASSWORD = /^Qa-[0-9a-f]{48}$/;
const OTP = /^[0-9]{6}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const ORIGIN = /^http:\/\/127\.0\.0\.1:[1-9][0-9]{3,4}$/;
const BEARER = /^[A-Za-z0-9_-]{1,1024}\.[A-Za-z0-9_-]{1,2048}\.[A-Za-z0-9_-]{1,2048}$/;
const MAX_BODY = 1024;
const MAX_SAMPLE_UPLOAD = 16 * 1024;
const RESUME_SOURCE = "/api/jobs/resume-source";
const RESUME_VERSION = "/api/jobs/resume-versions/";
const PREPARE = "/api/jobs/applications";
const COMMUNICATION = "/api/jobs/communication-actions";
const MAX_FINGERPRINT = 512;
const POST_SIGNUP_READS = new Set(["/account/me", "/api/jobs/beta-access", "/api/jobs/workspace", "/api/jobs/taxonomy"]);

function need(value) { if (!value) throw new Error(REFUSED); }
function plain(value) { return value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype; }
function exact(value, names) { return plain(value) && Object.keys(value).sort().join(",") === names; }
function cancel(signal) { signal?.throwIfAborted(); }

function localOrigin(origin) {
  need(typeof origin === "string" && ORIGIN.test(origin));
  const parsed = new URL(origin);
  need(parsed.origin === origin && parsed.username === "" && parsed.password === "" && Number(parsed.port) !== 5187);
  return origin;
}

function browserBinding(value, backendPort) {
  need(exact(value, "openContext,port,webDirectory") && Number.isSafeInteger(value.port)
    && value.port > 1024 && value.port <= 65535 && ![5187, 5432, backendPort].includes(value.port)
    && typeof value.webDirectory === "string" && value.webDirectory.startsWith("/") && value.webDirectory !== "/"
    && !/[\0\r\n]/.test(value.webDirectory) && typeof value.openContext === "function");
  return Object.freeze({ ...value });
}

function binding(value) {
  need(plain(value) && typeof value.manifestFile === "string" && value.manifestFile.startsWith("/")
    && !/[\0\r\n]/.test(value.manifestFile) && typeof value.manifestSha256 === "string"
    && /^[0-9a-f]{64}$/.test(value.manifestSha256));
  return Object.freeze({ manifestFile: value.manifestFile, manifestSha256: value.manifestSha256 });
}

async function freshContext(context) {
  need(context && typeof context === "object" && typeof context.pages === "function" && typeof context.newPage === "function"
    && typeof context.route === "function" && typeof context.routeWebSocket === "function" && typeof context.close === "function"
    && typeof context.storageState === "function" && context.pages().length === 0);
  const state = await context.storageState();
  need(plain(state) && Array.isArray(state.cookies) && Array.isArray(state.origins)
    && state.cookies.length === 0 && state.origins.length === 0);
  if (typeof context.serviceWorkers === "function") {
    need(Array.isArray(context.serviceWorkers()) && context.serviceWorkers().length === 0);
  }
}

async function requestHeaders(request) {
  const headers = typeof request.allHeaders === "function" ? await request.allHeaders() : request.headers();
  need(plain(headers) && !headers.authorization && !headers.cookie && !headers["transfer-encoding"]
    && !headers["content-encoding"] && typeof headers["content-type"] === "string"
    && /^application\/json(?:\s*;|$)/i.test(headers["content-type"]));
  return headers;
}

function requestJson(request, sizeLimit = MAX_BODY) {
  const raw = request.postData();
  need(typeof raw === "string" && Buffer.byteLength(raw, "utf8") > 0 && Buffer.byteLength(raw, "utf8") <= sizeLimit);
  const value = JSON.parse(raw);
  need(Buffer.from(JSON.stringify(value)).equals(Buffer.from(raw)));
  return value;
}

function requireCandidateHeaders(headers) {
  need(plain(headers) && typeof headers.authorization === "string" && headers.authorization.startsWith("Bearer ")
    && BEARER.test(headers.authorization.slice(7)) && !headers.cookie
    && !headers["transfer-encoding"] && !headers["content-encoding"]);
}

async function validateSampleUpload(request, seen) {
  need(seen.sampleReady && seen.uploads++ === 0);
  const headers = typeof request.allHeaders === "function" ? await request.allHeaders() : request.headers();
  requireCandidateHeaders(headers);
  need(headers["content-type"] === "application/json");
  const body = requestJson(request, MAX_SAMPLE_UPLOAD);
  const { fileName, mimeType, candidateEmail, name, headline, text } = LOCAL_QA_SAMPLE_RESUME;
  need(exact(body, "bytes_base64,file_name,media_type,profile,request_id")
    && typeof body.request_id === "string" && UUID.test(body.request_id)
    && body.file_name === fileName && body.media_type === mimeType
    && body.bytes_base64 === Buffer.from(text, "utf8").toString("base64")
    && plain(body.profile) && body.profile.full_name === name && body.profile.email === candidateEmail
    && body.profile.headline === headline && body.profile.onboarding_complete === false);
  if (seen.includeMockReceipt) seen.sampleBearerHash = bearerHash(headers.authorization);
  seen.uploadAdmitted = true;
}

function bearerHash(value) { return createHash("sha256").update(value).digest("hex"); }

export function requireLocalQaSampleResumeReadback({ asset, profile, assetId, onboardingComplete = false }) {
  const { fileName, mimeType, candidateEmail, name, headline, sha256, text } = LOCAL_QA_SAMPLE_RESUME;
  need(typeof assetId === "string" && UUID.test(assetId) && plain(asset) && asset.id === assetId
    && asset.file_name === fileName && asset.media_type === mimeType && asset.sha256 === sha256
    && asset.size_bytes === Buffer.byteLength(text, "utf8")
    && plain(profile) && profile.source_resume_asset_id === assetId && profile.source_resume_sha256 === sha256
    && profile.source_resume_name === fileName && profile.source_resume_media_type === mimeType
    && profile.full_name === name && profile.email === candidateEmail && profile.headline === headline
    && typeof onboardingComplete === "boolean" && profile.onboarding_complete === onboardingComplete);
}

async function validateSignupWrite(request, origin, password, seen) {
  const target = new URL(request.url());
  need(target.origin === origin && target.username === "" && target.password === "" && target.search === ""
    && request.method() === "POST");
  await requestHeaders(request);
  const value = requestJson(request);
  if (target.pathname === "/auth/signup/start") {
    need(seen.start++ === 0 && exact(value, "device_fingerprint,email,password,terms_accepted,turnstile_token")
      && value.email === EMAIL && value.password === password && value.terms_accepted === true
      && value.turnstile_token === null && typeof value.device_fingerprint === "string"
      && value.device_fingerprint.length > 0 && value.device_fingerprint.length <= MAX_FINGERPRINT
      && !/[\0\r\n]/.test(value.device_fingerprint));
    seen.fingerprint = value.device_fingerprint;
    return;
  }
  need(target.pathname === "/auth/signup/confirm" && seen.confirm++ === 0
    && exact(value, "device_fingerprint,email,otp") && value.email === EMAIL && OTP.test(value.otp)
    && value.otp === seen.otp && value.device_fingerprint === seen.fingerprint);
  seen.confirmAdmitted = true;
  return "confirm";
}

async function safeGet(request, origin, manifest, seen) {
  const target = new URL(request.url());
  need(target.origin === origin && target.username === "" && target.password === "" && request.method() === "GET");
  const path = target.pathname;
  if (path.startsWith(RESUME_VERSION)) {
    need(seen.mockReceiptReadOpen && seen.receiptReads === 0
      && request.url() === `${origin}${RESUME_VERSION}${seen.preparedResumeId}`);
    const headers = typeof request.allHeaders === "function" ? await request.allHeaders() : request.headers();
    requireCandidateHeaders(headers);
    need(bearerHash(headers.authorization) === seen.sampleBearerHash
      && (request.postData() === null || request.postData() === ""));
    need(seen.receiptReads === 0);
    seen.receiptReads++;
    return Object.freeze({ kind: "prepared_resume", token: headers.authorization.slice(7) });
  }
  if (seen.jobId && (path === "/jobs/matches" || path === "/jobs/applications")) {
    need(target.search === "");
    return;
  }
  if (seen.preparedApplicationId && path === COMMUNICATION) {
    const query = `?limit=100&application_id=${seen.preparedApplicationId}`;
    need(target.search === "?limit=100" || target.search === query);
    const headers = typeof request.allHeaders === "function" ? await request.allHeaders() : request.headers();
    requireCandidateHeaders(headers);
    need(request.postData() === null || request.postData() === "");
    return target.search === query ? "application_communications" : "all_communications";
  }
  if (POST_SIGNUP_READS.has(path)) {
    need(seen.confirmForwarded === true && target.search === "");
    const headers = typeof request.allHeaders === "function" ? await request.allHeaders() : request.headers();
    requireCandidateHeaders(headers);
    need((headers["content-type"] === undefined || /^application\/json(?:\s*;\s*charset=utf-8)?$/i.test(headers["content-type"]))
      && (request.postData() === null || request.postData() === ""));
    return;
  }
  if (path === "/auth/captcha/config") {
    need(target.search === "");
    return;
  }
  if (path === "/" || path === "/jobs" || path === "/jobs/") {
    need(target.search === "");
    return;
  }
  if (path === "/login") {
    need(target.search === "?mode=signup&next=%2Fjobs" || target.search === "?mode=signup&next=%2Fjobs%2F");
    return;
  }
  const digest = manifest[path];
  need(typeof digest === "string" && /^[0-9a-f]{64}$/.test(digest));
  need(target.search === "" || (path.startsWith("/assets/")
    && (/^\?v=[0-9]{1,20}$/.test(target.search) || target.search === `?v=${digest.slice(0, 12)}`)));
}

// A deliberately small request-policy seam for the Node-only control tests.
// It cannot open a listener, context, or browser; the runtime below remains
// the only place that forwards an admitted request.
export function createLocalQaLinuxSignupRequestPolicy({ origin, password, manifest, includeSampleResume = false,
  includePrepare = false, includeMockReceipt = false }) {
  const local = localOrigin(origin);
  need(typeof password === "string" && PASSWORD.test(password) && plain(manifest)
    && typeof includeSampleResume === "boolean" && typeof includePrepare === "boolean"
    && typeof includeMockReceipt === "boolean" && (!includePrepare || includeSampleResume)
    && (!includeMockReceipt || includePrepare));
  const seen = { start: 0, confirm: 0, confirmAdmitted: false, confirmForwarded: false,
    fingerprint: undefined, otp: undefined, sampleReady: false, uploads: 0, uploadAdmitted: false,
    jobId: undefined, prepareAttempts: 0, prepareAdmitted: false, preparedApplicationId: undefined,
    includeMockReceipt, sampleBearerHash: undefined, preparedResumeId: undefined,
    mockReceiptReadOpen: false, receiptReads: 0 };
  return Object.freeze({
    async authorize(request) {
      need(request && typeof request.method === "function");
      if (request.method() === "GET") {
        return safeGet(request, local, manifest, seen);
      }
      if (request.url() === `${local}${RESUME_SOURCE}` && request.method() === "POST") {
        await validateSampleUpload(request, seen);
        return undefined;
      }
      if (request.url() === `${local}${PREPARE}` && request.method() === "POST") {
        need(includePrepare && seen.jobId && seen.prepareAttempts++ === 0);
        const headers = typeof request.allHeaders === "function" ? await request.allHeaders() : request.headers();
        requireCandidateHeaders(headers);
        need(headers["content-type"] === "application/json");
        const body = requestJson(request);
        need(exact(body, "job_id,mode,submission_mode") && body.job_id === seen.jobId
          && body.mode === "factual" && body.submission_mode === "review_first");
        seen.prepareAdmitted = true;
        return "prepare";
      }
      return validateSignupWrite(request, local, password, seen);
    },
    acceptOtp(code) {
      need(seen.start === 1 && seen.confirm === 0 && seen.otp === undefined && typeof code === "string" && OTP.test(code));
      seen.otp = code;
      return code;
    },
    forwarded(kind) {
      need(kind === "confirm" && seen.confirmAdmitted && !seen.confirmForwarded);
      seen.confirmForwarded = true;
    },
    beginSampleImport() {
      need(includeSampleResume && seen.confirmForwarded && !seen.sampleReady && seen.uploads === 0);
      seen.sampleReady = true;
    },
    beginPrepare(jobId) {
      need(includePrepare && seen.uploadAdmitted && seen.jobId === undefined
        && typeof jobId === "string" && /^[A-Za-z0-9_-]{1,160}$/.exec(jobId)?.[0] === jobId);
      seen.jobId = jobId;
    },
    finishPrepare(applicationId, resumeId) {
      need(seen.prepareAdmitted && seen.preparedApplicationId === undefined
        && typeof applicationId === "string" && UUID.test(applicationId)
        && (!includeMockReceipt || (typeof resumeId === "string" && UUID.test(resumeId))));
      seen.preparedApplicationId = applicationId;
      if (includeMockReceipt) seen.preparedResumeId = resumeId;
    },
    beginMockReceiptRead(resumeId) {
      need(includeMockReceipt && !seen.mockReceiptReadOpen && seen.preparedApplicationId !== undefined
        && typeof seen.preparedResumeId === "string" && resumeId === seen.preparedResumeId
        && typeof seen.sampleBearerHash === "string");
      seen.mockReceiptReadOpen = true;
    },
    complete() {
      need(seen.start === 1 && seen.confirm === 1 && seen.confirmForwarded === true
        && seen.uploads === (includeSampleResume ? 1 : 0) && seen.uploadAdmitted === includeSampleResume
        && seen.prepareAttempts === (includePrepare ? 1 : 0) && seen.prepareAdmitted === includePrepare
        && (seen.preparedApplicationId !== undefined) === includePrepare
        && seen.receiptReads === (includeMockReceipt ? 1 : 0));
    },
  });
}

async function closeAll(context, gateway) {
  let unclosed = false;
  try { if (context) await context.close(); } catch { unclosed = true; }
  try { if (gateway) await gateway.close(); } catch { unclosed = true; }
  if (unclosed) throw new Error(UNCLOSED);
}

/**
 * Exercise precisely one owned candidate signup through a fresh, Linux-native
 * browser context. The token remains in the page; callers receive only the
 * synthetic account ID and fixed counters.
 */
export async function runLocalQaLinuxBrowserSignup({ browser, buildBinding, backendPort, password,
  takeSignupCode, assertOwned, signal, includeSampleResume = false, setupPrepare, setupMockReceipt }) {
  let gateway, context;
  try {
    cancel(signal);
    need(Number.isSafeInteger(backendPort) && backendPort > 1024 && backendPort <= 65535
      && typeof password === "string" && PASSWORD.test(password) && typeof takeSignupCode === "function"
      && typeof assertOwned === "function" && await assertOwned() === true);
    const includePrepare = setupPrepare !== undefined;
    const includeMockReceipt = setupMockReceipt !== undefined;
    need(!includePrepare || (typeof setupPrepare === "function" && includeSampleResume));
    need(!includeMockReceipt || (includePrepare && typeof setupMockReceipt === "function"));
    const nativeBrowser = browserBinding(browser, backendPort);
    const measured = binding(buildBinding);
    const manifest = await loadManifest(measured.manifestFile, measured.manifestSha256);
    cancel(signal);
    need(await assertOwned() === true);
    gateway = await startLocalQaGateway({
      webDirectory: nativeBrowser.webDirectory,
      manifest,
      backendPort,
      port: nativeBrowser.port,
      verifyBackend: async target => {
        cancel(signal);
        need(plain(target) && Object.keys(target).sort().join(",") === "host,port"
          && target.host === "127.0.0.1" && target.port === backendPort && await assertOwned() === true);
        cancel(signal);
        return true;
      },
    });
    const origin = localOrigin(gateway.origin);
    need(origin === `http://127.0.0.1:${nativeBrowser.port}` && await assertOwned() === true);
    cancel(signal);
    context = await nativeBrowser.openContext(Object.freeze({ origin, serviceWorkers: "block", signal }));
    await freshContext(context);
    let rejected = 0, webSockets = 0;
    const policy = createLocalQaLinuxSignupRequestPolicy({
      origin, password, manifest, includeSampleResume, includePrepare, includeMockReceipt,
    });
    let prepare, preparedApplicationId, mockReceiptOperations, preparedResumeId;
    await context.routeWebSocket("**/*", async socket => {
      webSockets++;
      try { await socket.close(); } catch {}
    });
    await context.route("**/*", async route => {
      try {
        cancel(signal);
        const request = route.request();
        const write = await policy.authorize(request);
        cancel(signal);
        if (write === "prepare") {
          const result = await prepare.prepareDraft();
          cancel(signal);
          need(plain(result) && result.application?.job_id === prepare.jobId
            && result.application.state === "awaiting_review" && result.application.submission_mode === "review_first"
            && result.application.submitted_at_ms === null && result.application.run_id === null
            && result.application.receipt?.confirmation === null && !Object.hasOwn(result.application.receipt, "approved_execution")
            && result.metering == null && await assertOwned() === true);
          preparedApplicationId = result.application.id;
          policy.finishPrepare(preparedApplicationId, result.resume_version?.id);
          await route.fulfill({ status: 200, json: result });
          return;
        }
        if (write?.kind === "prepared_resume") {
          need(mockReceiptOperations !== undefined && typeof preparedResumeId === "string"
            && await assertOwned() === true);
          const resume = await mockReceiptOperations.readBoundResume(Object.freeze({
            resumeId: preparedResumeId, token: write.token,
          }));
          cancel(signal);
          need(plain(resume) && resume.id === preparedResumeId && resume.mode === "factual"
            && resume.job_id === prepare.jobId && await assertOwned() === true);
          await route.fulfill({ status: 200, json: resume, headers: { "cache-control": "private, no-store" } });
          return;
        }
        if (write === "all_communications" || write === "application_communications") {
          const list = await prepare.readEmptyCommunicationActions(write === "application_communications" ? preparedApplicationId : undefined);
          cancel(signal);
          need(Array.isArray(list) && list.length === 0 && await assertOwned() === true);
          await route.fulfill({ status: 200, json: list });
          return;
        }
        // The browser can schedule its next read before the continuation IPC
        // resolves. Mark the exact admitted confirm as forwarding first; a
        // failed continuation is still a rejected request and fails the run.
        if (write === "confirm") policy.forwarded(write);
        await route.fallback();
      } catch {
        rejected++;
        try { await route.abort("blockedbyclient"); } catch {}
      }
    });
    const page = await context.newPage();
    need(context.pages().length === 1 && context.pages()[0] === page);
    const takeExactSignupCode = async () => {
      const code = await takeSignupCode();
      return policy.acceptOtp(code);
    };
    const result = await completeLocalQaBrowserSignup(Object.freeze({ page, origin, password, takeSignupCode: takeExactSignupCode }));
    let uploaded, prepared, mockReceipt;
    if (includeSampleResume) {
      cancel(signal);
      need(await assertOwned() === true);
      policy.beginSampleImport();
      uploaded = await completeLocalQaBrowserSampleResume(Object.freeze({ page, origin }));
      need(exact(uploaded, "assetId") && UUID.test(uploaded.assetId));
      if (includePrepare) {
        prepare = await setupPrepare(Object.freeze({ accountId: result.accountId, assetId: uploaded.assetId }));
        need(exact(prepare, "jobId,prepareDraft,readEmptyCommunicationActions")
          && typeof prepare.prepareDraft === "function" && typeof prepare.readEmptyCommunicationActions === "function");
        cancel(signal);
        need(await assertOwned() === true);
        cancel(signal);
        policy.beginPrepare(prepare.jobId);
        // Reload the signed-in workspace after the independent setup saved
        // onboarding and the synthetic match. The pre-import React snapshot
        // must not stand in for a fresh authenticated read.
        await page.goto(`${origin}/jobs/matches`, { waitUntil: "domcontentloaded", timeout: 8_000 });
        const { completeLocalQaBrowserSamplePrepare } = await import("./local-qa-browser-sample-prepare.mjs");
        prepared = await completeLocalQaBrowserSamplePrepare({ page, origin, jobId: prepare.jobId, assetId: uploaded.assetId });
        need(exact(prepared, "applicationId,resumeId") && prepared.applicationId === preparedApplicationId
          && UUID.test(prepared.resumeId));
        if (includeMockReceipt) {
          const draft = Object.freeze({ ...prepared, accountId: result.accountId, jobId: prepare.jobId,
            sourceAssetId: uploaded.assetId, sourceSha256: LOCAL_QA_SAMPLE_RESUME.sha256 });
          preparedResumeId = draft.resumeId;
          cancel(signal);
          need(await assertOwned() === true);
          const operations = await setupMockReceipt(draft);
          need(exact(operations, "readBoundResume,readOwnedState,runMockWorker")
            && Object.values(operations).every(fn => typeof fn === "function"));
          mockReceiptOperations = Object.freeze({ ...operations });
          const { createOwnedQaMockReceiptCheckpoint } = await import("./local-qa-mock-receipt-checkpoint.mjs");
          const { observeLocalQaBrowserMockReceipt } = await import("./local-qa-browser-mock-receipt.mjs");
          const checkpoint = createOwnedQaMockReceiptCheckpoint({
            assertOwned: async () => {
              cancel(signal);
              return context.pages().length === 1 && context.pages()[0] === page
                && page.url() === `${origin}/jobs/applications` && await assertOwned() === true;
            },
            readOwnedState: mockReceiptOperations.readOwnedState,
            runMockWorker: mockReceiptOperations.runMockWorker,
            observeInBrowser: expected => {
              cancel(signal);
              policy.beginMockReceiptRead(expected.resumeId);
              return observeLocalQaBrowserMockReceipt({ page, origin, expected });
            },
          });
          mockReceipt = await checkpoint(draft);
        }
      }
    }
    cancel(signal);
    policy.complete();
    need(exact(result, "accountId") && UUID.test(result.accountId)
      && rejected === 0 && webSockets === 0 && context.pages().length === 1 && context.pages()[0] === page
      && await assertOwned() === true);
    if (typeof context.serviceWorkers === "function") {
      need(Array.isArray(context.serviceWorkers()) && context.serviceWorkers().length === 0);
    }
    return Object.freeze({ accountId: result.accountId, ...(uploaded ? { resumeAssetId: uploaded.assetId } : {}),
      ...(prepared ? { prepared: Object.freeze({ ...prepared, jobId: prepare.jobId }) } : {}),
      ...(mockReceipt ? { mockReceipt } : {}) });
  } catch (error) {
    throw new Error(error?.message === UNCLOSED ? UNCLOSED : REFUSED);
  } finally {
    await closeAll(context, gateway);
  }
}
