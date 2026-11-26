import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";

const schema = readFileSync(new URL("./nonprod-c2c-rights-db.schema.sql", import.meta.url), "utf8");
const select = readFileSync(new URL("./nonprod-c2c-rights-db.select.sql", import.meta.url), "utf8");
const sqliteRuntime = readFileSync(
  new URL("../../server/src/db/mod.rs", import.meta.url),
  "utf8",
);
const deletionIntents = readFileSync(
  new URL("../../infra/sqlite/server-runtime/043_account_deletion_intents.sql", import.meta.url),
  "utf8",
);
const NOW = 1_800_000_000_000;
const HASH = "a".repeat(64);

function createTableDdl(source, table) {
  const start = source.indexOf(`    CREATE TABLE IF NOT EXISTS ${table} (`);
  assert.notEqual(start, -1, `missing ${table} DDL`);
  const terminator = "\n    );";
  const end = source.indexOf(terminator, start);
  assert.notEqual(end, -1, `unterminated ${table} DDL`);
  return source.slice(start, end + terminator.length);
}

const ACCOUNT_DDL = createTableDdl(sqliteRuntime, "accounts");
const TRACK_DDL = createTableDdl(sqliteRuntime, "jobs_tracks");

function database() {
  const db = new DatabaseSync(":memory:");
  db.exec("PRAGMA foreign_keys = ON");
  db.exec(ACCOUNT_DDL);
  db.exec(TRACK_DDL);
  db.exec(deletionIntents);
  db.exec(schema);
  return db;
}

function insertAccount(db, id) {
  db.prepare("INSERT INTO accounts (id, email, password_hash) VALUES (?, ?, ?)")
    .run(id, `${id}@example.test`, "fixture-password-hash");
}

function insertTrack(db, accountId, id, active = 1) {
  db.prepare(`INSERT INTO jobs_tracks
    (id, account_id, track_json, active, created_at_ms, updated_at_ms)
    VALUES (?, ?, '{}', ?, ?, ?)`)
    .run(id, accountId, active, NOW - 1, NOW - 1);
}

function insertSource(db, revision = 1, state = "approved") {
  db.prepare(`INSERT INTO jobs_c2c_fixture_source_rights
    (source_id, revision, vendor_id, terms_sha256, evidence_sha256, state, permits_matching,
     effective_at_ms, expires_at_ms)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`)
    .run("source-1", revision, "vendor-1", HASH, "b".repeat(64), state, 1, NOW - 100, NOW + 100);
}

