// Fixed, account-scoped PostgreSQL readback for the non-production C2C handoff.
// This module deliberately exposes neither a general SQL interface nor a DSN.

const REFUSAL = "BLUEY_LOCAL_QA_LINUX_C2C_CENSUS_REFUSED";
const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const MAX_OUTPUT = 4096;

const ZERO_COUNTERS = Object.freeze([
  "submitted",
  "attempts",
  "auto_submit_authorizations",
  "balance_events",
  "browser_sessions",
  "commands",
  "communication_actions",
  "execution_leases",
  "interventions",
  "irreversible_receipts",
  "metering",
  "run_events",
  "workflow_executions",
]);

const ONE_COUNTERS = Object.freeze([
  "accounts",
  "profiles",
  "applications",
  "prepared",
  "resumes",
  "factual_draft_links",
]);

const CENSUS_KEYS = Object.freeze([
  ...ZERO_COUNTERS,
  ...ONE_COUNTERS,
  "balance_cents",
  "entitlement",
].sort());
const ENTITLEMENT_KEYS = ["cloud_browser", "local_browser", "plan", "used_packets"];
const RECEIPT_COUNTS = Object.freeze({
  application_evidence: 3,
  evidence_total: 3,
  receipt_uploads_total: 3,
  ready_receipt_uploads: 3,
  capacity_total: 1,
  committed_capacity: 1,
});
const RECEIPT_KEYS = Object.freeze([...CENSUS_KEYS, ...Object.keys(RECEIPT_COUNTS)].sort());
const RECEIPT_ONE_COUNTERS = new Set([
  "accounts", "profiles", "applications", "resumes", "submitted", "attempts",
  "browser_sessions", "execution_leases", "metering",
]);

function refuse() {
  throw new Error(REFUSAL);
}

function need(value) {
  if (!value) refuse();
}

function plain(value) {
  return value !== null && typeof value === "object"
    && Object.getPrototypeOf(value) === Object.prototype;
}

function exactKeys(value, names) {
  return plain(value) && Object.keys(value).sort().join("|") === names.join("|");
}

function count(value) {
  need(Number.isSafeInteger(value) && value >= 0);
  return value;
}

/** The exact canonical C2C fixture account identity accepted by the census. */
export function requireC2cFixtureAccountId(accountId) {
  need(typeof accountId === "string" && UUID_V4.exec(accountId)?.[0] === accountId);
  return accountId;
}

// All account-specific subqueries execute in one PostgreSQL statement/snapshot.
// `accountId` is validated as a canonical UUID before it is embedded as a SQL
// literal, so this is not a caller-supplied SQL channel.
function censusValueStatement(accountId) {
  const account = requireC2cFixtureAccountId(accountId);
  const literal = `'${account}'`;
  return `SELECT json_build_object(
  'accounts',(SELECT count(*) FROM public.accounts WHERE id=${literal}),
  'profiles',(SELECT count(*) FROM public.jobs_profiles WHERE account_id=${literal}),
  'applications',(SELECT count(*) FROM public.jobs_applications WHERE account_id=${literal}),
  'prepared',(SELECT count(*) FROM public.jobs_applications
    WHERE account_id=${literal} AND state='awaiting_review' AND submitted_at_ms IS NULL),
  'resumes',(SELECT count(*) FROM public.jobs_resume_versions WHERE account_id=${literal}),
  'factual_draft_links',(SELECT count(*) FROM public.jobs_applications application
    JOIN public.jobs_resume_versions resume ON resume.account_id=application.account_id
      AND resume.id=application.resume_version_id AND resume.job_id=application.job_id
    WHERE application.account_id=${literal} AND application.state='awaiting_review'
      AND application.submitted_at_ms IS NULL AND resume.mode='factual'),
  'submitted',(SELECT count(*) FROM public.jobs_applications
    WHERE account_id=${literal} AND submitted_at_ms IS NOT NULL),
  'attempts',(SELECT count(*) FROM public.jobs_attempt_reservations WHERE account_id=${literal}),
  'auto_submit_authorizations',(SELECT count(*) FROM public.jobs_auto_submit_authorizations WHERE account_id=${literal}),
  'balance_events',(SELECT count(*) FROM public.balance_ledger_entries WHERE account_id=${literal}),
  'browser_sessions',(SELECT count(*) FROM public.jobs_browser_sessions WHERE account_id=${literal}),
  'commands',(SELECT count(*) FROM public.jobs_workflow_commands WHERE account_id=${literal}),
  'communication_actions',(SELECT count(*) FROM public.jobs_communication_actions WHERE account_id=${literal}),
  'execution_leases',(SELECT count(*) FROM public.jobs_execution_leases WHERE account_id=${literal}),
  'interventions',(SELECT count(*) FROM public.jobs_interventions WHERE account_id=${literal}),
  'irreversible_receipts',(SELECT count(*)
    FROM public.jobs_managed_cloud_irreversible_effect_receipts WHERE account_id=${literal}),
  'metering',(SELECT count(*) FROM public.jobs_packet_metering WHERE account_id=${literal}),
  'run_events',(SELECT count(*) FROM public.jobs_run_events WHERE account_id=${literal}),
  'workflow_executions',(SELECT count(*) FROM public.jobs_workflow_executions WHERE account_id=${literal}),
  'balance_cents',(SELECT balance_cents FROM public.accounts WHERE id=${literal}),
  'entitlement',(SELECT json_build_object('plan',plan,'local_browser',local_browser,
    'cloud_browser',cloud_browser,'used_packets',used_packets)
    FROM public.jobs_entitlements WHERE account_id=${literal})
)::text`;
}

