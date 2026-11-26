// Private test driver. The caller owns the fresh authenticated browser context,
// pinned loopback gateway, OS network restriction and physical SQL observer.
import assert from "node:assert/strict";
import { hasNoQaDispatch, isQaC2cCandidateReceipt } from "./local-qa-durable-draft.mjs";

const ZERO_COUNTERS = Object.freeze(["submitted", "attempts", "auto_submit_authorizations", "balance_events",
  "browser_sessions", "commands", "communication_actions", "execution_leases", "interventions",
  "irreversible_receipts", "metering", "run_events", "workflow_executions"]);
const ONE_COUNTERS = Object.freeze(["accounts", "profiles", "applications", "prepared", "resumes", "factual_draft_links"]);
const CENSUS_KEYS = [...ZERO_COUNTERS, ...ONE_COUNTERS, "entitlement", "balance_cents"].sort();
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const SYNTHETIC_URL = /^https:\/\/boards\.greenhouse\.io\/blueynonprod\/jobs\/durable-c2c-qa-[0-9a-f]{32}$/;

function captureWorkspace(workspace, binding, employerUrl) {
  assert(hasNoQaDispatch(workspace) && workspace.matches_page?.next_cursor === null
    && Array.isArray(workspace.applications) && workspace.applications.length === 1
    && Array.isArray(workspace.matches) && workspace.matches.length === 1, "c2c_handoff_workspace_scope");
  const [app] = workspace.applications;
  const [match] = workspace.matches;
  assert(app.id === binding.applicationId && app.job_id === binding.jobId && app.resume_version_id === binding.resumeId
    && app.state === "awaiting_review" && app.submission_mode === "review_first"
    && app.run_id === null && app.submitted_at_ms === null && app.receipt?.confirmation === null
    && app.receipt.resume_version_id === binding.resumeId
    && app.receipt.job_snapshot_fingerprint === binding.jobSnapshotFingerprint
    && isQaC2cCandidateReceipt(app.receipt, binding.jobId)
    && app.receipt.job_snapshot.canonical_url === employerUrl
    && match.id === binding.jobId && match.employment_type === "contract c2c" && match.canonical_url === employerUrl,
  "c2c_handoff_persisted_draft_binding");
  return structuredClone(app);
}

function captureCensus(row) {
  assert(row && Object.getPrototypeOf(row) === Object.prototype, "c2c_handoff_census_record");
  assert.deepEqual(Object.keys(row).sort(), CENSUS_KEYS, "c2c_handoff_census_fields");
  for (const key of ZERO_COUNTERS) assert.equal(row[key], 0, `c2c_handoff_census_${key}`);
  for (const key of ONE_COUNTERS) assert.equal(row[key], 1, `c2c_handoff_census_${key}`);
  assert(Number.isSafeInteger(row.balance_cents), "c2c_handoff_census_balance");
  assert.deepEqual(row.entitlement, { plan: "free", local_browser: 0, cloud_browser: 0, used_packets: 0 },
    "c2c_handoff_census_entitlement");
  // The caller cannot mutate an earlier baseline during a later readback.
  return structuredClone(row);
}

function bindOrigin(value) {
  assert(typeof value === "string" && /^http:\/\/127\.0\.0\.1:[1-9][0-9]{3,4}$/.test(value),
    "c2c_handoff_loopback_origin");
  const url = new URL(value);
  assert(url.origin === value && Number(url.port) > 1024 && Number(url.port) !== 5187, "c2c_handoff_owned_port");
  return value;
}

/**
 * Consumes an already-signed-in, test-owned context. The page must have rendered
 * the persisted draft panel. Every external request is intercepted; the one
 * expected navigation is aborted, never fetched or fulfilled. This hook does
 * not install the pre-login OS fence or authorize starting a native runner.
 */
