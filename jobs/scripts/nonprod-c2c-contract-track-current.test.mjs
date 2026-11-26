import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const query = read("./nonprod-c2c-contract-track-current.select.sql");
const runtime = read("../../server/src/db/mod.rs");
const policy = read("../../infra/sqlite/server-runtime/056_jobs_canonical_taxonomy_authority.sql");
const intent = read("../../infra/sqlite/server-runtime/043_account_deletion_intents.sql");
const transitions = "jobs_track_policy_track_input_transitions";
const heads = "jobs_track_policy_track_input_heads";
const semantic = "a".repeat(64);
const firstHash = "b".repeat(64);
const nextHash = "c".repeat(64);

function section(source, start, stop) {
  const offset = source.indexOf(start);
  assert.notEqual(offset, -1, start);
  assert.equal(offset, source.lastIndexOf(start), start);
  const end = source.indexOf(stop, offset);
  assert(end > offset, stop);
  return source.slice(offset, end);
}

function fixture(t) {
  const db = new DatabaseSync(":memory:", { enableForeignKeyConstraints: true });
  t.after(() => db.close());
  const table = (source, name, indent, closing) =>
    section(source, `${indent}CREATE TABLE IF NOT EXISTS ${name} (`, closing) + closing;
  for (const name of ["accounts", "jobs_tracks"]) {
    db.exec(table(runtime, name, "    ", "\n    );"));
  }
  db.exec(intent);
  db.exec(section(policy, "CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_tracks_account_id_unique",
    ";") + ";");
  for (const name of [transitions, heads]) db.exec(table(policy, name, "", "\n);"));
  db.exec(section(policy, "DROP TRIGGER IF EXISTS trg_jobs_track_policy_track_input_transitions_validate_insert;",
    "DROP TRIGGER IF EXISTS trg_jobs_track_policy_revisions_validate_insert;"));
  // A fact is account-owned, not a foreign-key child of the live Track.
  // The separate FIX-1122 fixture qualifies immutable history and token erasure.
  db.exec(`CREATE TABLE fixture_account_contract_facts (
    account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL, transition_id TEXT NOT NULL, transition_sha256 TEXT NOT NULL
  ) STRICT;`);

  const account = db.prepare("INSERT INTO accounts(id,email,password_hash) VALUES(?,?,?)");
  for (const id of ["owner", "other"]) account.run(id, `${id}@example.test`, "synthetic");
  const track = db.prepare("INSERT INTO jobs_tracks VALUES(?,?,'{}',1,10,10)");
  track.run("target", "owner");
  track.run("other-track", "other");
  const event = db.prepare(`INSERT INTO ${transitions} VALUES(?,?,?,?,?,?,?,?,?)`);
  const head = db.prepare(`INSERT INTO ${heads} VALUES(?,?,?,?,?,?,?,?,?)`);
  const initial = (id, hash, trackId = "target", accountId = "owner") => {
    event.run(id, accountId, trackId, 1, 0, semantic, hash, null, 10);
    head.run(accountId, trackId, 1, 0, id, semantic, hash, null, 10);
  };
  initial("transition-first", firstHash);
  initial("transition-other", "d".repeat(64), "other-track", "other");
  db.prepare("INSERT INTO fixture_account_contract_facts VALUES(?,?,?,?)")
    .run("owner", "target", "transition-first", firstHash);

  const factScope = db.prepare("SELECT * FROM fixture_account_contract_facts WHERE account_id=?");
  const current = db.prepare(query);
  const lookup = ({ account_id, track_id, transition_id, transition_sha256 }) =>
    current.get({ $1: account_id, $2: track_id, $3: transition_id, $4: transition_sha256 })
      ?.track_input_current === 1;
  const advance = (updateHead = true) => {
    event.run("transition-next", "owner", "target", 2, 1, semantic, nextHash, firstHash, 11);
    if (updateHead) {
      assert.equal(db.prepare(`UPDATE ${heads} SET input_generation=2,
        previous_input_generation=1, input_transition_id='transition-next',
        input_transition_sha256=?, predecessor_input_transition_sha256=?, updated_at_ms=11
        WHERE account_id='owner' AND career_track_id='target'`).run(nextHash, firstHash).changes, 1);
    }
  };
  return { db, initial, track, factScope, lookup, advance };
}

test("exact stored Track scope is current; foreign Track and mixed transitions are not", (t) => {
  const { factScope, lookup } = fixture(t);
  const fact = factScope.get("owner");
  assert.equal(lookup(fact), true);
  assert.equal(lookup({ ...fact, account_id: "other" }), false);
  assert.equal(lookup({ ...fact, track_id: "other-track" }), false);
  assert.equal(lookup({ ...fact, transition_id: "transition-other" }), false);
  assert.equal(lookup({ ...fact, transition_sha256: nextHash }), false);
});

test("a newer committed transition hides the previous scope even before the head is advanced", (t) => {
  const { factScope, lookup, advance } = fixture(t);
  const fact = factScope.get("owner");
  advance(false);
  assert.deepEqual(factScope.get("owner"), fact);
  assert.equal(lookup(fact), false);
  assert.equal(lookup({ ...fact, transition_id: "transition-next", transition_sha256: nextHash }), false);
});

test("advancing the head keeps the old fact historical without following the new head", (t) => {
  const { factScope, lookup, advance } = fixture(t);
  const fact = factScope.get("owner");
  advance();
  assert.equal(lookup(fact), false);
  assert.equal(lookup({ ...fact, transition_id: "transition-next", transition_sha256: nextHash }), true);
  assert.deepEqual(factScope.get("owner"), fact);
});

test("deleting and recreating Track with same ID, timestamp and digest still refuses the old fact", (t) => {
  const { db, track, factScope, lookup, initial } = fixture(t);
  const fact = factScope.get("owner");
  db.exec("DELETE FROM jobs_tracks WHERE account_id='owner' AND id='target'");
  assert.equal(lookup(fact), false);
  assert.equal(db.prepare(`SELECT count(*) AS n FROM ${heads} WHERE account_id='owner'`).get().n, 0);
  track.run("target", "owner");
  assert.equal(lookup(fact), false);
  initial("transition-recreated", firstHash);
  assert.equal(lookup(fact), false);
  assert.equal(lookup({ ...fact, transition_id: "transition-recreated" }), true);
  assert.deepEqual(factScope.get("owner"), fact);
});

test("inactive Track and durable deletion intent each deny currentness independently", (t) => {
  const { db, factScope, lookup } = fixture(t);
  const fact = factScope.get("owner");
  db.exec("UPDATE jobs_tracks SET active=0 WHERE id='target'");
  assert.equal(lookup(fact), false);
  db.exec("UPDATE jobs_tracks SET active=1 WHERE id='target'");
  assert.equal(lookup(fact), true);
  db.exec(`INSERT INTO account_deletion_intents
    (account_id,requested_at_ms,last_checked_at_ms,fresh_upload_cutoff_ms) VALUES('owner',11,11,0)`);
  assert.equal(lookup(fact), false);
  assert.deepEqual(factScope.get("owner"), fact);
});
