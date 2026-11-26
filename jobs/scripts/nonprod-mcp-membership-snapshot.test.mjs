// Detached synthetic proof. It is not a migration, route, grant issuer, or MCP tool.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { lstatSync, mkdtempSync, readFileSync, realpathSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";

const root = process.env.BLUEY_TEST_RUN_ROOT;
const runRoot = realpathSync(root);
assert.match(runRoot, /^\/(?:private\/)?tmp\/bluey-test-runs-[A-Za-z0-9_-]+\/run\.[0-9]+\.[0-9]+\.[A-Za-z0-9]+$/);
const rootInfo = lstatSync(runRoot);
assert.ok(rootInfo.isDirectory() && rootInfo.uid === process.getuid() && (rootInfo.mode & 0o777) === 0o700);
const read = path => readFileSync(new URL(path, import.meta.url), "utf8");
const jobsDDL = read("../../server/src/db/mod.rs");
const ledgerDDL = read("../../infra/sqlite/server-runtime/056_jobs_canonical_taxonomy_authority.sql");
const betaDDL = read("../../infra/sqlite/server-runtime/060_jobs_public_beta_access.sql");
const deletionDDL = read("../../infra/sqlite/server-runtime/043_account_deletion_intents.sql");
const transition = "a".repeat(64);
const foreignTransition = "c".repeat(64);
const changedTransition = "b".repeat(64);

function table(source, name) {
  const start = source.indexOf(`CREATE TABLE IF NOT EXISTS ${name} (`);
  const tail = source.slice(start);
  const end = tail.indexOf(";");
  assert.ok(start >= 0 && end > 0, `registered ${name} DDL`);
  return tail.slice(0, end + 1);
}

function digest(rows) {
  assert.ok(Array.isArray(rows) && rows.length > 0);
  const seen = new Set();
  for (const row of rows) {
    assert.ok(typeof row.postingId === "string" && row.postingId.length >= 1 && row.postingId.length <= 256);
    assert.doesNotMatch(row.postingId, /[^A-Za-z0-9_.:-]/);
    assert.ok(Number.isSafeInteger(row.score) && row.score >= -9007199254740991 && row.score <= 9007199254740991);
    assert.ok(Number.isSafeInteger(row.updatedAtMs) && row.updatedAtMs >= 0 && row.updatedAtMs <= 9007199254740991);
    assert.ok(!seen.has(row.postingId)); seen.add(row.postingId);
  }
  const canonical = [...rows].sort((a, b) => Buffer.compare(Buffer.from(a.postingId), Buffer.from(b.postingId)))
    .map(({ postingId, score, updatedAtMs }) => [postingId, score, updatedAtMs]);
  return createHash("sha256").update("bluey-jobs-fixture-membership-v1\0")
    .update(JSON.stringify(canonical)).digest("hex");
}

const oldRows = [
  { postingId: "m-1", score: 10, updatedAtMs: 100 },
  { postingId: "m-2", score: 9, updatedAtMs: 90 },
];
const newRows = [
  { postingId: "m-3", score: 11, updatedAtMs: 110 },
  { postingId: "m-4", score: 8, updatedAtMs: 80 },
];

