import assert from "node:assert/strict";
import test from "node:test";
import { observeLocalQaC2cHandoff } from "./local-qa-c2c-handoff.mjs";
import { exerciseLocalQaLinuxC2cBrowser } from "./local-qa-linux-c2c-browser.mjs";
import { bindLocalQaLinuxC2cRestartWorkspace, validateLocalQaLinuxC2cBrowserBinding } from "./local-qa-linux-c2c-restart-gateway.mjs";
import { c2cCensusStatement, parseC2cCensusOutput,
  parseReviewFirstReceiptCensusOutput, reviewFirstReceiptCensusStatement } from "./local-qa-linux-c2c-census.mjs";
import { parseLocalQaLinuxC2cUnitShow, validateLocalQaLinuxC2cUnitShow } from "./local-qa-linux-c2c-unit.mjs";

const origin = "http://127.0.0.1:45931";
const jobId = "123e4567-e89b-42d3-a456-426614174000";
const employerUrl = `https://boards.greenhouse.io/blueynonprod/jobs/durable-c2c-qa-${"a".repeat(32)}`;
const detail = `${origin}/api/jobs/matches/${jobId}`;
const preparedDraft = { applicationId: "123e4567-e89b-42d3-a456-426614174001", fixtureMode: "c2c", jobId,
  resumeId: "123e4567-e89b-42d3-a456-426614174002", jobSnapshotFingerprint: "d".repeat(64) };
const eligibility = { can_prepare: true, can_queue_local: false, can_queue_cloud: false, can_auto_submit: false,
  hard_failures: [], review_reasons: [{ code: "c2c_submission_requires_candidate" }],
  passed_checks: ["employment_type_allowed", "engagement_type_allowed", "track_employment_type_allowed",
    "track_engagement_type_allowed"] };
const accountId = "123e4567-e89b-42d3-a456-426614174003";
const credentials = { email: "candidate@bluey-qa.test", password: `Qa-${"a".repeat(48)}` };

