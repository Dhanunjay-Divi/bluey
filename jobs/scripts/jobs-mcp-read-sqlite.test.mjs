// Exercise the product SQLite migrations and detached grant-only query without
// opening a product DB. Intended destination: jobs/scripts/.
// Base SHA-256: 7720915c8d0fd0589d1fc4cffb5e85020eb79d44f8cec471c176d5c25cf0f870
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const migration = (name) => read(`../../infra/sqlite/server-runtime/${name}.sql`);
const query = read("../../server/src/db/jobs-mcp-read-sqlite.sql");
const runtime = read("../../server/src/db/mod.rs");
const trackDDL = migration("056_jobs_canonical_taxonomy_authority");
const now = 10_000;
const digest = (digit) => createHash("sha256").update(`bjm1_${digit.repeat(64)}`).digest("hex");
const owner = digest("a");
const other = digest("b");
const audience = "bluey-jobs-mcp-stdio-v1";
const scope = "jobs:matches:read";
const alphaTransition = "c".repeat(64);
const betaTransition = "d".repeat(64);
const resultColumns = [
  "account_id", "track_id", "input_generation", "input_transition_sha256",
  "issued_at_ms", "expires_at_ms", "inputs_updated_at_ms", "job_id", "title",
  "company", "location", "match_score", "candidate_updated_at_ms",
];

function table(sql, name) {
  const start = sql.indexOf(`CREATE TABLE IF NOT EXISTS ${name} (`);
  const end = sql.indexOf(";", start);
  assert.ok(start >= 0 && end > start, `missing ${name} fixture DDL`);
  return sql.slice(start, end + 1);
}

function insertMatch(db, account, id, score, updated, witnessed = updated) {
  db.prepare(`INSERT INTO jobs_postings
    (id,account_id,canonical_key,posting_json,source,company,title,match_score,created_at_ms,updated_at_ms)
    VALUES (?,?,?,'private fixture body','feed','Synthetic','Data Engineer',?,1,1)`)
    .run(id, account, id, score);
  db.prepare(`INSERT INTO jobs_global_candidates
    (id,canonical_key,candidate_json,company,title,canonical_url,availability_status,
     first_seen_at_ms,last_seen_at_ms,updated_at_ms) VALUES (?,?,'private candidate data',
     'Synthetic','Data Engineer','https://example.invalid/private','active',1,1,?)`)
    .run(id, id, updated);
  db.prepare(`INSERT INTO jobs_global_candidate_memberships
    (source_id,external_id,candidate_id,content_hash,first_seen_at_ms,last_seen_at_ms,last_seen_run_id)
    VALUES ('public-feed',?,?, 'opaque',1,1,'run')`).run(id, id);
  db.prepare("INSERT INTO jobs_global_candidate_materializations VALUES (?,?,?,?,?,1)")
    .run(account, id, id, `${account}-track`, witnessed);
}