function createDb(path = ":memory:") {
  const db = new DatabaseSync(path);
  db.exec(`PRAGMA foreign_keys=ON;
    CREATE TABLE accounts(id TEXT PRIMARY KEY, email_verified_at INTEGER, is_temporary INTEGER NOT NULL);
    ${table(jobsDDL, "jobs_tracks")}
    CREATE UNIQUE INDEX fixture_tracks_account_id ON jobs_tracks(account_id,id);
    ${table(ledgerDDL, "jobs_track_policy_track_input_transitions")}
    ${table(ledgerDDL, "jobs_track_policy_track_input_heads")}
    ${betaDDL}
    ${deletionDDL}
    CREATE TABLE fixture_mcp_principals(id TEXT PRIMARY KEY, account_id TEXT NOT NULL REFERENCES accounts(id));
    CREATE TABLE fixture_mcp_grants(
      id TEXT PRIMARY KEY, account_id TEXT NOT NULL, track_id TEXT NOT NULL, track_generation INTEGER NOT NULL,
      track_transition TEXT NOT NULL, revoked INTEGER NOT NULL DEFAULT 0
    ) STRICT;
    CREATE TABLE fixture_mcp_membership_snapshots(
      account_id TEXT NOT NULL, track_id TEXT NOT NULL, revision INTEGER NOT NULL,
      track_generation INTEGER NOT NULL, track_transition TEXT NOT NULL, digest TEXT NOT NULL,
      item_count INTEGER NOT NULL CHECK(item_count > 0),
      PRIMARY KEY(account_id,track_id,revision)
    ) STRICT;
    CREATE TABLE fixture_mcp_membership_items(
      account_id TEXT NOT NULL, track_id TEXT NOT NULL, revision INTEGER NOT NULL,
      posting_id TEXT NOT NULL, score INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL,
      PRIMARY KEY(account_id,track_id,revision,posting_id),
      FOREIGN KEY(account_id,track_id,revision) REFERENCES fixture_mcp_membership_snapshots(account_id,track_id,revision)
    ) STRICT;
    CREATE TABLE fixture_mcp_membership_heads(
      account_id TEXT NOT NULL, track_id TEXT NOT NULL, revision INTEGER NOT NULL,
      digest TEXT NOT NULL, item_count INTEGER NOT NULL, track_generation INTEGER NOT NULL, track_transition TEXT NOT NULL,
      PRIMARY KEY(account_id,track_id),
      FOREIGN KEY(account_id,track_id,revision) REFERENCES fixture_mcp_membership_snapshots(account_id,track_id,revision)
    ) STRICT;
    CREATE TRIGGER fixture_snapshot_no_update BEFORE UPDATE ON fixture_mcp_membership_snapshots BEGIN SELECT RAISE(ABORT,'immutable'); END;
    CREATE TRIGGER fixture_snapshot_no_delete BEFORE DELETE ON fixture_mcp_membership_snapshots BEGIN SELECT RAISE(ABORT,'immutable'); END;
    CREATE TRIGGER fixture_item_no_update BEFORE UPDATE ON fixture_mcp_membership_items BEGIN SELECT RAISE(ABORT,'immutable'); END;
    CREATE TRIGGER fixture_item_no_delete BEFORE DELETE ON fixture_mcp_membership_items BEGIN SELECT RAISE(ABORT,'immutable'); END;
    CREATE TRIGGER fixture_snapshot_no_old_insert BEFORE INSERT ON fixture_mcp_membership_snapshots
      WHEN EXISTS(SELECT 1 FROM fixture_mcp_membership_heads h WHERE h.account_id=NEW.account_id AND h.track_id=NEW.track_id AND NEW.revision <= h.revision)
      BEGIN SELECT RAISE(ABORT,'published_snapshot'); END;
    CREATE TRIGGER fixture_item_no_late_insert BEFORE INSERT ON fixture_mcp_membership_items
      WHEN EXISTS(SELECT 1 FROM fixture_mcp_membership_heads h WHERE h.account_id=NEW.account_id AND h.track_id=NEW.track_id AND NEW.revision <= h.revision)
      BEGIN SELECT RAISE(ABORT,'published_snapshot'); END;
    CREATE TRIGGER fixture_head_forward_only BEFORE UPDATE ON fixture_mcp_membership_heads
      WHEN NEW.revision != OLD.revision + 1 OR NEW.item_count < 1
        OR NEW.account_id != OLD.account_id OR NEW.track_id != OLD.track_id
      BEGIN SELECT RAISE(ABORT,'head_not_forward'); END;
    CREATE TRIGGER fixture_head_no_delete BEFORE DELETE ON fixture_mcp_membership_heads
      BEGIN SELECT RAISE(ABORT,'immutable'); END;
    CREATE INDEX fixture_membership_page ON fixture_mcp_membership_items(account_id,track_id,revision,score DESC,updated_at_ms DESC,posting_id);`);
  db.exec(`INSERT INTO accounts VALUES ('owner',1,0),('foreign',1,0);
    UPDATE jobs_public_beta_cohorts SET state='open',hard_cap=2,assigned_count=2 WHERE id='public-v1';
    INSERT INTO jobs_public_beta_enrollments VALUES('public-v1','owner','admin',1),('public-v1','foreign','admin',1);
    INSERT INTO fixture_mcp_principals VALUES('principal-owner','owner'),('principal-foreign','foreign');
    INSERT INTO fixture_mcp_grants VALUES('grant-owner','owner','track',1,'${transition}',0),('grant-foreign','foreign','foreign-track',1,'${foreignTransition}',0);`);
  for (const [accountId, trackId, trackTransition] of [["owner", "track", transition], ["foreign", "foreign-track", foreignTransition]]) {
    db.prepare("INSERT INTO jobs_tracks VALUES (?,?,'{}',1,1,1)").run(trackId, accountId);
    db.prepare("INSERT INTO jobs_track_policy_track_input_transitions VALUES (?,?,?,1,0,?,?,NULL,1)")
      .run(`${accountId}-t1`, accountId, trackId, "d".repeat(64), trackTransition);
    db.prepare("INSERT INTO jobs_track_policy_track_input_heads VALUES (?,?,1,0,?,?,?,NULL,1)")
      .run(accountId, trackId, `${accountId}-t1`, "d".repeat(64), trackTransition);
  }
  seed(db, "owner", "track", transition, oldRows);
  seed(db, "foreign", "foreign-track", foreignTransition, [{ postingId: "f-1", score: 99, updatedAtMs: 1 }]);
  return db;
}

