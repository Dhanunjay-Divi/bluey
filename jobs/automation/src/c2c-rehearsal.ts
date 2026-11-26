import { createHash } from "node:crypto";

/** Phase 620D local rehearsal policy. This is deliberately not an outreach authority. */
export interface C2CRehearsalRequirement {
  id: string;
  revision: number;
  sourceId: string;
  sourceRevision: number;
  termsSha256: string;
  vendorId: string;
  engagement: "c2c" | "w2" | "1099" | "unknown";
  observedAtMs: number;
  verifiedAtMs: number;
  expiresAtMs: number;
}

export interface C2CRehearsalSourceRights {
  sourceId: string;
  sourceRevision: number;
  termsSha256: string;
  vendorId: string;
  state: "approved" | "denied" | "terms_changed" | "expired" | "review_required";
  permitsMatching: boolean;
  evidenceSha256: string;
  effectiveAtMs: number;
  expiresAtMs: number;
}

export interface C2CRehearsalRepresentation {
  accountId: string;
  trackId: string;
  requirementId: string;
  requirementRevision: number;
  vendorId: string;
  revision: number;
  state: "active" | "revoked";
  evidenceSha256: string;
  effectiveAtMs: number;
  expiresAtMs: number;
}

export interface C2CRehearsalInput {
  accountId: string;
  trackId: string;
  nowMs: number;
  requirement: C2CRehearsalRequirement;
  sourceRights: C2CRehearsalSourceRights | null;
  representation: C2CRehearsalRepresentation | null;
}

type C2CRehearsalBlockReason =
  | "requirement_unverified"
  | "source_rights_unverified"
  | "representation_unverified"
  | "invalid_draft"
  | "review_not_current";

const NO_EFFECTS = Object.freeze({ canContact: false, canAttach: false, canSubmit: false } as const);

type BlockedRehearsal = typeof NO_EFFECTS & { state: "blocked"; reason: C2CRehearsalBlockReason };
type ReviewOnly = typeof NO_EFFECTS & { state: "review_only" };
export type C2CRehearsalDecision = BlockedRehearsal | ReviewOnly;

const blocked = (reason: C2CRehearsalBlockReason): BlockedRehearsal =>
  ({ state: "blocked", reason, ...NO_EFFECTS });

const opaqueId = (value: unknown): value is string =>
  typeof value === "string" && /^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/.test(value);
const hash = (value: unknown): value is string =>
  typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const time = (value: number): boolean => Number.isSafeInteger(value) && value >= 0;
const revision = (value: number): boolean => Number.isSafeInteger(value) && value > 0;
const current = (start: number, now: number, end: number): boolean =>
  time(start) && time(end) && start <= now && now < end;

/**
 * Accepts snapshots from a trusted resolver or synthetic fixtures. Caller-supplied
 * IDs and hashes are not evidence of rights. The result can only qualify a local
 * rehearsal; no provider payload, attachment, legal or application authority exists.
 */
export function evaluateC2CRehearsal(input: C2CRehearsalInput): C2CRehearsalDecision {
  const { accountId, trackId, nowMs, requirement, sourceRights, representation } = input;
  if (!time(nowMs) || !opaqueId(accountId) || !opaqueId(trackId) || !requirement ||
    !opaqueId(requirement.id) || !opaqueId(requirement.sourceId) || !opaqueId(requirement.vendorId) ||
    !revision(requirement.revision) || !revision(requirement.sourceRevision) || !hash(requirement.termsSha256) ||
    requirement.engagement !== "c2c" || !time(requirement.observedAtMs) ||
    !current(requirement.verifiedAtMs, nowMs, requirement.expiresAtMs) ||
    requirement.observedAtMs > requirement.verifiedAtMs) {
    return blocked("requirement_unverified");
  }
  if (!sourceRights || sourceRights.state !== "approved" || sourceRights.permitsMatching !== true ||
    sourceRights.sourceId !== requirement.sourceId || sourceRights.sourceRevision !== requirement.sourceRevision ||
    sourceRights.termsSha256 !== requirement.termsSha256 || sourceRights.vendorId !== requirement.vendorId ||
    !hash(sourceRights.evidenceSha256) || !current(sourceRights.effectiveAtMs, nowMs, sourceRights.expiresAtMs) ||
    requirement.observedAtMs < sourceRights.effectiveAtMs) {
    return blocked("source_rights_unverified");
  }
  if (!representation || representation.state !== "active" || representation.accountId !== accountId ||
    representation.trackId !== trackId || representation.requirementId !== requirement.id ||
    representation.requirementRevision !== requirement.revision || representation.vendorId !== requirement.vendorId ||
    !revision(representation.revision) ||
    !hash(representation.evidenceSha256) || !current(representation.effectiveAtMs, nowMs, representation.expiresAtMs)) {
    return blocked("representation_unverified");
  }
  return { state: "review_only", ...NO_EFFECTS };
}

