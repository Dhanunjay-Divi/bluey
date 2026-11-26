import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { describe, expect, it } from "vitest";
import {
  prepareC2CRehearsalReview,
  simulateC2CRehearsal,
  type C2CRehearsalDraft,
  type C2CRehearsalInput,
} from "../src/c2c-rehearsal.js";

const SCHEMA = readFileSync(new URL("../../scripts/nonprod-c2c-rights-db.schema.sql", import.meta.url), "utf8");
const SELECT = readFileSync(new URL("../../scripts/nonprod-c2c-rights-db.select.sql", import.meta.url), "utf8");
const DDL = readFileSync(new URL("../../../server/src/db/mod.rs", import.meta.url), "utf8");
const INTENTS = readFileSync(
  new URL("../../../infra/sqlite/server-runtime/043_account_deletion_intents.sql", import.meta.url),
  "utf8",
);
const DATE = 1_800_000_000_000;
const ownerId = "synthetic-owner";
const otherId = "synthetic-other";
const trackId = "fixture-track";
const requirementId = "fixture-requirement";
const draft: C2CRehearsalDraft = {
  recipientId: "fixture-recipient", recipientEvidenceSha256: "d".repeat(64),
  subject: "Synthetic C2C introduction", body: "Review locally. Do not contact this fixture recipient.",
  proposedDocumentSha256: "e".repeat(64), attachment_refs: [],
};

function table(name: string): string {
  const start = DDL.indexOf(`    CREATE TABLE IF NOT EXISTS ${name} (`);
  const end = DDL.indexOf("\n    );", start);
  if (start < 0 || end < start) throw new Error("fixture_schema_missing");
  return DDL.slice(start, end + "\n    );".length);
}

function seeded(): DatabaseSync {
  const db = new DatabaseSync(":memory:");
  db.exec(`PRAGMA foreign_keys=ON; ${table("accounts")} ${table("jobs_tracks")} ${INTENTS} ${SCHEMA}`);
  db.prepare("INSERT INTO accounts(id,email,password_hash) VALUES (?,?,?),(?,?,?)").run(
    ownerId, "owner@example.test", "fixture", otherId, "other@example.test", "fixture",
  );
  db.prepare("INSERT INTO jobs_tracks(id,account_id,track_json,active,created_at_ms,updated_at_ms) VALUES (?,?,'{}',1,0,0)")
    .run(trackId, ownerId);
  db.prepare("INSERT INTO jobs_c2c_fixture_source_rights VALUES ('fixture-source',1,'fixture-vendor',?,?,'approved',1,?,?)")
    .run("a".repeat(64), "b".repeat(64), DATE - 100, DATE + 100);
  db.prepare("INSERT INTO jobs_c2c_fixture_requirements VALUES (?,1,'fixture-source',1,'fixture-vendor',?,'c2c',?,?,?)")
    .run(requirementId, "a".repeat(64), DATE - 80, DATE - 70, DATE + 90);
  installGrant(db, 1, "active");
  return db;
}

function installGrant(db: DatabaseSync, revision: number, state: "active" | "revoked") {
  db.prepare("INSERT INTO jobs_c2c_fixture_grants VALUES (?,?,?,1,'fixture-vendor',?,?,?,?,?)")
    .run(ownerId, trackId, requirementId, revision, state, "c".repeat(64), DATE - 60, DATE + 80);
}

// This reducer is only a test bridge from an independently checked fixture
// SELECT. It is not an authenticated resolver, migration, or provider input.
function current(db: DatabaseSync, accountId = ownerId, nowMs = DATE): C2CRehearsalInput | null {
  const rows = db.prepare(SELECT).all({ $1: accountId, $2: trackId, $3: requirementId, $4: nowMs });
  if (rows.length === 0) return null;
  expect(rows).toHaveLength(1);
  const row = rows[0];
  expect(row).toMatchObject({ account_id: accountId, track_id: trackId, requirement_id: requirementId,
    state: "review_only", can_contact: 0, can_attach: 0, can_submit: 0 });
  const text = (key: string) => {
    const value = row[key];
    if (typeof value !== "string") throw new Error("invalid_fixture_text");
    return value;
  };
  const int = (key: string) => {
    const value = row[key];
    if (typeof value !== "number" || !Number.isSafeInteger(value)) throw new Error("invalid_fixture_integer");
    return value;
  };
  const requirement = {
    id: text("requirement_id"), revision: int("requirement_revision"), sourceId: text("source_id"),
    sourceRevision: int("source_revision"), termsSha256: text("terms_sha256"), vendorId: text("vendor_id"),
    engagement: "c2c" as const, observedAtMs: int("observed_at_ms"), verifiedAtMs: int("verified_at_ms"),
    expiresAtMs: int("requirement_expires_at_ms"),
  };
  return { accountId, trackId, nowMs, requirement, sourceRights: {
    sourceId: requirement.sourceId, sourceRevision: requirement.sourceRevision,
    termsSha256: requirement.termsSha256, vendorId: requirement.vendorId, state: "approved", permitsMatching: true,
    evidenceSha256: text("source_evidence_sha256"), effectiveAtMs: int("source_effective_at_ms"),
    expiresAtMs: int("source_expires_at_ms"),
  }, representation: {
    accountId, trackId, requirementId, requirementRevision: requirement.revision, vendorId: requirement.vendorId,
    revision: int("grant_revision"), state: "active", evidenceSha256: text("grant_evidence_sha256"),
    effectiveAtMs: int("grant_effective_at_ms"), expiresAtMs: int("grant_expires_at_ms"),
  } };
}

describe("local C2C stored rights to simulated receipt", () => {
  it("stops fresh rehearsal resolution as soon as account deletion is committed", () => {
    const db = seeded();
    try {
      const input = current(db);
      if (!input) throw new Error("fixture review unavailable");
      expect(prepareC2CRehearsalReview(input, draft).state).toBe("review_only");
      db.prepare(`INSERT INTO account_deletion_intents
        (account_id, requested_at_ms, last_checked_at_ms, fresh_upload_cutoff_ms)
        VALUES (?, ?, ?, ?)`).run(ownerId, DATE, DATE, DATE);
      expect(current(db)).toBeNull();
      expect(db.prepare("SELECT COUNT(*) AS total FROM jobs_c2c_fixture_grants").get()?.total).toBe(1);
    } finally {
      db.close();
    }
  });

  it("re-reads the owned current grant and refuses a stale review after replacement", () => {
    const db = seeded();
    try {
      const input = current(db)!;
      expect(input.representation?.revision).toBe(1);
      const review = prepareC2CRehearsalReview(input, draft);
      if (review.state !== "review_only") throw new Error("fixture review unavailable");
      const approval = { reviewSha256: review.reviewSha256, approvedAtMs: DATE, expiresAtMs: DATE + 5 };
      expect(simulateC2CRehearsal(current(db)!, draft, approval)).toEqual({ state: "simulated_only",
        reviewSha256: review.reviewSha256, attachment_refs: [], canContact: false,
        canAttach: false, canSubmit: false });
      installGrant(db, 2, "active");
      const replacement = current(db)!;
      expect(replacement.representation?.revision).toBe(2);
      expect(simulateC2CRehearsal(replacement, draft, approval)).toMatchObject({ state: "blocked", reason: "review_not_current" });
      expect(current(db, otherId)).toBeNull();
      expect(current(db, ownerId, DATE + 80)).toBeNull();
      installGrant(db, 3, "revoked");
      expect(current(db)).toBeNull();
    } finally {
      db.close();
    }
  });
});
