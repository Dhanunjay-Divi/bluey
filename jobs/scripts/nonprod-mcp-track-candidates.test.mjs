// Detached test of Track-scoped selection, before actual product authorization and eligibility.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";

const read = path => readFileSync(new URL(path, import.meta.url), "utf8");
const selector = read("./nonprod-mcp-track-candidates.select.sql");
const jobsDDL = read("../../server/src/db/mod.rs");
const ledgerDDL = read("../../infra/sqlite/server-runtime/056_jobs_canonical_taxonomy_authority.sql");
const betaDDL = read("../../infra/sqlite/server-runtime/060_jobs_public_beta_access.sql");
const intentDDL = read("../../infra/sqlite/server-runtime/043_account_deletion_intents.sql");
const transition = "d".repeat(64);
const now = 10_000;
const account = "owner", track = "target";

function table(source, name) {
  const start = source.indexOf(`CREATE TABLE IF NOT EXISTS ${name} (`);
  const tail = source.slice(start);
  const end = tail.indexOf(";");
  assert.ok(start >= 0 && end > 0);
  return tail.slice(0, end + 1);
}

function fixture() {
  const db = new DatabaseSync(":memory:");
  db.exec(`PRAGMA foreign_keys=ON;
    CREATE TABLE accounts(id TEXT PRIMARY KEY, email_verified_at INTEGER, is_temporary INTEGER NOT NULL);
    ${table(jobsDDL, "jobs_tracks")}
    CREATE UNIQUE INDEX fixture_jobs_tracks_unique ON jobs_tracks(account_id,id);
    ${table(jobsDDL, "jobs_postings")}
    CREATE UNIQUE INDEX fixture_jobs_postings_unique ON jobs_postings(account_id,id);
    ${table(ledgerDDL, "jobs_track_policy_track_input_transitions")}
    ${table(ledgerDDL, "jobs_track_policy_track_input_heads")}
    ${betaDDL}
    ${intentDDL}
    CREATE TABLE fixture_mcp_match_heads(
      account_id TEXT, track_id TEXT, track_generation INTEGER, track_transition_sha256 TEXT,
      membership_revision INTEGER, PRIMARY KEY(account_id,track_id)
    ) STRICT;
    CREATE TABLE fixture_mcp_match_membership(
      account_id TEXT, track_id TEXT, membership_revision INTEGER,
      posting_id TEXT COLLATE BINARY NOT NULL
        CHECK(instr(posting_id,char(0)) = 0 AND length(posting_id) BETWEEN 1 AND 256
          AND posting_id NOT GLOB '*[^A-Za-z0-9_.:-]*'),
      match_score INTEGER NOT NULL CHECK(match_score BETWEEN -9007199254740991 AND 9007199254740991),
      updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms BETWEEN 0 AND 9007199254740991),
      score_key INTEGER GENERATED ALWAYS AS (-match_score) STORED,
      time_key INTEGER GENERATED ALWAYS AS (-updated_at_ms) STORED,
      PRIMARY KEY(account_id,track_id,membership_revision,posting_id),
      FOREIGN KEY(account_id,posting_id) REFERENCES jobs_postings(account_id,id) ON DELETE CASCADE
    ) STRICT;
    CREATE INDEX fixture_mcp_match_order ON fixture_mcp_match_membership
      (account_id,track_id,membership_revision,score_key,time_key,posting_id COLLATE BINARY);
    INSERT INTO accounts VALUES ('owner',1,0),('foreign',1,0);
    UPDATE jobs_public_beta_cohorts SET state='open',hard_cap=2,assigned_count=2 WHERE id='public-v1';
    INSERT INTO jobs_public_beta_enrollments VALUES
      ('public-v1','owner','admin',1),('public-v1','foreign','admin',1);`);
  const physical = db.prepare("INSERT INTO jobs_tracks(id,account_id,track_json,active,created_at_ms,updated_at_ms) VALUES (?,?,'{}',1,1,1)");
  const append = db.prepare(`INSERT INTO jobs_track_policy_track_input_transitions VALUES (?,?,?,1,0,?,?,NULL,1)`);
  const head = db.prepare(`INSERT INTO jobs_track_policy_track_input_heads VALUES (?,?,1,0,?,?,?,NULL,1)`);
  const projection = db.prepare("INSERT INTO fixture_mcp_match_heads VALUES (?,?,1,?,7)");
  for (const [owner, id, hash] of [[account, track, transition], [account, "other-track", "e".repeat(64)],
    ["foreign", "foreign-track", "f".repeat(64)]]) {
    physical.run(id, owner);
    append.run(`${id}-transition`, owner, id, "a".repeat(64), hash);
    head.run(owner, id, `${id}-transition`, "a".repeat(64), hash);
    projection.run(owner, id, hash);
  }
  const posting = db.prepare(`INSERT INTO jobs_postings
    (id,account_id,canonical_key,posting_json,source,company,title,match_score,created_at_ms,updated_at_ms)
    VALUES (?,?,?,'synthetic-opaque-body','fixture','Synthetic','Synthetic',?,1,?)`);
  const membership = db.prepare("INSERT INTO fixture_mcp_match_membership VALUES (?,?,7,?,?,?)");
  for (const [owner, id, jobId, score, updated] of [
    [account, track, "m-1", 10, now], [account, track, "m-2", 10, now], [account, track, "m-3", 9, now - 1],
    [account, "other-track", "wrong-track", 1000, now], ["foreign", "foreign-track", "wrong-account", 1001, now],
  ]) {
    posting.run(jobId, owner, jobId, score, updated);
    membership.run(owner, id, jobId, score, updated);
  }
  return db;
}