export async function observeLocalQaC2cHandoff({ context, page, origin, preparedDraft, employerUrl,
  assertOwned, readCensus, readWorkspace, readMailAttempts }) {
  const local = bindOrigin(origin);
  assert(preparedDraft && Object.getPrototypeOf(preparedDraft) === Object.prototype
    && Object.keys(preparedDraft).sort().join(",") === "applicationId,fixtureMode,jobId,jobSnapshotFingerprint,resumeId"
    && preparedDraft.fixtureMode === "c2c"
    && [preparedDraft.applicationId, preparedDraft.jobId, preparedDraft.resumeId].every(id => typeof id === "string" && UUID.test(id))
    && typeof preparedDraft.jobSnapshotFingerprint === "string" && /^[0-9a-f]{64}$/.test(preparedDraft.jobSnapshotFingerprint)
    && typeof employerUrl === "string" && SYNTHETIC_URL.test(employerUrl), "c2c_handoff_fixture_scope");
  const binding = Object.freeze({ ...preparedDraft });
  const { jobId } = binding;
  assert([assertOwned, readCensus, readWorkspace, readMailAttempts].every(fn => typeof fn === "function"), "c2c_handoff_observers");
  assert(context.pages().length === 1 && context.pages()[0] === page
    && page.url() === `${local}/jobs/applications`, "c2c_handoff_owned_page");
  const verify = async () => assert.equal(await assertOwned(), true, "c2c_handoff_owner_changed");
  const mail = async () => assert.equal(await readMailAttempts(), 0, "c2c_handoff_mail_attempt");
  await verify();
  const saved = captureWorkspace(await readWorkspace(), binding, employerUrl);
  const before = captureCensus(await readCensus());
  await mail();
  const detailUrl = `${local}/api/jobs/matches/${jobId}`;
  let matchReads = 0, navigationsAborted = 0, mutations = 0, unexpected = 0;
  let resolveDeparture, timer;
  const departure = new Promise(resolve => { resolveDeparture = resolve; });
  // Leave this handler installed until the context is closed, including on
  // failure. Fall back only to the outer caller's pre-existing loopback filter.
  try {
    await context.routeWebSocket("**/*", socket => {
      unexpected++;
      socket.close();
    });
    await context.route("**/*", async route => {
      const req = route.request();
      const target = new URL(req.url());
      if (req.method() !== "GET") {
        mutations++;
        return route.abort("blockedbyclient");
      }
      if (req.url() === employerUrl && req.isNavigationRequest() && req.frame() === page.mainFrame()) {
        if (matchReads !== 1 || navigationsAborted !== 0) unexpected++;
        await route.abort("blockedbyclient");
        navigationsAborted++;
        resolveDeparture();
        return;
      }
      if (target.origin !== local || target.username || target.password) {
        unexpected++;
        return route.abort("blockedbyclient");
      }
      if (req.url() === detailUrl) matchReads++;
      return route.fallback();
    });
    const dialog = page.getByRole("dialog");
    await dialog.getByText("You submit this application", { exact: true }).waitFor({ timeout: 8000 });
    assert.equal(matchReads, 0, "c2c_handoff_match_read_before_click");
    const deadline = new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error("c2c_handoff_no_cancelled_navigation")), 12000);
    });
    const currentMatch = page.waitForResponse(response => response.url() === detailUrl
      && response.request().method() === "GET", { timeout: 10000 }).then(async response => {
      assert.equal(response.status(), 200, "c2c_handoff_current_match_status");
      const posting = await response.json();
      assert(posting?.canonical_url === employerUrl
        && isQaC2cCandidateReceipt({ job_snapshot: posting, eligibility: posting.eligibility }, jobId),
      "c2c_handoff_fresh_match_binding");
    });
    await Promise.race([deadline, Promise.all([
      departure, currentMatch,
      dialog.getByRole("button", { name: "Open employer site to apply", exact: true }).click({ timeout: 8000 }),
    ])]);
    assert.equal(context.pages().length, 1, "c2c_handoff_unexpected_popup");
  } finally {
    clearTimeout(timer);
    await context.close();
  }
  await verify();
  assert.deepEqual(captureWorkspace(await readWorkspace(), binding, employerUrl), saved, "c2c_handoff_application_changed");
  assert.deepEqual(captureCensus(await readCensus()), before, "c2c_handoff_census_changed");
  await mail();
  await verify();
  assert.equal(matchReads, 1, "c2c_handoff_exact_match_read");
  assert.equal(navigationsAborted, 1, "c2c_handoff_exact_cancelled_navigation");
  assert.equal(mutations, 0, "c2c_handoff_mutation_attempted");
  assert.equal(unexpected, 0, "c2c_handoff_unexpected_request");
  return Object.freeze({ matchReads, navigationsAborted, mutations, unexpected, stableCensus: true });
}