function ownedFixture(override = {}) {
  const effects = [];
  const frame = {};
  let handler, socketHandler, emitMatch, closed = false, reads = 0, workspaceReads = 0;
  let currentUrl = `${origin}/jobs/applications`, pageCreated = !override.fresh, pageLanding;
  const row = { accounts: 1, profiles: 1, applications: 1, prepared: 1, resumes: 1, factual_draft_links: 1,
    submitted: 0, attempts: 0, auto_submit_authorizations: 0, balance_events: 0, browser_sessions: 0,
    commands: 0, communication_actions: 0, execution_leases: 0, interventions: 0, irreversible_receipts: 0,
    metering: 0, run_events: 0, workflow_executions: 0, balance_cents: 0,
    entitlement: { plan: "free", local_browser: 0, cloud_browser: 0, used_packets: 0 } };
  const posting = { id: jobId, employment_type: "contract c2c", canonical_url: employerUrl };
  const draft = { id: preparedDraft.applicationId, job_id: jobId, resume_version_id: preparedDraft.resumeId,
    state: "awaiting_review", submission_mode: "review_first", run_id: null, submitted_at_ms: null,
    receipt: { eligibility, confirmation: null, job_snapshot: posting,
      resume_version_id: preparedDraft.resumeId, job_snapshot_fingerprint: preparedDraft.jobSnapshotFingerprint } };
  const workspace = { applications: [draft], matches: [posting], matches_page: { next_cursor: null },
    browser_sessions: [], auto_submit_authorizations: [], entitlement: { plan: "free", local_browser: false,
      cloud_browser: false, used_packets: 0 } };
  async function emit(url, { method = "GET", navigation = false, body } = {}) {
    await handler({
      request: () => ({ url: () => url, method: () => method, isNavigationRequest: () => navigation, frame: () => frame,
        allHeaders: async () => ({ "content-type": "application/json" }), postData: () => JSON.stringify(body) }),
      abort: async code => { assert.equal(code, "blockedbyclient"); effects.push(["abort", url]); },
      fallback: async () => {
        assert.equal(new URL(url).origin, origin);
        effects.push(["fallback", url]);
        if (url === detail) emitMatch({ url: () => url, request: () => ({ method: () => method }),
          status: () => override.matchStatus ?? 200,
          json: async () => ({ id: jobId, canonical_url: override.matchUrl ?? employerUrl, employment_type: "contract c2c",
            eligibility: { ...eligibility, ...override.freshEligibility } }) });
      },
    });
  }
  const page = { url: () => currentUrl, mainFrame: () => frame,
    async goto(url) {
      currentUrl = url;
      await emit(url, { navigation: true });
      if (override.foreignBeforeLogin && url.includes("/login?")) await emit("https://unexpected.invalid");
    },
    waitForURL: predicate => new Promise(resolve => {
      pageLanding = url => { assert(predicate(new URL(url))); resolve(); };
    }),
    locator(selector) {
      if (selector === "#accountEmail" || selector === "#accountPassword") {
        return { async fill(value) { assert.equal(value, selector === "#accountEmail" ? credentials.email : credentials.password); } };
      }
      if (selector === "#accountPrimaryButton") {
        return { async click() {
          await emit(`${origin}/auth/login`, { method: "POST", body: credentials });
          if (override.repeatLogin) await emit(`${origin}/auth/login`, { method: "POST", body: credentials });
          currentUrl = `${origin}/jobs/`;
          pageLanding(currentUrl);
        } };
      }
      assert.equal(selector, ".application-list .application-row");
      return { count: async () => override.doubleRows ? 2 : 1, first: () => ({
        async waitFor() {}, async click() { effects.push(["saved-row"]); },
      }) };
    },
    waitForResponse: (predicate, { timeout }) => new Promise(resolve => {
      assert.equal(timeout, 10000);
      emitMatch = response => { if (predicate(response)) resolve(response); };
    }), getByRole(role) {
    assert.equal(role, "dialog");
    return {
      getByText(name, options) {
        assert.equal(name, "You submit this application"); assert.deepEqual(options, { exact: true });
        return { waitFor: async () => { effects.push(["rendered"]); } };
      },
      getByRole(name, options) {
        assert.equal(name, "button");
        assert.deepEqual(options, { name: "Open employer site to apply", exact: true });
        return { async click() {
          if (override.clickError) throw new Error("synthetic click refused");
          if (override.earlyDeparture) await emit(employerUrl, { navigation: true });
          await emit(detail);
          if (override.duplicateDetail) await emit(detail);
          if (override.write) await emit(`${origin}/api/jobs/applications`, { method: "POST" });
          if (override.socket) socketHandler({ close() { effects.push(["socket_closed"]); } });
          if (!override.earlyDeparture) await emit(employerUrl, { navigation: true });
          if (override.foreign) await emit("https://unexpected.invalid");
        } };
      },
    };
  } };
  const context = { pages: () => pageCreated ? [page] : [],
    storageState: async () => ({ origins: [], cookies: override.dirtyContext ? [{}] : [] }),
    serviceWorkers: () => [],
    async newPage() { pageCreated = true; return page; },
    async route(pattern, callback) { assert.equal(pattern, "**/*"); handler = callback; },
    async routeWebSocket(pattern, callback) { assert.equal(pattern, "**/*"); socketHandler = callback; },
    async close() { closed = true; effects.push(["closed"]); if (override.closeFails) throw new Error("unclosed fixture"); },
  };
  const input = { context, page, origin, preparedDraft, employerUrl, assertOwned: async () => true,
    async readCensus() {
      if (++reads > 1) {
        assert(closed);
        if (override.balanceDrift) row.balance_cents++;
        if (override.queueDrift) row.attempts++;
      }
      return row;
    },
    async readWorkspace() {
      if (++workspaceReads > 1) {
        assert(closed);
        if (override.appDrift) draft.updated_at_ms = 10;
      }
      return workspace;
    },
    readMailAttempts: async () => override.mail && closed ? 1 : 0,
  };
  return { input, effects, workspace, row };
}