function seed(db, accountId, trackId, trackTransition, rows) {
  const hash = digest(rows);
  db.exec("BEGIN IMMEDIATE");
  try {
    db.prepare("INSERT INTO fixture_mcp_membership_snapshots VALUES (?,?,1,1,?,?,?)")
      .run(accountId, trackId, trackTransition, hash, rows.length);
    const item = db.prepare("INSERT INTO fixture_mcp_membership_items VALUES (?,?,1,?,?,?)");
    for (const row of rows) item.run(accountId, trackId, row.postingId, row.score, row.updatedAtMs);
    db.prepare("INSERT INTO fixture_mcp_membership_heads VALUES (?,?,1,?,?,1,?)")
      .run(accountId, trackId, hash, rows.length, trackTransition);
    db.exec("COMMIT");
  } catch (error) { db.exec("ROLLBACK"); throw error; }
}

function publish(db, {
  expectedRevision, revision, rows, claimedDigest = digest(rows), claimedCount = rows.length, beforeCommit,
}) {
  assert.ok(Number.isSafeInteger(expectedRevision) && expectedRevision >= 1);
  assert.ok(Number.isSafeInteger(revision) && revision > expectedRevision);
  db.exec("BEGIN IMMEDIATE");
  try {
    const track = db.prepare("SELECT input_generation, input_transition_sha256 FROM jobs_track_policy_track_input_heads WHERE account_id='owner' AND career_track_id='track'").get();
    if (!track || track.input_generation !== 1 || track.input_transition_sha256 !== transition) throw new Error("track_changed");
    if (claimedDigest !== digest(rows) || claimedCount !== rows.length) throw new Error("snapshot_mismatch");
    db.prepare("INSERT INTO fixture_mcp_membership_snapshots VALUES ('owner','track',?,?,?,?,?)")
      .run(revision, 1, transition, claimedDigest, claimedCount);
    const item = db.prepare("INSERT INTO fixture_mcp_membership_items VALUES ('owner','track',?,?,?,?)");
    for (const row of rows) item.run(revision, row.postingId, row.score, row.updatedAtMs);
    const changed = db.prepare(`UPDATE fixture_mcp_membership_heads
      SET revision=?,digest=?,item_count=?,track_generation=1,track_transition=?
      WHERE account_id='owner' AND track_id='track' AND revision=? AND digest=(SELECT digest FROM fixture_mcp_membership_snapshots WHERE account_id='owner' AND track_id='track' AND revision=?)`)
      .run(revision, claimedDigest, claimedCount, transition, expectedRevision, expectedRevision).changes;
    if (changed !== 1) throw new Error("stale_publisher");
    beforeCommit?.();
    db.exec("COMMIT");
  } catch (error) { db.exec("ROLLBACK"); throw error; }
}

