import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";

const query = readFileSync(new URL("./nonprod-c2c-thread-read.select.sql", import.meta.url), "utf8");
const intentDDL = readFileSync(new URL("../../infra/sqlite/server-runtime/043_account_deletion_intents.sql", import.meta.url), "utf8");
const root = process.env.BLUEY_TEST_RUN_ROOT;
assert.match(root ?? "", /^\/(?:private\/)?tmp\/bluey-test-runs-[^/]+\/run\.[0-9]+\.[0-9]+\.[A-Za-z0-9]+$/);
const now = 1_800_000_000_000;
const expected = [{ provider: "gmail", direction: "inbound", occurred_at_ms: now - 10 }];

function fixture(db) {
  db.exec(`PRAGMA foreign_keys=ON;
    CREATE TABLE accounts(id TEXT PRIMARY KEY) STRICT;
    ${intentDDL}
    CREATE TABLE fixture_c2c_context (
      account_id TEXT, track_id TEXT, track_revision INTEGER, requirement_id TEXT,
      requirement_revision INTEGER, source_id TEXT, source_revision INTEGER, source_seal TEXT,
      vendor_id TEXT, active INTEGER,
      expires_at_ms INTEGER, PRIMARY KEY(account_id, track_id, requirement_id)
    ) STRICT;
    CREATE TABLE fixture_connections (
      account_id TEXT, id TEXT, provider TEXT, subject_key TEXT, revision INTEGER,
      read_grant_revision INTEGER, connected INTEGER, read_allowed INTEGER, read_expires_at_ms INTEGER,
      PRIMARY KEY(account_id,id)
    ) STRICT;
    CREATE TABLE fixture_thread_links (
      id TEXT PRIMARY KEY, account_id TEXT, connection_id TEXT, provider TEXT, thread_key TEXT,
      subject_key TEXT, connection_revision INTEGER, read_grant_revision INTEGER,
      track_id TEXT, track_revision INTEGER, requirement_id TEXT, requirement_revision INTEGER,
      source_id TEXT, source_revision INTEGER, source_seal TEXT, vendor_id TEXT, purpose TEXT, effective_at_ms INTEGER,
      expires_at_ms INTEGER, revoked_at_ms INTEGER
    ) STRICT;
    CREATE TABLE fixture_thread_evidence (
      account_id TEXT, connection_id TEXT, provider TEXT, thread_key TEXT, private_order_key TEXT,
      direction TEXT, occurred_at_ms INTEGER, retain_until_ms INTEGER NOT NULL,
      PRIMARY KEY(account_id,connection_id,provider,thread_key,private_order_key)
    ) STRICT;
    CREATE INDEX fixture_exact_thread ON fixture_thread_evidence(
      account_id, connection_id, provider, thread_key, occurred_at_ms, private_order_key
    );
    INSERT INTO accounts VALUES ('owner'), ('other');
    INSERT INTO fixture_c2c_context VALUES
      ('owner','track',2,'req',3,'source-a',4,'source-seal-1','vendor-a',1,${now + 50}),
      ('owner','track',2,'req-b',3,'source-b',4,'source-seal-2','vendor-b',1,${now + 50}),
      ('other','track',2,'req',3,'source-a',4,'source-seal-1','vendor-a',1,${now + 50});
    INSERT INTO fixture_connections VALUES
      ('owner','conn','gmail','subject-a',5,6,1,1,${now + 50}),
      ('owner','conn-2','gmail','subject-b',5,6,1,1,${now + 50}),
      ('owner','out','outlook_email','subject-c',5,6,1,1,${now + 50}),
      ('other','conn','gmail','subject-a',5,6,1,1,${now + 50});
    INSERT INTO fixture_thread_links VALUES
      ('owner-link','owner','conn','gmail','thread','subject-a',5,6,'track',2,'req',3,'source-a',4,'source-seal-1',
        'vendor-a','c2c_thread_read',${now - 20},${now + 50},NULL),
      ('out-link','owner','out','outlook_email','thread','subject-c',5,6,'track',2,'req-b',3,'source-b',4,'source-seal-2',
        'vendor-b','c2c_thread_read',${now - 20},${now + 50},NULL),
      ('other-link','other','conn','gmail','thread','subject-a',5,6,'track',2,'req',3,'source-a',4,'source-seal-1',
        'vendor-a','c2c_thread_read',${now - 20},${now + 50},NULL);
    INSERT INTO fixture_thread_evidence VALUES
      ('owner','conn','gmail','thread','same-id','inbound',${now - 10},${now + 10}),
      ('other','conn','gmail','thread','same-id','inbound',${now - 20},${now + 10}),
      ('owner','conn-2','gmail','thread','same-id','inbound',${now - 30},${now + 10}),
      ('owner','conn','gmail','other-thread','same-id','inbound',${now - 40},${now + 10}),
      ('owner','out','outlook_email','thread','same-id','inbound',${now - 50},${now + 10});`);
  return db;
}

function read(db, owner = "owner", link = "owner-link", at = now) {
  return db.prepare(query).all({ $1: owner, $2: link, $3: at }).map(row => ({ ...row }));
}