test("only the exact fresh match is forwarded and the employer navigation is cancelled before census", async () => {
  const { input, effects } = ownedFixture();
  assert.deepEqual(await observeLocalQaC2cHandoff(input),
    { matchReads: 1, navigationsAborted: 1, mutations: 0, unexpected: 0, stableCensus: true });
  assert.deepEqual(effects, [["rendered"], ["fallback", detail], ["abort", employerUrl], ["closed"]]);
});

test("Linux saved browser uses the current sign-in and one persisted row before the canceled handoff", async () => {
  const { input, effects } = ownedFixture({ fresh: true });
  const { page: _page, ...browserInput } = input;
  assert.deepEqual(await exerciseLocalQaLinuxC2cBrowser({ ...browserInput, credentials }),
    { matchReads: 1, navigationsAborted: 1, mutations: 0, unexpected: 0, stableCensus: true });
  assert.deepEqual(effects, [
    ["fallback", `${origin}/login?next=%2Fjobs`], ["fallback", `${origin}/auth/login`],
    ["fallback", `${origin}/jobs/applications`], ["saved-row"], ["rendered"], ["fallback", detail],
    ["abort", employerUrl], ["closed"],
  ]);
});

test("Linux saved browser refuses pre-existing auth, repeated login, foreign traffic and multiple rows", async () => {
  for (const override of [{ dirtyContext: true }, { repeatLogin: true }, { foreignBeforeLogin: true }, { doubleRows: true }]) {
    const { input, effects } = ownedFixture({ fresh: true, ...override });
    await assert.rejects(exerciseLocalQaLinuxC2cBrowser({ ...input, credentials }), /c2c_browser_refused/);
    assert.equal(effects.filter(([kind, url]) => kind === "fallback" && url === `${origin}/auth/login`).length,
      override.dirtyContext ? 0 : 1);
    assert.equal(effects.filter(([kind]) => kind === "closed").length, 1);
    assert(!effects.some(([, url]) => url === detail || url === employerUrl));
  }
  const { input } = ownedFixture({ fresh: true, closeFails: true });
  await assert.rejects(exerciseLocalQaLinuxC2cBrowser({ ...input, credentials }), /c2c_browser_unclosed/);
});

test("Linux restart gateway binds only the same draft, résumé and synthetic employer", () => {
  const { workspace } = ownedFixture();
  assert.deepEqual(bindLocalQaLinuxC2cRestartWorkspace(accountId, workspace, preparedDraft).readback,
    { accountId, resumeId: preparedDraft.resumeId, c2cApplicationId: preparedDraft.applicationId, c2cMatchId: jobId });
  const mismatch = structuredClone(workspace);
  mismatch.matches[0].canonical_url = "https://another.invalid";
  assert.throws(() => bindLocalQaLinuxC2cRestartWorkspace(accountId, mismatch, preparedDraft));
  assert.throws(() => bindLocalQaLinuxC2cRestartWorkspace(accountId, workspace, { ...preparedDraft, resumeId: accountId }));
  assert.throws(() => validateLocalQaLinuxC2cBrowserBinding({ openContext: async () => null, port: 43107,
    webDirectory: "/run/admitted/web" }, [43107]));
});

test("Linux census binds one canonical candidate on the fixed read-only schema", () => {
  const { row } = ownedFixture();
  const sql = c2cCensusStatement(accountId);
  assert(sql.startsWith("SELECT json_build_object(") && sql.includes("public.jobs_managed_cloud_irreversible_effect_receipts"));
  assert.throws(() => c2cCensusStatement(`${accountId}\n`), /C2C_CENSUS_REFUSED/);
  assert.deepEqual(parseC2cCensusOutput(JSON.stringify(row)), row);
  assert.throws(() => parseC2cCensusOutput(JSON.stringify({ ...row, entitlement: null })), /C2C_CENSUS_REFUSED/);
  assert.throws(() => parseC2cCensusOutput(JSON.stringify({ ...row, leaked: "fixture" })), /C2C_CENSUS_REFUSED/);
});

