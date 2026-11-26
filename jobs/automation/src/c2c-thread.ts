/** Read-only view of provider evidence. This module does not fetch or send messages. */
export interface C2CThreadScope {
  accountId: string;
  requirementId: string;
  vendorId: string;
  connectionId: string;
  provider: "gmail" | "outlook_email";
  providerThreadId: string;
}

export interface C2CThreadEvidence extends C2CThreadScope {
  providerMessageId: string;
  evidenceSha256: string;
  direction: "inbound" | "outbound";
  occurredAtMs: number;
}

type ThreadMessage = Readonly<Pick<C2CThreadEvidence, "provider" | "direction" | "occurredAtMs">>;
type VerifiedMessage = Readonly<Pick<C2CThreadEvidence, "evidenceSha256" | "direction" | "occurredAtMs">>;
type BlockReason = "invalid_scope" | "no_messages" | "too_many_messages" | "message_scope_mismatch" |
  "invalid_evidence" | "conflicting_message_evidence";

export type C2CThreadProjection = Readonly<
  | { state: "read_only"; messages: readonly ThreadMessage[]; canReply: false }
  | { state: "blocked"; reason: BlockReason; canReply: false }
>;

const MAX_THREAD_MESSAGES = 250;
const MAX_TIMESTAMP_MS = 8_640_000_000_000_000;
const blocked = (reason: BlockReason): C2CThreadProjection => ({ state: "blocked", reason, canReply: false });

const ownedId = (value: unknown): boolean =>
  typeof value === "string" && /^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/.test(value);
const providerId = (value: unknown): boolean =>
  typeof value === "string" && /^[A-Za-z0-9_.+:/=-]{1,512}$/.test(value);
const evidenceHash = (value: unknown): boolean =>
  typeof value === "string" && /^[a-f0-9]{64}$/.test(value);

function sameScope(scope: C2CThreadScope, row: C2CThreadEvidence): boolean {
  return row.accountId === scope.accountId && row.requirementId === scope.requirementId &&
    row.vendorId === scope.vendorId && row.connectionId === scope.connectionId &&
    row.provider === scope.provider && row.providerThreadId === scope.providerThreadId;
}

/**
 * Accepts evidence already admitted by a trusted resolver and scoped to the owner.
 * An ID or digest supplied here does not authenticate a message or grant access.
 * Direction must come from the provider evidence, never a subject or sender guess.
 */
export function projectC2CThread(
  scope: C2CThreadScope,
  evidence: readonly C2CThreadEvidence[],
): C2CThreadProjection {
  if (!scope || !ownedId(scope.accountId) || !ownedId(scope.requirementId) || !ownedId(scope.vendorId) ||
    !ownedId(scope.connectionId) || !providerId(scope.providerThreadId) ||
    (scope.provider !== "gmail" && scope.provider !== "outlook_email")) {
    return blocked("invalid_scope");
  }
  if (!Array.isArray(evidence)) return blocked("invalid_evidence");
  if (evidence.length === 0) return blocked("no_messages");
  if (evidence.length > MAX_THREAD_MESSAGES) return blocked("too_many_messages");

  // Provider identifiers and evidence fingerprints are validation inputs, not view fields.
  const messages = new Map<string, VerifiedMessage>();
  for (const row of evidence) {
    if (!row || !sameScope(scope, row)) return blocked("message_scope_mismatch");
    const { providerMessageId, evidenceSha256, direction, occurredAtMs } = row;
    if (!providerId(providerMessageId) || !evidenceHash(evidenceSha256) ||
      (direction !== "inbound" && direction !== "outbound") ||
      !Number.isSafeInteger(occurredAtMs) || occurredAtMs < 0 || occurredAtMs > MAX_TIMESTAMP_MS) {
      return blocked("invalid_evidence");
    }
    const existing = messages.get(providerMessageId);
    if (existing) {
      if (existing.evidenceSha256 !== evidenceSha256 || existing.direction !== direction ||
        existing.occurredAtMs !== occurredAtMs) return blocked("conflicting_message_evidence");
      continue;
    }
    messages.set(providerMessageId, { evidenceSha256, direction, occurredAtMs });
  }

  const ordered = [...messages.entries()]
    .sort(([leftId, left], [rightId, right]) => left.occurredAtMs - right.occurredAtMs ||
      (leftId < rightId ? -1 : leftId === rightId ? 0 : 1))
    .map(([, { direction, occurredAtMs }]) => ({ provider: scope.provider, direction, occurredAtMs }));
  return { state: "read_only", messages: ordered, canReply: false };
}