function fixture(t) {
  const db = new DatabaseSync(":memory:");
  t.after(() => db.close());
  db.exec(`PRAGMA foreign_keys=ON;
    CREATE TABLE accounts(id TEXT PRIMARY KEY, email_verified_at INTEGER, is_temporary INTEGER NOT NULL);
    ${table(runtime, "jobs_tracks")}
    CREATE UNIQUE INDEX track_owner ON jobs_tracks(account_id,id);
    ${table(runtime, "jobs_postings")}
    ${table(runtime, "jobs_profiles")}
    ${table(runtime, "jobs_preferences")}
    ${table(trackDDL, "jobs_track_policy_track_input_transitions")}
    ${table(trackDDL, "jobs_track_policy_track_input_heads")}
    ${migration("035_jobs_global_candidate_index")}
    ${migration("043_account_deletion_intents")}
    ${migration("060_jobs_public_beta_access")}
    CREATE TABLE jobs_workday_reconciliation_holds(account_id TEXT NOT NULL, job_id TEXT NOT NULL);
    ${migration("072_jobs_mcp_read_grants")}
    INSERT INTO accounts VALUES ('alpha',1,0),('beta',1,0);
    UPDATE jobs_public_beta_cohorts SET state='open',hard_cap=2,assigned_count=2 WHERE id='public-v1';
    INSERT INTO jobs_public_beta_enrollments VALUES ('public-v1','alpha','admin',1),('public-v1','beta','admin',1);
    INSERT INTO jobs_global_discovery_sources (id,provider,source_key,source_json,next_run_at_ms,
      created_at_ms,updated_at_ms,health) VALUES ('public-feed','fixture','source','{}',1,1,1,'healthy');
    INSERT INTO jobs_global_ingestion_runs (id,source_id,replay_key,status,started_at_ms,completed_at_ms)
      VALUES ('run','public-feed','unique','completed',1,2);
  `);
  const physical = db.prepare("INSERT INTO jobs_tracks VALUES (?,?,'{}',1,1,1)");
  const transition = db.prepare("INSERT INTO jobs_track_policy_track_input_transitions VALUES (?,?,?,1,0,?,?,NULL,1)");
  const head = db.prepare("INSERT INTO jobs_track_policy_track_input_heads VALUES (?,?,1,0,?,?,?,NULL,1)");
  const grant = db.prepare("INSERT INTO jobs_mcp_read_grants VALUES (?,?,?,?,?,1,?,?,?,NULL)");
  for (const [account, track, token, hash] of [
    ["alpha", "alpha-track", owner, alphaTransition], ["beta", "beta-track", other, betaTransition],
  ]) {
    physical.run(track, account);
    transition.run(`${track}-1`, account, track, "e".repeat(64), hash);
    head.run(account, track, `${track}-1`, "e".repeat(64), hash);
    grant.run(token, account, track, audience, scope, hash, now - 100, now + 100);
  }
  for (const [account, id, score, updated, witnessed] of [
    ["alpha", "lower", 80, 24, 24], ["alpha", "latest", 90, 23, 23],
    ["alpha", "older", 90, 22, 22], ["alpha", "held", 99, 21, 21],
    ["alpha", "stale", 100, 21, 20], ["beta", "beta-job", 100, 21, 21],
  ]) insertMatch(db, account, id, score, updated, witnessed);
  db.exec("INSERT INTO jobs_workday_reconciliation_holds VALUES ('alpha','held')");
  return db;
}

function matches(db, grant = owner, at = now) {
  const changes = () => db.prepare("SELECT total_changes() AS n").get().n;
  const before = changes();
  const rows = db.prepare(query).all({ 1: grant, 2: at }).map((row) => ({ ...row }));
  assert.equal(changes(), before);
  return rows;
}

function emptyMarker(inputsUpdatedAtMs = 1) {
  return {
    account_id: "alpha", track_id: "alpha-track", input_generation: 1,
    input_transition_sha256: alphaTransition, issued_at_ms: now - 100,
    expires_at_ms: now + 100, inputs_updated_at_ms: inputsUpdatedAtMs,
    job_id: null, title: null, company: null, location: null, match_score: null,
    candidate_updated_at_ms: null,
  };
}

test("the grant selects one tenant and the projection returns only the closed 13 columns", (t) => {
  const db = fixture(t);
  insertMatch(db, "alpha", "a-tie", 95, 30);
  insertMatch(db, "alpha", "A-tie", 95, 30);
  const alpha = matches(db);
  assert.deepEqual(alpha.map((row) => row.job_id), ["A-tie", "a-tie", "latest", "older", "lower"]);
  for (const row of alpha) {
    assert.deepEqual(Object.keys(row), resultColumns);
    assert.equal(row.account_id, "alpha");
    assert.equal(row.track_id, "alpha-track");
    assert.equal(row.input_generation, 1);
    assert.equal(row.input_transition_sha256, alphaTransition);
  }
  assert(!JSON.stringify(alpha).includes("private fixture body"));
  assert(!JSON.stringify(alpha).includes("private candidate data"));
  assert.deepEqual(matches(db, other).map((row) => row.job_id), ["beta-job"]);
  assert.deepEqual(matches(db, digest("f")), []);
});

test("lease time, account verification, enrollment, deletion and Track revision deny every read", (t) => {
  const db = fixture(t);
  assert.deepEqual(matches(db, owner, now - 101), []);
  assert.deepEqual(matches(db, owner, now + 100), []);
  for (const update of [
    "UPDATE accounts SET email_verified_at=NULL WHERE id='alpha'",
    "UPDATE accounts SET is_temporary=1 WHERE id='alpha'",
    "UPDATE jobs_public_beta_cohorts SET state='suspended' WHERE id='public-v1'",
    "DELETE FROM jobs_public_beta_enrollments WHERE account_id='alpha'",
    "INSERT INTO jobs_public_beta_overrides VALUES ('public-v1','alpha',1,1,1,1)",
    "INSERT INTO account_deletion_intents VALUES ('alpha',1,1,1,0)",
    "UPDATE jobs_tracks SET active=0 WHERE id='alpha-track'",
    "DELETE FROM jobs_track_policy_track_input_heads WHERE account_id='alpha'",
  ]) {
    db.exec(`SAVEPOINT deny_fixture; ${update};`);
    assert.deepEqual(matches(db), [], update);
    db.exec("ROLLBACK TO deny_fixture; RELEASE deny_fixture;");
  }
  assert.equal(matches(db).length, 3);
});