test("receipt census is separate from free draft census and requires one included exact attempt", () => {
  const { row } = ownedFixture();
  const prepared = { ...row, application_evidence: 0, evidence_total: 0, receipt_uploads_total: 0,
    ready_receipt_uploads: 0, capacity_total: 0, committed_capacity: 0,
    entitlement: { plan: "cloud", local_browser: 1, cloud_browser: 1, used_packets: 0 } };
  const submitted = { ...prepared, prepared: 0, factual_draft_links: 0, submitted: 1,
    attempts: 1, execution_leases: 1, browser_sessions: 1, metering: 1,
    application_evidence: 3, evidence_total: 3, receipt_uploads_total: 3,
    ready_receipt_uploads: 3, capacity_total: 1, committed_capacity: 1,
    entitlement: { ...prepared.entitlement, used_packets: 1 } };
  assert.deepEqual(parseReviewFirstReceiptCensusOutput(JSON.stringify(prepared), "prepared"), prepared);
  assert.deepEqual(parseReviewFirstReceiptCensusOutput(JSON.stringify(submitted), "submitted"), submitted);
  assert.throws(() => parseC2cCensusOutput(JSON.stringify(prepared)), /C2C_CENSUS_REFUSED/);
  assert.throws(() => parseReviewFirstReceiptCensusOutput(JSON.stringify(row), "prepared"), /RECEIPT_CENSUS_REFUSED/);
  assert.throws(() => parseReviewFirstReceiptCensusOutput(JSON.stringify(prepared), "submitted"), /RECEIPT_CENSUS_REFUSED/);
  assert.throws(() => parseReviewFirstReceiptCensusOutput(JSON.stringify(submitted), "prepared"), /RECEIPT_CENSUS_REFUSED/);
  for (const alteration of [{ attempts: 2 }, { metering: 0 }, { balance_events: 1 }, { workflow_executions: 1 },
    { auto_submit_authorizations: 1 }, { communication_actions: 1 }, { entitlement: prepared.entitlement },
    { application_evidence: 2 }, { evidence_total: 4 }, { receipt_uploads_total: 4 },
    { ready_receipt_uploads: 2 }, { capacity_total: 2 }, { committed_capacity: 0 }]) {
    assert.throws(() => parseReviewFirstReceiptCensusOutput(JSON.stringify({ ...submitted, ...alteration }), "submitted"),
      /RECEIPT_CENSUS_REFUSED/);
  }
});

test("receipt SQL measures application/run evidence without changing the legacy query", () => {
  const applicationId = "12558586-faef-4082-a8eb-15749c749a35";
  const runId = "run-synthetic";
  const draft = reviewFirstReceiptCensusStatement(accountId, applicationId, "prepared", null);
  const final = reviewFirstReceiptCensusStatement(accountId, applicationId, "submitted", runId);
  assert(draft.includes("run_id=NULL"));
  assert(final.includes(`account_id='${accountId}' AND application_id='${applicationId}' AND run_id='${runId}'`));
  assert(final.includes("state='committed' AND consumed_bytes > 0 AND consumed_objects=3"));
  assert(final.includes("public.jobs_application_evidence") && final.includes("public.object_uploads"));
  assert(!c2cCensusStatement(accountId).includes("public.jobs_application_evidence"));
  for (const args of [
    [accountId, applicationId, "prepared", runId], [accountId, applicationId, "submitted", null],
    [accountId, applicationId, "submitted", "run's"], [accountId, "invalid", "prepared", null],
  ]) assert.throws(() => reviewFirstReceiptCensusStatement(...args), /C2C_CENSUS_REFUSED/);
});