test("reads only the exact account, connection, provider, vendor link and thread", () => {
  const db = fixture(new DatabaseSync(":memory:"));
  try {
    const before = db.prepare("SELECT total_changes() AS n").get().n;
    assert.deepEqual(read(db), expected);
    const plan = db.prepare(`EXPLAIN QUERY PLAN ${query}`).all({ $1: "owner", $2: "owner-link", $3: now });
    const evidence = plan.find(row => row.detail.startsWith("SEARCH e "));
    assert.match(evidence?.detail ?? "", /fixture_exact_thread.*account_id=\? AND connection_id=\? AND provider=\? AND thread_key=\?/);
    assert.ok(!plan.some(row => row.parent === evidence.parent && row.detail.includes("TEMP B-TREE")),
      "ordering must use the exact-thread index before the capped scan");
    assert.deepEqual(read(db, "owner", "out-link"), [
      { provider: "outlook_email", direction: "inbound", occurred_at_ms: now - 50 },
    ]);
    for (const [owner, link] of [["owner", "other-link"], ["other", "owner-link"], ["owner", "unknown"]]) {
      assert.deepEqual(read(db, owner, link), []);
    }
    assert.equal(db.prepare("SELECT total_changes() AS n").get().n, before);
  } finally { db.close(); }
});

test("purpose, identity, expiry, revocation and deletion transitions never expose partial rows", () => {
  const db = fixture(new DatabaseSync(":memory:"));
  const edits = [
    ["fixture_thread_links", "purpose='c2c_plan_only'"],
    ["fixture_thread_links", "effective_at_ms=" + (now + 1)],
    ["fixture_thread_links", "expires_at_ms=" + now],
    ["fixture_thread_links", "revoked_at_ms=" + now],
    ["fixture_thread_links", "vendor_id='vendor-b'"],
    ["fixture_c2c_context", "track_revision=3"],
    ["fixture_c2c_context", "requirement_revision=4"],
    ["fixture_c2c_context", "source_id='wrong-source'"],
    ["fixture_c2c_context", "source_revision=5"],
    ["fixture_c2c_context", "source_seal='wrong-source-seal'"],
    ["fixture_c2c_context", "active=0"],
    ["fixture_c2c_context", "expires_at_ms=" + now],
    ["fixture_connections", "subject_key='replacement'"],
    ["fixture_connections", "revision=6"],
    ["fixture_connections", "read_grant_revision=7"],
    ["fixture_connections", "read_allowed=0"],
    ["fixture_connections", "connected=0"],
    ["fixture_connections", "read_expires_at_ms=" + now],
    ["fixture_thread_evidence", "retain_until_ms=" + now],
  ];
  try {
    for (const [table, edit] of edits) {
      db.exec(`SAVEPOINT one; UPDATE ${table} SET ${edit} WHERE account_id='owner';`);
      assert.deepEqual(read(db), [], `${table}: ${edit}`);
      assert.equal(read(db, "other", "other-link").length, 1);
      db.exec("ROLLBACK TO one; RELEASE one;");
      assert.deepEqual(read(db), expected);
    }
    db.exec("DELETE FROM accounts WHERE id='owner'");
    assert.deepEqual(read(db), []);
    assert.equal(read(db, "other", "other-link").length, 1);
  } finally { db.close(); }
});

test("denies unknown/outbound direction and over-cap data, never the first 250 of 251", () => {
  const db = fixture(new DatabaseSync(":memory:"));
  const insert = db.prepare(`INSERT INTO fixture_thread_evidence
    VALUES ('owner','conn','gmail','thread',?,?,?,?)`);
  try {
    for (const [direction, occurred, deadline = now + 10] of [
      [null, now - 1], ["outbound", now - 1], ["inbound", now + 1],
      ["inbound", now - 1, now], ["outbound", now - 1, now],
    ]) {
      db.exec("SAVEPOINT invalid");
      insert.run("unexpected", direction, occurred, deadline);
      assert.deepEqual(read(db), []);
      db.exec("ROLLBACK TO invalid; RELEASE invalid");
    }
    for (let i = 1; i < 250; i += 1) insert.run(`row-${i}`, "inbound", now - 1, now + 10);
    assert.equal(read(db).length, 250);
    insert.run("too-many", "inbound", now - 1, now + 10);
    assert.deepEqual(read(db), []);
    db.exec(`UPDATE fixture_thread_evidence SET retain_until_ms=${now} WHERE private_order_key='too-many'`);
    assert.deepEqual(read(db), [], "expired overflow still bounds the thread scan");
    db.exec("DELETE FROM fixture_thread_evidence WHERE private_order_key='too-many'");
    assert.equal(read(db).length, 250);
  } finally { db.close(); }
});

for (const action of ["deletion", "withdrawal"]) {
  test(`fresh read after committed ${action} refuses while the old snapshot stays consistent`, () => {
    const writer = new DatabaseSync(join(root, `c2c-thread-${action}.sqlite`));
    let reader;
    try {
      writer.exec("PRAGMA journal_mode=WAL");
      fixture(writer);
      reader = new DatabaseSync(join(root, `c2c-thread-${action}.sqlite`), { readOnly: true });
      reader.exec("BEGIN");
      assert.deepEqual(read(reader), expected);
      if (action === "deletion") {
        writer.prepare(`INSERT INTO account_deletion_intents
          (account_id,requested_at_ms,last_checked_at_ms,fresh_upload_cutoff_ms) VALUES (?,?,?,?)`)
          .run("owner", now, now, now);
      } else {
        writer.prepare("UPDATE fixture_thread_links SET revoked_at_ms=? WHERE id='owner-link'").run(now);
      }
      assert.deepEqual(read(reader), expected);
      reader.exec("COMMIT");
      assert.deepEqual(read(reader), []);
      assert.equal(read(reader, "other", "other-link").length, 1);
    } finally { reader?.close(); writer.close(); }
  });
}