const readSql = `WITH authorized AS (
  SELECT p.account_id,g.track_id,g.track_generation,g.track_transition
  FROM fixture_mcp_principals p JOIN fixture_mcp_grants g ON g.account_id=p.account_id
  JOIN accounts a ON a.id=p.account_id JOIN jobs_tracks t ON t.account_id=p.account_id AND t.id=g.track_id
  JOIN jobs_track_policy_track_input_heads h ON h.account_id=p.account_id AND h.career_track_id=g.track_id
  JOIN jobs_public_beta_enrollments e ON e.account_id=p.account_id AND e.cohort_id='public-v1'
  JOIN jobs_public_beta_cohorts c ON c.id=e.cohort_id
  WHERE p.id=$principal AND g.id=$grant AND g.revoked=0 AND t.active=1 AND a.email_verified_at IS NOT NULL
    AND a.is_temporary=0 AND c.state IN ('open','closed_to_new') AND h.input_generation=g.track_generation
    AND h.input_transition_sha256=g.track_transition AND NOT EXISTS(SELECT 1 FROM account_deletion_intents d WHERE d.account_id=p.account_id)
    AND NOT EXISTS(SELECT 1 FROM jobs_public_beta_overrides o WHERE o.account_id=p.account_id AND o.cohort_id=c.id AND o.denied<>0)
), current AS (
  SELECT a.account_id,a.track_id,h.revision,h.digest,h.item_count FROM authorized a
  JOIN fixture_mcp_membership_heads h ON h.account_id=a.account_id AND h.track_id=a.track_id
    AND h.track_generation=a.track_generation AND h.track_transition=a.track_transition
  JOIN fixture_mcp_membership_snapshots s ON s.account_id=h.account_id AND s.track_id=h.track_id AND s.revision=h.revision
    AND s.digest=h.digest AND s.item_count=h.item_count
    AND s.track_generation=h.track_generation AND s.track_transition=h.track_transition
)
SELECT c.revision,c.digest,c.item_count,i.posting_id,i.score,i.updated_at_ms FROM current c
JOIN fixture_mcp_membership_items i ON i.account_id=c.account_id AND i.track_id=c.track_id AND i.revision=c.revision
ORDER BY i.score DESC,i.updated_at_ms DESC,i.posting_id ASC LIMIT $limit`;

function readFresh(db, principal, grant, expected = undefined, limit = 10) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 10) return null;
  const rows = db.prepare(readSql).all({ $principal: principal, $grant: grant, $limit: limit }).map(row => ({ ...row }));
  if (rows.length === 0) return null;
  const first = rows[0];
  if (!rows.every(row => row.revision === first.revision && row.digest === first.digest && row.item_count === first.item_count)
    || (expected && (expected.revision !== first.revision || expected.digest !== first.digest))) return null;
  return { revision: first.revision, digest: first.digest, rows };
}

test("publisher makes only complete immutable snapshots visible and stale CAS rolls back", () => {
  const db = createDb();
  try {
    assert.throws(() => publish(db, { expectedRevision: 1, revision: 2, rows: newRows, claimedCount: 3 }), /snapshot_mismatch/);
    assert.throws(() => publish(db, {
      expectedRevision: 1, revision: 2, rows: newRows, claimedDigest: "0".repeat(64),
    }), /snapshot_mismatch/);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM fixture_mcp_membership_snapshots WHERE revision=2").get().n, 0);
    assert.throws(() => publish(db, {
      expectedRevision: 1, revision: 2, rows: newRows, beforeCommit() { throw new Error("fixture_abort"); },
    }), /fixture_abort/);
    assert.equal(readFresh(db, "principal-owner", "grant-owner").revision, 1);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM fixture_mcp_membership_items WHERE revision=2").get().n, 0);
    publish(db, { expectedRevision: 1, revision: 2, rows: newRows });
    assert.deepEqual(readFresh(db, "principal-owner", "grant-owner").rows.map(row => row.posting_id), ["m-3", "m-4"]);
    assert.throws(() => db.exec("INSERT INTO fixture_mcp_membership_items VALUES('owner','track',1,'late',1,1)"),
      /published_snapshot/);
    assert.throws(() => db.exec("DELETE FROM fixture_mcp_membership_items WHERE posting_id='m-1'"), /immutable/);
    assert.throws(() => db.exec("UPDATE fixture_mcp_membership_items SET score=99 WHERE posting_id='m-3'"), /immutable/);
    assert.throws(() => db.exec("DELETE FROM fixture_mcp_membership_heads WHERE account_id='owner'"), /immutable/);
    assert.throws(() => publish(db, { expectedRevision: 1, revision: 3, rows: oldRows }), /stale_publisher/);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM fixture_mcp_membership_snapshots WHERE revision=3").get().n, 0);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM fixture_mcp_membership_items WHERE revision=3").get().n, 0);
  } finally { db.close(); }
});