function insertRequirement(db, revision = 1, sourceRevision = 1, expiresAtMs = NOW + 90) {
  db.prepare(`INSERT INTO jobs_c2c_fixture_requirements
    (requirement_id, revision, source_id, source_revision, vendor_id, terms_sha256, engagement,
     observed_at_ms, verified_at_ms, expires_at_ms)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
    .run("requirement-1", revision, "source-1", sourceRevision, "vendor-1", HASH, "c2c",
      NOW - 80, NOW - 70, expiresAtMs);
}

function insertGrant(db, {
  accountId = "account-1",
  trackId = "track-1",
  requirementRevision = 1,
  revision = 1,
  state = "active",
  expiresAtMs = NOW + 80,
} = {}) {
  db.prepare(`INSERT INTO jobs_c2c_fixture_grants
    (account_id, track_id, requirement_id, requirement_revision, vendor_id, revision, state,
     evidence_sha256, effective_at_ms, expires_at_ms)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
    .run(accountId, trackId, "requirement-1", requirementRevision, "vendor-1", revision, state,
      "c".repeat(64), NOW - 60, expiresAtMs);
}

function seeded() {
  const db = database();
  insertAccount(db, "account-1");
  insertTrack(db, "account-1", "track-1");
  insertSource(db);
  insertRequirement(db);
  insertGrant(db);
  return db;
}

function query(db, account = "account-1", track = "track-1", requirement = "requirement-1", now = NOW) {
  return db.prepare(select).all({ $1: account, $2: track, $3: requirement, $4: now })
    .map((row) => ({ ...row }));
}

function snapshot(db) {
  return [
    "accounts",
    "account_deletion_intents",
    "jobs_tracks",
    "jobs_c2c_fixture_source_rights",
    "jobs_c2c_fixture_requirements",
    "jobs_c2c_fixture_grants",
  ].map((table) => db.prepare(`SELECT * FROM ${table} ORDER BY 1, 2, 3`).all());
}

test("C2C rights closure returns exactly one review-only row for its exact active scope", () => {
  const db = seeded();
  try {
    assert.deepEqual(query(db), [{
      requirement_id: "requirement-1",
      requirement_revision: 1,
      source_id: "source-1",
      source_revision: 1,
      grant_revision: 1,
      vendor_id: "vendor-1",
      account_id: "account-1",
      track_id: "track-1",
      terms_sha256: HASH,
      observed_at_ms: NOW - 80,
      verified_at_ms: NOW - 70,
      requirement_expires_at_ms: NOW + 90,
      source_evidence_sha256: "b".repeat(64),
      source_effective_at_ms: NOW - 100,
      source_expires_at_ms: NOW + 100,
      grant_evidence_sha256: "c".repeat(64),
      grant_effective_at_ms: NOW - 60,
      grant_expires_at_ms: NOW + 80,
      state: "review_only",
      can_contact: 0,
      can_attach: 0,
      can_submit: 0,
    }]);
    assert.deepEqual(query(db, "account-1", "track-1", "requirement-other"), []);
    assert.deepEqual(query(db, "account-1", "track-other"), []);
    assert.deepEqual(query(db, "account-other"), []);
  } finally {
    db.close();
  }
});

test("C2C fixture schema rejects a cross-account grant foreign key", () => {
  const db = seeded();
  try {
    insertAccount(db, "account-2");
    assert.throws(() => insertGrant(db, { accountId: "account-2" }), /FOREIGN KEY/);
    assert.deepEqual(query(db, "account-2"), []);
  } finally {
    db.close();
  }
});

test("C2C fixture schema rejects a non-hex source terms digest", () => {
  const db = database();
  try {
    assert.throws(() => db.prepare(`INSERT INTO jobs_c2c_fixture_source_rights
      (source_id, revision, vendor_id, terms_sha256, evidence_sha256, state, permits_matching,
       effective_at_ms, expires_at_ms)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`)
      .run("source-invalid", 1, "vendor-1", "z".repeat(64), "b".repeat(64), "approved", 1,
        NOW - 100, NOW + 100), /CHECK/);
  } finally {
    db.close();
  }
});

test("C2C rights closure requires a current active Career Track and all links", () => {
  const db = seeded();
  try {
    db.prepare("UPDATE jobs_tracks SET active = 0 WHERE id = ?").run("track-1");
    assert.deepEqual(query(db), []);
    db.prepare("UPDATE jobs_tracks SET active = 1 WHERE id = ?").run("track-1");
    assert.equal(query(db).length, 1);

    const missing = database();
    try {
      assert.deepEqual(query(missing), []);
    } finally {
      missing.close();
    }
  } finally {
    db.close();
  }
});

test("deletion intent hides only the owner's C2C review without changing stored rights", () => {
  const db = seeded();
  try {
    insertAccount(db, "account-2");
    insertTrack(db, "account-2", "track-2");
    insertGrant(db, { accountId: "account-2", trackId: "track-2" });
    const unaffected = query(db, "account-2", "track-2");
    assert.equal(query(db).length, 1);
    assert.equal(unaffected.length, 1);
    db.prepare(`INSERT INTO account_deletion_intents
      (account_id, requested_at_ms, last_checked_at_ms, fresh_upload_cutoff_ms)
      VALUES (?, ?, ?, ?)`).run("account-1", NOW, NOW, NOW);

    db.exec("PRAGMA query_only = ON");
    const before = snapshot(db);
    assert.equal(before.at(-1).length, 2);
    assert.deepEqual(query(db), []);
    assert.deepEqual(query(db, "account-2", "track-2"), unaffected);
    assert.deepEqual(snapshot(db), before);
  } finally {
    db.close();
  }
});

test("C2C rights closure never falls back from a latest revoked grant or denied source", () => {
  const revoked = seeded();
  const denied = seeded();
  try {
    insertGrant(revoked, { revision: 2, state: "revoked" });
    assert.deepEqual(query(revoked), []);

    insertSource(denied, 2, "denied");
    assert.deepEqual(query(denied), []);
  } finally {
    revoked.close();
    denied.close();
  }
});

test("C2C rights closure never falls back from expired latest grant or newer requirement", () => {
  const expiredGrant = seeded();
  const newerRequirement = seeded();
  try {
    insertGrant(expiredGrant, { revision: 2, expiresAtMs: NOW - 1 });
    assert.deepEqual(query(expiredGrant), []);

    insertRequirement(newerRequirement, 2);
    assert.deepEqual(query(newerRequirement), []);
  } finally {
    expiredGrant.close();
    newerRequirement.close();
  }
});

test("C2C approval after a source revision requires a fresh requirement and grant", () => {
  const db = seeded();
  try {
    insertSource(db, 2);
    assert.deepEqual(query(db), []);
    insertRequirement(db, 2, 2);
    assert.deepEqual(query(db), []);
    insertGrant(db, { requirementRevision: 2, revision: 2 });
    const [row] = query(db);
    assert.deepEqual([row.requirement_revision, row.source_revision, row.grant_revision], [2, 2, 2]);
    assert.deepEqual([row.can_contact, row.can_attach, row.can_submit], [0, 0, 0]);
  } finally {
    db.close();
  }
});

test("C2C fixture rejects a requirement re-bound to another vendor", () => {
  const db = seeded();
  try {
    assert.throws(() => db.prepare(`INSERT INTO jobs_c2c_fixture_requirements
      SELECT requirement_id, 2, source_id, source_revision, ?, terms_sha256, engagement,
             observed_at_ms, verified_at_ms, expires_at_ms
        FROM jobs_c2c_fixture_requirements WHERE requirement_id = ? AND revision = 1`)
      .run("other-vendor", "requirement-1"), /FOREIGN KEY/);
    assert.equal(query(db).length, 1);
  } finally {
    db.close();
  }
});

test("C2C rights closure SELECT does not mutate fixture state", () => {
  const db = seeded();
  try {
    db.exec("PRAGMA query_only = ON");
    const before = snapshot(db);
    assert.equal(query(db).length, 1);
    assert.deepEqual(snapshot(db), before);
  } finally {
    db.close();
  }
});
