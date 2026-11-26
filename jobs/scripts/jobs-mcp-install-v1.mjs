// Offline compiler only. The dedicated operator, not this module, verifies
// admission, applies the SQL in a transaction, and validates installed state.
import { createHash } from "node:crypto";

const SHORTLIST_SHA256 = "da08fad72bf8c5eccf65f62e8877f013712a4fded25b8b92b787c34ce5317919";
const TEMPLATE_SHA256 = "a17589ab1af7ead89e7b2019b46d8a60a93740bc443ea6e690ec9bed86790a49";
const BODY_START = "WITH authorized AS (";
const SLOT = "-- @CLOSED_SHORTLIST_V1_QUERY@";
const DB_CLOCK = "(EXTRACT(EPOCH FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT";
const DIGEST_ARG =
  '(CASE WHEN $1 COLLATE pg_catalog."C" ~ \'^[0-9a-f]{64}$\' ' +
  "AND pg_catalog.octet_length($1) = 64 THEN $1 END)";
const RELATIONS = Object.freeze([
  "account_deletion_intents",
  "accounts",
  "jobs_global_candidate_materializations",
  "jobs_global_candidate_memberships",
  "jobs_global_candidates",
  "jobs_global_discovery_sources",
  "jobs_global_ingestion_runs",
  "jobs_mcp_read_grants",
  "jobs_postings",
  "jobs_preferences",
  "jobs_profiles",
  "jobs_public_beta_cohorts",
  "jobs_public_beta_enrollments",
  "jobs_public_beta_overrides",
  "jobs_track_policy_track_input_heads",
  "jobs_tracks",
  "jobs_workday_reconciliation_holds",
]);

function refused() {
  throw new Error("jobs_mcp_install_v1_input_refused");
}

function digest(value) {
  return createHash("sha256").update(value).digest("hex");
}

function checkedText(value, pin) {
  // Hash and decode the same held bytes. No path or SQL override is accepted.
  if (!Buffer.isBuffer(value) || value.length === 0 || value.length > 16_384) refused();
  const held = Buffer.from(value);
  if (digest(held) !== pin) refused();
  return new TextDecoder("utf-8", { fatal: true }).decode(held);
}

function replaceExact(source, before, after, count = 1) {
  if (source.split(before).length !== count + 1) refused();
  return source.split(before).join(after);
}

export function renderJobsMcpInstallV1(shortlistBytes, templateBytes) {
  const shortlist = checkedText(shortlistBytes, SHORTLIST_SHA256);
  const template = checkedText(templateBytes, TEMPLATE_SHA256);
  const start = shortlist.indexOf(BODY_START);
  if (start < 0 || shortlist.lastIndexOf(BODY_START) !== start) refused();
  let body = shortlist.slice(start).trimEnd();
  // Relation positions are closed; the two CTE reads deliberately stay local.
  const seen = [...body.matchAll(/\b(?:FROM|JOIN) ([a-z_]+) AS /g)].map((match) => match[1]);
  if (seen.length !== RELATIONS.length || seen.sort().some((name, i) => name !== RELATIONS[i])) {
    refused();
  }
  body = body.replace(/\b(FROM|JOIN) ([a-z_]+) AS /g, "$1 public.$2 AS ");
  body = replaceExact(body, 'COLLATE "C"', 'COLLATE pg_catalog."C"', 2);
  body = replaceExact(body, "statement_timestamp()", "pg_catalog.statement_timestamp()", 2);
  body = replaceExact(body, "$1", DIGEST_ARG);
  body = replaceExact(body, "$2", DB_CLOCK, 2);
  const sql = replaceExact(template, SLOT, body);
  return Object.freeze({
    version: 1,
    role: "jobs_mcp_shortlist_operator_transaction_fragment",
    transactionRequired: true,
    shortlistSha256: SHORTLIST_SHA256,
    templateSha256: TEMPLATE_SHA256,
    sqlSha256: digest(sql),
    sql,
  });
}