function candidates(db, overrides = {}) {
  return db.prepare(selector).all({ $1: account, $2: track, $3: 1, $4: transition, $5: 7,
    $6: 1, $7: null, $8: null, $9: null, ...overrides }).map(row => ({ ...row }));
}

test("the exact Track prefix is applied before ordering, keyset and lookahead limit", () => {
  const db = fixture();
  try {
    const first = candidates(db);
    assert.deepEqual(first, [
      { posting_id: "m-1", match_score: 10, updated_at_ms: now },
      { posting_id: "m-2", match_score: 10, updated_at_ms: now },
    ]);
    assert.deepEqual(candidates(db, { $7: 10, $8: now, $9: "m-1" }).map(row => row.posting_id), ["m-2", "m-3"]);
    assert.deepEqual(candidates(db, { $7: 9, $8: now - 1, $9: "m-3" }), []);
    const naive = db.prepare(`SELECT id FROM jobs_postings WHERE account_id=?
      ORDER BY match_score DESC,updated_at_ms DESC,id LIMIT 1`).get(account);
    assert.equal(naive.id, "wrong-track", "an account-first page would lose the eligible scoped row");
    assert.throws(() => db.exec("INSERT INTO fixture_mcp_match_membership VALUES ('owner','target',7,'wrong-account',99,1)"),
      /FOREIGN KEY/);
    const plan = db.prepare(`EXPLAIN QUERY PLAN ${selector}`).all({ $1: account, $2: track, $3: 1,
      $4: transition, $5: 7, $6: 1, $7: null, $8: null, $9: null });
    assert.ok(plan.some(row => /fixture_mcp_match_order.*account_id=\? AND track_id=\? AND membership_revision=\?/.test(row.detail)));
  } finally { db.close(); }
});

test("unavailable or changed authority never falls back to account-wide candidates", () => {
  const db = fixture();
  try {
    for (const override of [{ $1: "foreign" }, { $2: "other-track" }, { $3: 2 }, { $4: "0".repeat(64) },
      { $5: 6 }, { $6: 0 }, { $6: 11 }, { $6: 1.5 }, { $7: 10 }, { $9: "m-1" },
      { $7: 9007199254740992, $8: now, $9: "m-1" }, { $7: 10, $8: -1, $9: "m-1" },
      { $7: 10, $8: now, $9: "m/1" }, { $7: 10, $8: now, $9: 1 }, { $7: 10, $8: now, $9: "m-1\0tail" }]) {
      assert.deepEqual(candidates(db, override), []);
    }
    for (const mutation of [
      "UPDATE jobs_tracks SET active=2 WHERE id='target'",
      "UPDATE accounts SET email_verified_at=NULL WHERE id='owner'",
      "UPDATE accounts SET is_temporary=1 WHERE id='owner'",
      "UPDATE jobs_public_beta_cohorts SET state='suspended'",
      "INSERT INTO jobs_public_beta_overrides VALUES('public-v1','owner',1,1,1,1)",
      "INSERT INTO account_deletion_intents(account_id,requested_at_ms,last_checked_at_ms,fresh_upload_cutoff_ms) VALUES('owner',1,1,1)",
      "UPDATE fixture_mcp_match_heads SET membership_revision=8 WHERE account_id='owner' AND track_id='target'",
    ]) {
      db.exec(`SAVEPOINT authority; ${mutation};`);
      const before = db.prepare("SELECT total_changes() AS n").get().n;
      assert.deepEqual(candidates(db), [], mutation);
      assert.equal(db.prepare("SELECT total_changes() AS n").get().n, before);
      db.exec("ROLLBACK TO authority; RELEASE authority;");
      assert.equal(candidates(db).length, 2);
    }
  } finally { db.close(); }
});