export function c2cCensusStatement(accountId) {
  return `${censusValueStatement(accountId)};`;
}

/** Count only the bound receipt run, while rejecting excess candidate artifacts. */
export function reviewFirstReceiptCensusStatement(accountId, applicationId, phase, runId) {
  const base = censusValueStatement(accountId);
  need(typeof applicationId === "string" && UUID_V4.exec(applicationId)?.[0] === applicationId
    && ((phase === "prepared" && runId === null)
      || (phase === "submitted" && typeof runId === "string" && /^[A-Za-z0-9_-]{1,160}$/.exec(runId)?.[0] === runId)));
  const account = `'${accountId}'`;
  const application = `'${applicationId}'`;
  const run = runId === null ? "NULL" : `'${runId}'`;
  return `SELECT (census.value::jsonb || jsonb_build_object(
  'evidence_total',(SELECT count(*) FROM public.jobs_application_evidence WHERE account_id=${account}),
  'application_evidence',(SELECT count(*) FROM public.jobs_application_evidence
    WHERE account_id=${account} AND application_id=${application}),
  'receipt_uploads_total',(SELECT count(*) FROM public.object_uploads
    WHERE account_id=${account} AND metadata_json::jsonb->>'artifact_class'='jobs_submission_evidence'),
  'ready_receipt_uploads',(SELECT count(*) FROM public.object_uploads
    WHERE account_id=${account} AND metadata_json::jsonb->>'artifact_class'='jobs_submission_evidence'
      AND metadata_json::jsonb->>'jobs_application_id'=${application} AND metadata_json::jsonb->>'jobs_run_id'=${run}
      AND metadata_json::jsonb->>'jobs_runner'='cloud' AND state='ready' AND uploaded_at_ms > 0),
  'capacity_total',(SELECT count(*) FROM public.jobs_submission_evidence_capacity WHERE account_id=${account}),
  'committed_capacity',(SELECT count(*) FROM public.jobs_submission_evidence_capacity
    WHERE account_id=${account} AND application_id=${application} AND run_id=${run}
      AND runner='cloud' AND state='committed' AND consumed_bytes > 0 AND consumed_objects=3 AND completed_at_ms > 0)
))::text FROM (${base}) AS census(value);`;
}

function censusRow(output, keys = CENSUS_KEYS) {
  need(typeof output === "string" && output.length > 1 && output.length <= MAX_OUTPUT
    && /^\{[^\r\n]*\}\n?$/.test(output));
  const row = JSON.parse(output.trim());
  need(exactKeys(row, keys) && exactKeys(row.entitlement, ENTITLEMENT_KEYS));
  return Object.freeze({
    ...Object.fromEntries(keys.filter(key => key !== "entitlement")
      .map(key => [key, count(row[key])])),
    entitlement: Object.freeze({ ...row.entitlement }),
  });
}

/** Parse the sole bounded psql JSON row into the saved-account handoff shape. */
export function parseC2cCensusOutput(output) {
  try {
    const row = censusRow(output);
    need(row.entitlement.plan === "free"
      && row.entitlement.local_browser === 0
      && row.entitlement.cloud_browser === 0
      && row.entitlement.used_packets === 0);
    return row;
  } catch {
    refuse();
  }
}

/** Separate signed-posting QA scope: included packet, no debit or managed run. */
export function parseReviewFirstReceiptCensusOutput(output, phase) {
  try {
    need(phase === "prepared" || phase === "submitted");
    const row = censusRow(output, RECEIPT_KEYS);
    const ones = phase === "prepared" ? new Set(ONE_COUNTERS) : RECEIPT_ONE_COUNTERS;
    need(row.balance_cents === 0 && row.entitlement.plan === "cloud"
      && row.entitlement.local_browser === 1 && row.entitlement.cloud_browser === 1
      && row.entitlement.used_packets === (phase === "prepared" ? 0 : 1)
      && [...ZERO_COUNTERS, ...ONE_COUNTERS].every(key => row[key] === Number(ones.has(key)))
      && Object.entries(RECEIPT_COUNTS).every(([key, expected]) => row[key] === (phase === "submitted" ? expected : 0)));
    return row;
  } catch {
    throw new Error("BLUEY_LOCAL_QA_LINUX_REVIEW_FIRST_RECEIPT_CENSUS_REFUSED");
  }
}