test("revocation cannot be undone, rebound, or replaced by SQLite REPLACE", (t) => {
  const db = fixture(t);
  db.prepare("UPDATE jobs_mcp_read_grants SET revoked_at_ms=? WHERE token_sha256=?").run(now, owner);
  assert.deepEqual(matches(db), []);
  for (const update of ["revoked_at_ms=NULL", "expires_at_ms=10200", "scope='jobs:apply'"]) {
    assert.throws(() => db.prepare(`UPDATE jobs_mcp_read_grants SET ${update} WHERE token_sha256=?`).run(owner),
      /one-way revocation/);
  }
  assert.throws(() => db.prepare(`INSERT OR REPLACE INTO jobs_mcp_read_grants
    VALUES (?,'alpha','alpha-track',?,?,1,?,9900,10200,NULL)`).run(owner, audience, scope, alphaTransition),
  /tokens cannot be replaced/);
  db.exec(migration("072_jobs_mcp_read_grants"));
  assert.deepEqual(matches(db), []);
  assert.equal(matches(db, other).length, 1);
});

test("the stored grant refuses wrong audience, scope, owner, or noncanonical digest", (t) => {
  const db = fixture(t);
  const issue = db.prepare(`INSERT INTO jobs_mcp_read_grants
    VALUES (?,'alpha',?,?,?,1,?,9900,10200,NULL)`);
  const next = digest("f");
  assert.throws(() => issue.run(next, "alpha-track", "another-client", scope, alphaTransition), /CHECK/);
  assert.throws(() => issue.run(next, "alpha-track", audience, "jobs:apply", alphaTransition), /CHECK/);
  assert.throws(() => issue.run(next, "beta-track", audience, scope, alphaTransition), /FOREIGN KEY/);
  assert.throws(() => issue.run(`${next}\0extra`, "alpha-track", audience, scope, alphaTransition), /CHECK/);
  assert.deepEqual(matches(db, next), []);
});

test("unhealthy, incomplete, stale or held sources never manufacture a positive row", (t) => {
  const db = fixture(t);
  assert.deepEqual(matches(db).map((row) => row.job_id), ["latest", "older", "lower"]);
  for (const [update, inputs] of [
    ["UPDATE jobs_global_discovery_sources SET health='paused'", 1],
    ["UPDATE jobs_global_ingestion_runs SET status='running'", 1],
    ["UPDATE jobs_tracks SET updated_at_ms=2 WHERE id='alpha-track'", 2],
  ]) {
    db.exec(`SAVEPOINT empty_fixture; ${update};`);
    assert.deepEqual(matches(db), [emptyMarker(inputs)]);
    db.exec("ROLLBACK TO empty_fixture; RELEASE empty_fixture;");
  }
});

test("the fixed query exposes a deterministic 241st overflow sentinel without caller selectors", (t) => {
  const db = fixture(t);
  db.exec("DELETE FROM jobs_workday_reconciliation_holds; DELETE FROM jobs_global_candidate_materializations; DELETE FROM jobs_global_candidate_memberships; DELETE FROM jobs_global_candidates; DELETE FROM jobs_postings;");
  for (let index = 0; index < 260; index++) {
    insertMatch(db, "alpha", `job-${String(index).padStart(3, "0")}`, 90, 50);
  }
  const snapshot = matches(db);
  assert.equal(snapshot.length, 241);
  assert.equal(snapshot[0].job_id, "job-000");
  assert.equal(snapshot[240].job_id, "job-240");
  assert.deepEqual([...new Set(query.match(/\?[0-9]+/g))].sort(), ["?1", "?2"]);
  assert(!query.includes("requested_track_id"));
  assert(query.includes("posting.id COLLATE BINARY ASC"));
  assert(query.includes("LIMIT 241"));
});