test("the first-page sentinel includes the largest allowed key without overflow", () => {
  const db = fixture();
  const edge = Number.MAX_SAFE_INTEGER;
  try {
    const update = db.prepare(`UPDATE fixture_mcp_match_membership SET match_score=?,updated_at_ms=? WHERE posting_id=?`);
    update.run(edge, edge, "m-1");
    update.run(-edge, 0, "m-2");
    assert.deepEqual(candidates(db).map(row => row.posting_id), ["m-1", "m-3"]);
    assert.deepEqual(candidates(db, { $7: edge, $8: edge, $9: "m-1" }).map(row => row.posting_id), ["m-3", "m-2"]);
    assert.deepEqual(candidates(db, { $7: -edge, $8: 0, $9: "m-2" }), []);
    assert.throws(() => update.run(edge + 1, edge, "m-3"), /CHECK constraint/);
    assert.throws(() => update.run(-edge - 1, edge, "m-3"), /CHECK constraint/);
  } finally { db.close(); }
});

test("deep continuation seeks by the full generated-key tuple without sorting the Track", () => {
  const db = fixture();
  try {
    db.exec(`WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<12000)
      INSERT INTO jobs_postings(id,account_id,canonical_key,posting_json,source,company,title,match_score,created_at_ms,updated_at_ms)
      SELECT printf('d%05d',i),'owner',printf('d%05d',i),'sealed','fixture','Synthetic','Synthetic',20,1,${now} FROM n;
      INSERT INTO fixture_mcp_match_membership
        SELECT 'owner','target',7,id,match_score,updated_at_ms FROM jobs_postings WHERE id LIKE 'd%';
      ANALYZE;`);
    const bind = { $1: account, $2: track, $3: 1, $4: transition, $5: 7, $6: 1,
      $7: 20, $8: now, $9: "d11999" };
    assert.deepEqual(db.prepare(selector).all(bind).map(r => r.posting_id), ["d12000", "m-1"]);
    const plan = db.prepare(`EXPLAIN QUERY PLAN ${selector}`).all(bind).map(row => row.detail).join("\n");
    assert.match(plan, /fixture_mcp_match_order[^\n]*\(score_key,time_key,posting_id\)>\(\?,\?,\?\)/);
    assert.doesNotMatch(plan, /TEMP B-TREE/);
  } finally { db.close(); }
});

test("a committed Track replacement invalidates old pages until a new scoped projection is published", () => {
  const db = fixture();
  try {
    const next = "1".repeat(64);
    db.exec("BEGIN IMMEDIATE");
    db.prepare(`INSERT INTO jobs_track_policy_track_input_transitions VALUES
      ('target-transition-2','owner','target',2,1,?,?,?,2)`).run("b".repeat(64), next, transition);
    db.prepare(`UPDATE jobs_track_policy_track_input_heads
      SET input_generation=2,previous_input_generation=1,input_transition_id='target-transition-2',
          track_semantic_sha256=?,input_transition_sha256=?,predecessor_input_transition_sha256=?,updated_at_ms=2
      WHERE account_id='owner' AND career_track_id='target'`).run("b".repeat(64), next, transition);
    db.exec("COMMIT");
    assert.deepEqual(candidates(db, { $7: 10, $8: now, $9: "m-1" }), []);
    assert.deepEqual(candidates(db, { $3: 2, $4: next }), [], "the old projection does not follow the new Track head");
    db.exec("BEGIN IMMEDIATE");
    db.prepare(`UPDATE fixture_mcp_match_heads SET track_generation=2,track_transition_sha256=?,membership_revision=8
      WHERE account_id='owner' AND track_id='target'`).run(next);
    db.exec(`INSERT INTO fixture_mcp_match_membership SELECT account_id,track_id,8,posting_id,match_score,updated_at_ms
      FROM fixture_mcp_match_membership WHERE account_id='owner' AND track_id='target' AND membership_revision=7;
      COMMIT;`);
    assert.deepEqual(candidates(db, { $3: 2, $4: next }), []);
    assert.deepEqual(candidates(db, { $3: 2, $4: next, $5: 8 }).map(row => row.posting_id), ["m-1", "m-2"]);
    assert.deepEqual(candidates(db), []);
  } finally { db.close(); }
});