test("Linux unit guard binds exact invocation and rw scope, not an arbitrary service", () => {
  const unitName = "bluey-jobs-qa-fixture.service", invocationId = "a".repeat(32), unitCgroup = `/system.slice/${unitName}`;
  const launcherParent = "/tmp/bluey-test-runs-a1b2c3d4e5f6";
  const expected = { unitName, invocationId, unitCgroup, launcherParent, mainPid: 12345 };
  const row = { Id: unitName, InvocationID: invocationId, MainPID: "12345", ControlGroup: unitCgroup, PrivateNetwork: "yes",
    NoNewPrivileges: "yes", CapabilityBoundingSet: "", Restart: "no", KillMode: "control-group", ActiveState: "active",
    SubState: "running", Type: "exec", NetworkNamespacePath: "", JoinsNamespaceOf: "", PrivateTmp: "no",
    AmbientCapabilities: "", ProtectSystem: "strict", ProtectHome: "yes", ExitType: "cgroup", ReadWritePaths: launcherParent,
    RestrictAddressFamilies: "AF_UNIX AF_INET AF_INET6 AF_NETLINK" };
  const record = value => Object.entries(value).map(([key, v]) => `${key}=${v}\n`).join("");
  assert.equal(validateLocalQaLinuxC2cUnitShow(parseLocalQaLinuxC2cUnitShow(record(row)), expected), true);
  for (const change of [{ MainPID: "12346" }, { InvocationID: "b".repeat(32) }, { PrivateNetwork: "no" },
    { ReadWritePaths: `${launcherParent} /srv` }, { NetworkNamespacePath: "/run/netns/unknown" }, { AmbientCapabilities: "CAP_SYS_ADMIN" }]) {
    assert.throws(() => validateLocalQaLinuxC2cUnitShow(parseLocalQaLinuxC2cUnitShow(record({ ...row, ...change })), expected));
  }
  assert.throws(() => parseLocalQaLinuxC2cUnitShow(record(row) + "Id=another.service\n"));
});

test("refuses non-owned origin and non-fixture employer before the browser step", async () => {
  const { input, effects } = ownedFixture();
  await assert.rejects(observeLocalQaC2cHandoff({ ...input, origin: "http://localhost:45931" }), /loopback_origin/);
  await assert.rejects(observeLocalQaC2cHandoff({ ...input, employerUrl: "https://unexpected.invalid" }), /fixture_scope/);
  await assert.rejects(observeLocalQaC2cHandoff({ ...input, employerUrl: employerUrl.replace("a".repeat(32), "b".repeat(32)) }),
    /persisted_draft_binding/);
  assert.deepEqual(effects, []);
});

test("refuses click, traffic and census failures and closes the admitted browser context", async () => {
  for (const [override, error] of [
    [{ clickError: true }, /synthetic click refused/], [{ earlyDeparture: true }, /unexpected_request/],
    [{ matchStatus: 404 }, /current_match_status/], [{ matchUrl: "https://stale.invalid" }, /fresh_match_binding/],
    [{ freshEligibility: { hard_failures: [{ code: "mismatch" }] } }, /fresh_match_binding/],
    [{ freshEligibility: { passed_checks: eligibility.passed_checks.slice(1) } }, /fresh_match_binding/],
    [{ duplicateDetail: true }, /exact_match_read/], [{ write: true }, /mutation_attempted/],
    [{ foreign: true }, /unexpected_request/], [{ socket: true }, /unexpected_request/],
    [{ balanceDrift: true }, /census_changed/], [{ queueDrift: true }, /census_attempts/],
    [{ appDrift: true }, /application_changed/], [{ mail: true }, /mail_attempt/],
  ]) {
    const { input, effects } = ownedFixture(override);
    await assert.rejects(observeLocalQaC2cHandoff(input), error);
    assert.deepEqual(effects.at(-1), ["closed"]);
  }
});
