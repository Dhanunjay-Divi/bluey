// Synthetic-only SQLite issuer. No request handler, jobs route, or production grants.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

const table = "jobs_c2c_fixture_participant_consent";
const trackCurrent = readFileSync(new URL("./nonprod-c2c-contract-track-current.select.sql", import.meta.url), "utf8");
const purpose = "c2c_plan_only";
const digest = (tuple) => createHash("sha256").update(JSON.stringify(tuple)).digest("hex");
const id = (value) => typeof value === "string" && /^[a-z0-9][a-z0-9_-]{0,99}$/.test(value);
const sha = (value) => typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
const scopeSql = "account_id=? AND track_id=? AND participant_id=? AND source_id=? AND purpose=?";
const blocked = (reason) => Object.freeze({ status: "unavailable", reason });
const sourceSql = `
SELECT 1
  FROM jobs_c2c_source_rights_heads h
  JOIN jobs_c2c_source_rights_revisions r
    ON r.account_id=h.account_id AND r.source_subject_id=h.source_subject_id
   AND r.revision_id=h.revision_id AND r.canonical_sha256=h.canonical_sha256
   AND r.revision_no=h.generation
 WHERE h.account_id=? AND h.source_subject_id=? AND h.revision_id=?
   AND h.canonical_sha256=? AND h.transition_sha256=?
   AND r.purpose='c2c_plan_only' AND r.state='approved' AND r.plan_only=1
   AND r.not_before_ms<=? AND ?<r.expires_at_ms AND ?<=r.expires_at_ms
   AND NOT EXISTS (SELECT 1 FROM jobs_c2c_source_rights_revocations x
       WHERE x.account_id=r.account_id AND x.source_subject_id=r.source_subject_id)
   AND NOT EXISTS (SELECT 1 FROM jobs_c2c_source_rights_revisions n
       WHERE n.account_id=r.account_id AND n.source_subject_id=r.source_subject_id
         AND n.revision_no>r.revision_no)`;

function historical(row, replay) {
  return Object.freeze({ status: replay ? "replayed" : "recorded", revision: row.revision,
    transitionSha256: row.transition_sha256, recordedAtMs: row.recorded_ms,
    canContact: false, canAttach: false, canSubmit: false, canSign: false, canBill: false });
}

function fields(principal, request) {
  const common = ["trackId", "participantId", "sourceId", "expected", "requestId", "evidence", "state"];
  const active = ["sourceRevision", "sourceCanonical", "sourceTransition", "trackTransition", "trackHash", "expires"];
  if (!request || ![Object.prototype, null].includes(Object.getPrototypeOf(request))) return null;
  const { trackId, participantId, sourceId, expected, requestId, evidence, state } = request;
  const keys = state === "active" ? [...common, ...active] : common;
  if (Reflect.ownKeys(request).length !== keys.length || !keys.every(k => Object.hasOwn(request,k))
    || ![principal,trackId,participantId,sourceId,requestId].every(id) || !sha(evidence)
    || !["active","revoked"].includes(state) || !(expected === null || sha(expected))) return null;
  const { sourceRevision, sourceCanonical, sourceTransition, trackTransition, trackHash, expires } = request;
  if (state === "active" && (![sourceRevision,sourceCanonical,sourceTransition,trackHash].every(sha)
    || !id(trackTransition) || !Number.isSafeInteger(expires) || expires <= 0)) return null;
  const scope = [principal,trackId,participantId,sourceId,purpose];
  const basis = state === "active"
    ? [sourceRevision,sourceCanonical,sourceTransition,trackTransition,trackHash,expires] : [];
  return {scope,requestId,evidence,state,expected,basis,
    requestSha:digest(["fixture-participant-request-v1",scope,expected,state,evidence,basis])};
}

// Only called from the test harness with synthetic data. The principal is
// outside request; a real service must establish it and the evidence issuer.
export function advanceSyntheticParticipantConsent(db, principal, request) {
  const input = fields(principal,request);
  if (!input) return blocked("invalid_request");
  if (db.prepare("PRAGMA foreign_keys").get()?.foreign_keys !== 1) {
    return blocked("storage_unavailable");
  }
  const {scope,requestId,evidence,state,expected,basis,requestSha} = input;
  db.exec("BEGIN IMMEDIATE");
  try {
    const advance = () => {
      if (!db.prepare("SELECT 1 FROM accounts a WHERE a.id=? AND NOT EXISTS " +
        "(SELECT 1 FROM account_deletion_intents i WHERE i.account_id=a.id)").get(principal)) {
        return blocked("account_unavailable");
      }
      if (!db.prepare("SELECT 1 FROM jobs_c2c_fixture_participants WHERE account_id=? AND participant_id=?")
        .get(principal,scope[2])) return blocked("participant_unavailable");
      const replay = db.prepare("SELECT * FROM " + table + " WHERE account_id=? AND request_id=?")
        .get(principal,requestId);
      if (replay) {
        return replay.request_sha256 === requestSha
          ? historical(replay,true) : blocked("request_conflict");
      }
      const head = db.prepare("SELECT * FROM " + table + "_heads WHERE " + scopeSql).get(...scope);
      if ((head?.transition_sha256 ?? null) !== expected) return blocked("stale_predecessor");
      if (head?.state === "revoked" || (!head && state === "revoked")) return blocked("invalid_transition");
      const clock = db.prepare("SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER) AS now").get()?.now;
      if (!Number.isSafeInteger(clock) || clock < (head?.recorded_ms ?? 0)) return blocked("clock_unavailable");
      const [sourceRevision,sourceCanonical,sourceTransition,trackTransition,trackHash,expires] =
        state === "active" ? basis : [head.source_revision_id,head.source_canonical_sha256,
          head.source_transition_sha256,head.track_transition_id,head.track_input_sha256,clock];
      if (state === "active") {
        if (clock >= expires) return blocked("requested_window_closed");
        if (db.prepare("SELECT 1 FROM " + table + " WHERE " + scopeSql +
          " AND state='active' AND evidence_sha256=?").get(...scope,evidence)) {
          return blocked("new_evidence_required");
        }
        if (!db.prepare(trackCurrent).get({$1:principal,$2:scope[1],$3:trackTransition,$4:trackHash})) {
          return blocked("track_unavailable");
        }
        if (!db.prepare(sourceSql).get(principal,scope[3],sourceRevision,sourceCanonical,
          sourceTransition,clock,clock,expires)) return blocked("source_unavailable");
      }
      const next = (head?.revision ?? 0) + 1;
      const transition = digest(["fixture-participant-consent-v1",scope,next,expected,requestSha,
        clock,expires,sourceRevision,sourceCanonical,sourceTransition,trackTransition,trackHash]);
      db.prepare("INSERT INTO " + table + " VALUES (" + "?,".repeat(20) + "?)")
        .run(...scope,next,head?.revision ?? null,expected,transition,sourceRevision,sourceCanonical,
          sourceTransition,trackTransition,trackHash,evidence,requestId,requestSha,state,
          "synthetic-fixture",clock,expires);
      return historical({revision:next,transition_sha256:transition,recorded_ms:clock},false);
    };
    const result = advance();
    db.exec("COMMIT");
    return result;
  } catch (error) {
    db.exec("ROLLBACK");
    throw error;
  }
}
