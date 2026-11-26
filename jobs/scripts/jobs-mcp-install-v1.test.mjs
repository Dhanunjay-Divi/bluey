import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { renderJobsMcpInstallV1 } from "./jobs-mcp-install-v1.mjs";

const shortlist = readFileSync(new URL("../../server/src/db/jobs-mcp-read-postgres.sql", import.meta.url));
const template = readFileSync(new URL("../../infra/postgres/mcp-operator/read-shortlist-v1.sql.in", import.meta.url));
const embedded = readFileSync(new URL("../../infra/postgres/mcp-operator/read-shortlist-v1.sql", import.meta.url));
const REFUSAL = { message: "jobs_mcp_install_v1_input_refused" };

test("renders one pinned, deterministic, transaction-required artifact", () => {
  const artifact = renderJobsMcpInstallV1(shortlist, template);
  assert.deepEqual(renderJobsMcpInstallV1(Buffer.from(shortlist), Buffer.from(template)), artifact);
  assert.equal(Object.isFrozen(artifact), true);
  assert.equal(artifact.role, "jobs_mcp_shortlist_operator_transaction_fragment");
  assert.equal(artifact.transactionRequired, true);
  assert.equal(artifact.version, 1);
  assert.equal(artifact.sqlSha256, "88c0885e3403ea84d7fe1f200330c02b1589856819b2f1cb66c5744a50c4901e");
  assert.equal(Buffer.byteLength(artifact.sql), 6917);
  assert.deepEqual(Buffer.from(artifact.sql), embedded);
  assert.doesNotMatch(artifact.sql, /@CLOSED_SHORTLIST_V1_QUERY@/);
});

test("binds the projected authority predicate and uses only database time", () => {
  const { sql } = renderJobsMcpInstallV1(shortlist, template);
  assert.match(sql, /CREATE FUNCTION jobs_mcp\.read_shortlist\(text\)/);
  assert.equal([...sql.matchAll(/pg_catalog\.statement_timestamp\(\)/g)].length, 4);
  assert.doesNotMatch(sql, /\$[2-9]/);
  assert.match(sql, /'\^\[0-9a-f\]\{64\}\$' AND pg_catalog\.octet_length\(\$1\) = 64/);
  assert.equal(new Set([...sql.matchAll(/\b(?:FROM|JOIN) public\.([a-z_]+) AS /g)].map((m) => m[1])).size, 17);
  assert.match(sql, /capability\.revoked_at_ms IS NULL/);
  assert.match(sql, /FROM public\.account_deletion_intents AS deletion/);
  assert.match(sql, /FROM public\.jobs_workday_reconciliation_holds AS hold/);
  assert.match(sql, /LIMIT 241\n\)/);
  assert.match(sql, /LEFT JOIN current_matches ON TRUE/);
  assert.equal([...sql.matchAll(/COLLATE pg_catalog\."C" ASC/g)].length, 2);
});

test("has fixed schema and function ACLs without cluster or shared-database changes", () => {
  const { sql } = renderJobsMcpInstallV1(shortlist, template);
  assert.match(sql, /SECURITY DEFINER\nSET search_path = pg_catalog, pg_temp\nSET row_security = on/);
  assert.match(sql, /REVOKE ALL ON SCHEMA jobs_mcp FROM PUBLIC;/);
  assert.match(sql, /REVOKE ALL ON FUNCTION jobs_mcp\.read_shortlist\(text\) FROM PUBLIC;/);
  assert.match(sql, /ALTER FUNCTION jobs_mcp\.read_shortlist\(text\) OWNER TO bluey_jobs_mcp_owner;/);
  assert.match(sql, /GRANT EXECUTE ON FUNCTION jobs_mcp\.read_shortlist\(text\) TO bluey_jobs_mcp_reader;/);
  assert.ok(
    sql.indexOf("GRANT CREATE ON SCHEMA jobs_mcp TO bluey_jobs_mcp_owner;") <
      sql.indexOf("ALTER FUNCTION jobs_mcp.read_shortlist(text) OWNER TO bluey_jobs_mcp_owner;"),
  );
  assert.match(sql, /REVOKE CREATE ON SCHEMA jobs_mcp FROM bluey_jobs_mcp_owner;\n$/);
  assert.doesNotMatch(sql, /(?:CREATE|ALTER|DROP) ROLE|ON DATABASE|ALTER DEFAULT PRIVILEGES|CREATE OR REPLACE/);
  assert.doesNotMatch(sql, /^(?:BEGIN;|COMMIT;|ROLLBACK;|DROP )/m);
});

test("refuses changes to either input instead of rendering a different operator version", () => {
  const changedQuery = Buffer.from(shortlist);
  changedQuery[0] ^= 1;
  assert.throws(() => renderJobsMcpInstallV1(changedQuery, template), REFUSAL);
  const changedTemplate = Buffer.from(template);
  changedTemplate[0] ^= 1;
  assert.throws(() => renderJobsMcpInstallV1(shortlist, changedTemplate), REFUSAL);
  assert.throws(() => renderJobsMcpInstallV1(shortlist, Buffer.concat([template, Buffer.from("\n")])), REFUSAL);
});

test("refuses strings, empty or oversized buffers, and missing inputs", () => {
  for (const invalid of ["", shortlist.toString(), Buffer.alloc(0), Buffer.alloc(16_385), undefined]) {
    assert.throws(() => renderJobsMcpInstallV1(invalid, template), REFUSAL);
    assert.throws(() => renderJobsMcpInstallV1(shortlist, invalid), REFUSAL);
  }
});