test("one WAL snapshot binds fresh authority head and page while new requests see the committed head", () => {
  const walDir = mkdtempSync(join(runRoot, "mcp-membership-"));
  const file = join(walDir, "state.sqlite");
  const writer = createDb(file); let reader, arriving;
  try {
    writer.exec("PRAGMA journal_mode=WAL");
    reader = new DatabaseSync(file, { readOnly: true }); reader.exec("BEGIN");
    arriving = new DatabaseSync(file, { readOnly: true });
    const old = readFresh(reader, "principal-owner", "grant-owner"); assert.equal(old.revision, 1);
    publish(writer, { expectedRevision: 1, revision: 2, rows: newRows, beforeCommit() {
      assert.deepEqual(readFresh(arriving, "principal-owner", "grant-owner"), old);
    } });
    assert.deepEqual(readFresh(reader, "principal-owner", "grant-owner", old), old);
    assert.equal(readFresh(arriving, "principal-owner", "grant-owner", old), null);
    assert.deepEqual(readFresh(arriving, "principal-owner", "grant-owner").rows.map(r => r.posting_id), ["m-3", "m-4"]);
    reader.exec("COMMIT");
    assert.equal(readFresh(reader, "principal-owner", "grant-owner", old), null, "old epoch cannot open fresh read");
    const fresh = readFresh(reader, "principal-owner", "grant-owner");
    assert.equal(readFresh(reader, "principal-owner", "grant-owner", fresh).revision, 2);
    assert.equal(readFresh(arriving, "principal-foreign", "grant-foreign").rows[0].posting_id, "f-1");
  } finally { arriving?.close(); reader?.close(); writer.close(); }
});

test("republication of unchanged membership still invalidates an earlier cursor revision", () => {
  const db = createDb();
  try {
    const prior = readFresh(db, "principal-owner", "grant-owner");
    publish(db, { expectedRevision: prior.revision, revision: 2, rows: oldRows });
    const current = readFresh(db, "principal-owner", "grant-owner");
    assert.equal(current.revision, 2);
    assert.equal(current.digest, prior.digest, "identical rows intentionally preserve their content digest");
    assert.equal(readFresh(db, "principal-owner", "grant-owner", prior), null);
    assert.deepEqual(readFresh(db, "principal-owner", "grant-owner", current), current);
  } finally { db.close(); }
});

test("fresh resolver derives scope from principal and denies corruption, deletion, beta and Track changes", () => {
  const db = createDb();
  try {
    assert.equal(readFresh(db, "principal-owner", "grant-foreign"), null, "caller cannot supply foreign scope");
    assert.equal(readFresh(db, "principal-foreign", "grant-foreign").rows[0].posting_id, "f-1");
    const nextDigest = digest(newRows);
    db.exec("SAVEPOINT mismatched_head");
    db.prepare("INSERT INTO fixture_mcp_membership_snapshots VALUES ('owner','track',2,1,?,?,?)")
      .run(transition, nextDigest, newRows.length);
    const item = db.prepare("INSERT INTO fixture_mcp_membership_items VALUES ('owner','track',2,?,?,?)");
    for (const row of newRows) item.run(row.postingId, row.score, row.updatedAtMs);
    db.prepare("UPDATE fixture_mcp_membership_heads SET revision=2,digest='0',item_count=?,track_generation=1,track_transition=? WHERE account_id='owner' AND track_id='track'")
      .run(newRows.length, transition);
    assert.equal(readFresh(db, "principal-owner", "grant-owner"), null, "head/snapshot digest mismatch fails closed");
    db.exec("ROLLBACK TO mismatched_head; RELEASE mismatched_head");
    for (const mutation of [
      "INSERT INTO account_deletion_intents(account_id,requested_at_ms,last_checked_at_ms,fresh_upload_cutoff_ms) VALUES('owner',1,1,1)",
      "INSERT INTO jobs_public_beta_overrides VALUES('public-v1','owner',1,1,1,1)",
      "UPDATE jobs_public_beta_cohorts SET state='suspended'", "UPDATE fixture_mcp_grants SET revoked=1 WHERE id='grant-owner'",
      "UPDATE jobs_tracks SET active=0 WHERE account_id='owner' AND id='track'",
    ]) {
      db.exec(`SAVEPOINT fence; ${mutation}`); assert.equal(readFresh(db, "principal-owner", "grant-owner"), null, mutation); db.exec("ROLLBACK TO fence; RELEASE fence");
    }
    db.exec(`BEGIN IMMEDIATE;
      INSERT INTO jobs_track_policy_track_input_transitions VALUES('owner-t2','owner','track',2,1,'${"d".repeat(64)}','${changedTransition}','${transition}',2);
      UPDATE jobs_track_policy_track_input_heads SET input_generation=2,previous_input_generation=1,input_transition_id='owner-t2',track_semantic_sha256='${"d".repeat(64)}',input_transition_sha256='${changedTransition}',predecessor_input_transition_sha256='${transition}',updated_at_ms=2 WHERE account_id='owner' AND career_track_id='track';
      COMMIT`);
    assert.equal(readFresh(db, "principal-owner", "grant-owner"), null, "ledger-valid Track advance invalidates grant");
  } finally { db.close(); }
});