export interface C2CRehearsalDraft {
  recipientId: string;
  recipientEvidenceSha256: string;
  subject: string;
  body: string;
  // A digest of the locally reviewed document, not a file path or provider handle.
  proposedDocumentSha256: string | null;
  attachment_refs: readonly [];
}

export interface C2CRehearsalApproval {
  reviewSha256: string;
  approvedAtMs: number;
  expiresAtMs: number;
}

type PreparedRehearsal = ReviewOnly & { reviewSha256: string };
type SimulatedRehearsal = typeof NO_EFFECTS & {
  state: "simulated_only";
  reviewSha256: string;
  attachment_refs: readonly [];
};

const NO_ATTACHMENTS: readonly [] = Object.freeze([] as const);

function boundedPlainText(value: unknown, cap: number): value is string {
  if (typeof value !== "string" || value.length > cap || !value.trim() ||
    /[\u0000-\u0008\u000b-\u001f\u007f]/.test(value)) return false;
  const bytes = Buffer.from(value, "utf8");
  return bytes.length <= cap && bytes.toString("utf8") === value;
}

/** Prepare a receipt-shaped review fingerprint, not a provider request or approval. */
export function prepareC2CRehearsalReview(
  input: C2CRehearsalInput,
  draft: C2CRehearsalDraft,
): PreparedRehearsal | BlockedRehearsal {
  const decision = evaluateC2CRehearsal(input);
  if (decision.state === "blocked") return decision;
  if (!draft || !opaqueId(draft.recipientId) || !hash(draft.recipientEvidenceSha256) ||
    !boundedPlainText(draft.subject, 256) || /[\n\t]/.test(draft.subject) || !boundedPlainText(draft.body, 8192) ||
    (draft.proposedDocumentSha256 !== null && !hash(draft.proposedDocumentSha256)) ||
    !Array.isArray(draft.attachment_refs) || draft.attachment_refs.length !== 0) {
    return blocked("invalid_draft");
  }
  const { requirement, sourceRights, representation } = input;
  if (!sourceRights || !representation) return blocked("review_not_current");

  // Closed order and domain separation. Never stringify a caller's properties wholesale.
  const binding = JSON.stringify([
    "bluey-c2c-rehearsal-review-v2", input.accountId, input.trackId,
    requirement.id, requirement.revision, requirement.sourceId, requirement.sourceRevision,
    requirement.termsSha256, requirement.vendorId, requirement.engagement, requirement.observedAtMs,
    requirement.verifiedAtMs, requirement.expiresAtMs,
    sourceRights.evidenceSha256, sourceRights.effectiveAtMs, sourceRights.expiresAtMs,
    representation.revision, representation.evidenceSha256,
    representation.effectiveAtMs, representation.expiresAtMs,
    draft.recipientId, draft.recipientEvidenceSha256, draft.subject, draft.body, draft.proposedDocumentSha256,
  ]);
  return { ...decision, reviewSha256: createHash("sha256").update(binding, "utf8").digest("hex") };
}

/**
 * Compare an already trusted synthetic approval at the caller's current time.
 * This is a pure simulation, not signature, replay, transport or billing authority.
 */
export function simulateC2CRehearsal(
  input: C2CRehearsalInput,
  draft: C2CRehearsalDraft,
  approval: C2CRehearsalApproval | null,
): SimulatedRehearsal | BlockedRehearsal {
  const review = prepareC2CRehearsalReview(input, draft);
  if (review.state === "blocked") return review;
  if (!approval || approval.reviewSha256 !== review.reviewSha256 ||
    !current(approval.approvedAtMs, input.nowMs, approval.expiresAtMs) ||
    approval.approvedAtMs < input.requirement.verifiedAtMs ||
    !input.sourceRights || approval.approvedAtMs < input.sourceRights.effectiveAtMs ||
    !input.representation || approval.approvedAtMs < input.representation.effectiveAtMs) {
    return blocked("review_not_current");
  }
  return {
    state: "simulated_only", reviewSha256: review.reviewSha256,
    attachment_refs: NO_ATTACHMENTS, ...NO_EFFECTS,
  };
}
