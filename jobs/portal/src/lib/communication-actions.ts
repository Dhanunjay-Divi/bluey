import type {
  CommunicationActionDetail,
  CommunicationActionKind,
  CommunicationActionProvider,
  CommunicationActionStatus,
  CommunicationActionSummary,
  ReviewedCommunicationPayload,
} from "../types";

const SUMMARY_KEYS = [
  "id",
  "application_id",
  "connection_id",
  "source_message_id",
  "kind",
  "provider",
  "payload_sha256",
  "action_revision",
  "status",
  "execution_available",
  "execution_unavailable_reason",
  "approved_at_ms",
  "dispatched_at_ms",
  "created_at_ms",
  "updated_at_ms",
] as const;
const DETAIL_KEYS = [
  ...SUMMARY_KEYS,
  "connection_account_label",
  "source_context",
  "payload",
] as const;
const MAX_DATE_TIMESTAMP_MS = 8_640_000_000_000_000;
const EXECUTABLE_STATUSES = new Set<CommunicationActionStatus>([
  "awaiting_approval",
  "approved",
  "needs_input",
]);

const KINDS = new Set<CommunicationActionKind>(["reply", "calendar"]);
const PROVIDERS = new Set<CommunicationActionProvider>([
  "gmail",
  "outlook_email",
  "google_calendar",
  "outlook_calendar",
]);
const STATUSES = new Set<CommunicationActionStatus>([
  "awaiting_approval",
  "approved",
  "dispatching",
  "sent",
  "calendar_created",
  "needs_input",
  "failed",
  "side_effect_unknown",
  "cancelled",
]);
const EXECUTION_UNAVAILABLE_COPY =
  "Sending from connected inboxes is not available in this release. "
  + "You can inspect or cancel this draft; Bluey will not send or create it.";
const REVIEW_CRITICAL_CONTROL = /[\p{Cc}\p{Cf}\p{Cs}\p{Zl}\p{Zp}]/u;

export interface CommunicationActionStatusPresentation {
  label: string;
  detail: string;
  tone: "accent" | "success" | "warning" | "danger" | "muted";
}

export class CommunicationVerificationError extends Error {
  constructor(message = "Bluey could not verify this reviewed communication action.") {
    super(message);
    this.name = "CommunicationVerificationError";
  }
}

export function isCommunicationVerificationError(
  cause: unknown,
): cause is CommunicationVerificationError {
  return cause instanceof CommunicationVerificationError;
}

/** Keeps the visible loading request independent from later, non-loading polls. */
export class CommunicationRequestLineage {
  private latestVersion = 0;
  private loadingVersion: number | null = null;

  begin(showLoading: boolean): number {
    const version = ++this.latestVersion;
    if (showLoading) this.loadingVersion = version;
    return version;
  }

  supersede(): void {
    this.latestVersion += 1;
  }

  isCurrent(version: number): boolean {
    return version === this.latestVersion;
  }

  finishLoading(version: number): boolean {
    if (version !== this.loadingVersion) return false;
    this.loadingVersion = null;
    return true;
  }

  clearLoading(): void {
    this.loadingVersion = null;
  }
}

export function decodeCommunicationActionSummaries(value: unknown): CommunicationActionSummary[] {
  if (!Array.isArray(value) || value.length > 100) throw invalidCommunicationData();
  const summaries = value.map((item) => decodeSummary(item, false));
  if (new Set(summaries.map((item) => item.id)).size !== summaries.length) {
    throw invalidCommunicationData();
  }
  return summaries;
}

export function decodeCommunicationActionDetail(value: unknown): CommunicationActionDetail {
  const record = objectRecord(value);
  assertExactKeys(record, DETAIL_KEYS);
  const summary = decodeSummary(record, true);
  const connectionAccountLabel = canonicalEmail(record.connection_account_label);
  const sourceContext = decodeSourceContext(record.source_context, summary.kind);
  return {
    ...summary,
    connection_account_label: connectionAccountLabel,
    source_context: sourceContext,
    payload: record.payload,
  };
}

export function reviewedCommunicationPayload(
  detail: CommunicationActionDetail,
): ReviewedCommunicationPayload {
  const record = objectRecord(detail.payload);
  if (detail.kind === "reply") {
    assertExactKeys(record, ["to", "subject", "body_text"]);
    const to = canonicalEmail(record.to);
    const subject = boundedHeaderText(record.subject, 998, false);
    const bodyText = exactFreeformText(record.body_text, 32_000);
    if (bodyText.includes("\0") || detail.source_context?.reply_target !== to) {
      throw invalidCommunicationData();
    }
    return { kind: "reply", to, subject, body_text: bodyText };
  }

  assertExactKeys(record, ["title", "starts_at_ms", "ends_at_ms", "time_zone", "attendees"]);
  const title = boundedHeaderText(record.title, 512, false);
  const startsAtMs = positiveSafeInteger(record.starts_at_ms);
  const endsAtMs = positiveSafeInteger(record.ends_at_ms);
  const timeZone = validTimeZone(record.time_zone);
  if (endsAtMs <= startsAtMs || endsAtMs - startsAtMs > 24 * 60 * 60 * 1_000) {
    throw invalidCommunicationData();
  }
  const attendees = stringArray(record.attendees, 25).map(canonicalEmail);
  if (new Set(attendees).size !== attendees.length) throw invalidCommunicationData();
  return {
    kind: "calendar",
    title,
    starts_at_ms: startsAtMs,
    ends_at_ms: endsAtMs,
    time_zone: timeZone,
    attendees,
  };
}

export async function verifiedCommunicationPayload(
  detail: CommunicationActionDetail,
): Promise<ReviewedCommunicationPayload> {
  const payload = reviewedCommunicationPayload(detail);
  const digest = await canonicalCommunicationPayloadSha256(payload);
  if (digest !== detail.payload_sha256) throw invalidCommunicationData();
  return payload;
}

export async function canonicalCommunicationPayloadSha256(
  payload: ReviewedCommunicationPayload,
): Promise<string> {
  const encoded = new TextEncoder().encode(canonicalCommunicationPayloadJson(payload));
  const digest = await crypto.subtle.digest("SHA-256", encoded);
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

export function canonicalCommunicationPayloadJson(
  payload: ReviewedCommunicationPayload,
): string {
  const value = payload.kind === "reply"
    ? {
        body_text: payload.body_text,
        subject: payload.subject,
        to: payload.to,
      }
    : {
        attendees: payload.attendees,
        ends_at_ms: payload.ends_at_ms,
        starts_at_ms: payload.starts_at_ms,
        time_zone: payload.time_zone,
        title: payload.title,
      };
  return canonicalJson(value);
}

export function communicationDetailMatchesSummary(
  summary: CommunicationActionSummary,
  detail: CommunicationActionSummary,
): boolean {
  return immutableCommunicationFieldsMatch(summary, detail)
    && detail.action_revision >= summary.action_revision
    && (detail.action_revision > summary.action_revision
      ? detail.updated_at_ms > summary.updated_at_ms
      : communicationPersistedProjectionsMatch(summary, detail));
}

export function communicationTransitionMatches(
  current: CommunicationActionDetail,
  updated: CommunicationActionDetail,
  expectedStatus: "approved" | "cancelled",
): boolean {
  if (!immutableCommunicationFieldsMatch(current, updated)
    || !communicationDetailFieldsMatch(current, updated)
    || updated.action_revision !== current.action_revision + 1
    || updated.updated_at_ms <= current.updated_at_ms
    || updated.status !== expectedStatus) {
    return false;
  }
  if (expectedStatus === "approved") {
    return updated.approved_at_ms === updated.updated_at_ms
      && updated.dispatched_at_ms === current.dispatched_at_ms;
  }
  return updated.approved_at_ms === current.approved_at_ms
    && updated.dispatched_at_ms === current.dispatched_at_ms;
}

export function reconcileCommunicationActionDetail(
  detail: CommunicationActionDetail,
  summary: CommunicationActionSummary,
): CommunicationActionDetail {
  if (!immutableCommunicationFieldsMatch(detail, summary)) throw invalidCommunicationData();
  if (summary.action_revision < detail.action_revision) {
    if (summary.updated_at_ms >= detail.updated_at_ms) throw invalidCommunicationData();
    return detail;
  }
  if (summary.action_revision === detail.action_revision) {
    if (!communicationPersistedProjectionsMatch(detail, summary)) throw invalidCommunicationData();
    return communicationReadinessProjectionsMatch(detail, summary)
      ? detail
      : { ...detail, ...communicationReadinessProjection(summary) };
  }
  if (summary.updated_at_ms <= detail.updated_at_ms) throw invalidCommunicationData();
  return { ...detail, ...summary };
}

function immutableCommunicationFieldsMatch(
  summary: CommunicationActionSummary,
  detail: CommunicationActionSummary,
): boolean {
  return summary.id === detail.id
    && summary.application_id === detail.application_id
    && summary.connection_id === detail.connection_id
    && summary.source_message_id === detail.source_message_id
    && summary.kind === detail.kind
    && summary.provider === detail.provider
    && summary.payload_sha256 === detail.payload_sha256
    && summary.created_at_ms === detail.created_at_ms;
}

function communicationDetailFieldsMatch(
  left: CommunicationActionDetail,
  right: CommunicationActionDetail,
): boolean {
  return left.connection_account_label === right.connection_account_label
    && canonicalJson(left.source_context) === canonicalJson(right.source_context)
    && canonicalJson(left.payload) === canonicalJson(right.payload);
}

export function mergeCommunicationActionSummaries(
  current: CommunicationActionSummary[],
  incoming: CommunicationActionSummary[],
): CommunicationActionSummary[] {
  const merged = new Map(current.map((item) => [item.id, item]));
  for (const next of incoming) {
    const previous = merged.get(next.id);
    if (previous && !immutableCommunicationFieldsMatch(previous, next)) {
      throw invalidCommunicationData();
    }
    if (!previous) {
      merged.set(next.id, next);
    } else if (next.action_revision > previous.action_revision) {
      if (next.updated_at_ms <= previous.updated_at_ms) throw invalidCommunicationData();
      merged.set(next.id, next);
    } else if (next.action_revision === previous.action_revision) {
      if (!communicationPersistedProjectionsMatch(previous, next)) {
        throw invalidCommunicationData();
      }
      merged.set(next.id, next);
    } else if (next.updated_at_ms >= previous.updated_at_ms) {
      throw invalidCommunicationData();
    }
  }
  return [...merged.values()].sort((left, right) => (
    right.created_at_ms - left.created_at_ms || left.id.localeCompare(right.id)
  ));
}

export function communicationActionNeedsPeriodicRefresh(
  action: CommunicationActionSummary,
): boolean {
  return ["approved", "dispatching", "side_effect_unknown"].includes(action.status);
}

export function communicationActionsNeedPeriodicRefresh(
  actions: CommunicationActionSummary[],
  reviewOpen: boolean,
): boolean {
  return reviewOpen || actions.some(communicationActionNeedsPeriodicRefresh);
}

export function communicationActionCanApprove(action: CommunicationActionSummary): boolean {
  return communicationActionRequiresReview(action) && action.execution_available;
}

export function communicationActionCanCancel(action: CommunicationActionSummary): boolean {
  return communicationActionRequiresReview(action) || action.status === "approved";
}

export function communicationActionRequiresReview(action: CommunicationActionSummary): boolean {
  return action.status === "awaiting_approval" || action.status === "needs_input";
}

export function communicationApprovalUnavailableReason(
  action: CommunicationActionSummary,
): string {
  if (!communicationActionRequiresReview(action)) {
    return action.status === "approved"
      ? "This exact draft is already approved and waiting for provider evidence."
      : "This action can no longer be approved in its current state.";
  }
  return action.execution_available
    ? ""
    : action.execution_unavailable_reason || EXECUTION_UNAVAILABLE_COPY;
}

export function communicationActionStatus(
  action: CommunicationActionSummary,
): CommunicationActionStatusPresentation {
  const item = action.kind === "reply" ? "message" : "calendar event";
  switch (action.status) {
    case "awaiting_approval":
      return {
        label: "Needs approval",
        detail: "Nothing has been sent or added to your calendar.",
        tone: "warning",
      };
    case "approved":
      return action.execution_available
        ? {
            label: "Approved · waiting",
            detail: "Approved for provider delivery; no provider completion evidence yet.",
            tone: "accent",
          }
        : {
            label: "Approved · unavailable",
            detail: `${action.execution_unavailable_reason} The ${item} has not been completed.`,
            tone: "warning",
          };
    case "dispatching":
      return {
        label: action.kind === "reply" ? "Sending…" : "Creating event…",
        detail: "A provider attempt is in progress. Do not duplicate it manually.",
        tone: "accent",
      };
    case "sent":
      return { label: "Sent", detail: "The provider confirmed the message.", tone: "success" };
    case "calendar_created":
      return {
        label: "Added to calendar",
        detail: "The provider confirmed the calendar event.",
        tone: "success",
      };
    case "needs_input":
      return {
        label: "Review again",
        detail: "Provider evidence shows no message or event was created. Review the exact draft "
          + "again before granting a new approval revision; Bluey will never retry automatically.",
        tone: "warning",
      };
    case "failed":
      return {
        label: "Delivery unverified",
        detail: `This legacy failure does not prove whether the ${item} was created. Do not retry `
          + "or repeat it without provider reconciliation and a fresh reviewed action.",
        tone: "danger",
      };
    case "side_effect_unknown":
      return {
        label: "Outcome unknown",
        detail: "Do not retry or repeat this action manually until provider reconciliation proves the outcome.",
        tone: "danger",
      };
    case "cancelled":
      return {
        label: "Cancelled",
        detail: action.dispatched_at_ms === null
          ? `This exact ${item} was cancelled before any provider attempt.`
          : `Provider evidence showed no ${item} was created; cancellation prevents any `
            + "further provider attempt.",
        tone: "muted",
      };
  }
}

export function communicationKindLabel(kind: CommunicationActionKind): string {
  return kind === "reply" ? "Recruiter reply" : "Interview calendar event";
}

export function communicationProviderLabel(provider: CommunicationActionProvider): string {
  switch (provider) {
    case "gmail": return "Gmail";
    case "outlook_email": return "Outlook Mail";
    case "google_calendar": return "Google Calendar";
    case "outlook_calendar": return "Outlook Calendar";
  }
}

export function communicationApprovalLabel(kind: CommunicationActionKind): string {
  return kind === "reply" ? "Approve reply" : "Approve calendar event";
}

export function communicationReviewConfirmation(
  kind: CommunicationActionKind,
  calendarHasAttendees = false,
): string {
  if (kind === "reply") {
    return "I reviewed the connected account, original sender, reply address, subject, "
      + "received time, recipient, and full reply.";
  }
  return calendarHasAttendees
    ? "I reviewed the connected account, title, time zone, attendees, and event details, "
      + "and understand the provider will email every listed attendee."
    : "I reviewed the connected account, title, time zone, attendees, and event details, "
      + "and understand no invitation will be sent because there are no attendees.";
}

export function communicationCancellationDescription(kind: CommunicationActionKind): string {
  return kind === "reply"
    ? "Bluey will permanently prevent this exact reply draft from being delivered. The inbound message is not deleted."
    : "Bluey will permanently prevent this exact calendar draft from being created. No mailbox message is deleted.";
}

export function communicationDefaultUnavailableCopy(): string {
  return EXECUTION_UNAVAILABLE_COPY;
}

function decodeSummary(value: unknown, detail: boolean): CommunicationActionSummary {
  const record = objectRecord(value);
  if (!detail) assertExactKeys(record, SUMMARY_KEYS);
  const kind = enumField(record.kind, KINDS);
  const provider = enumField(record.provider, PROVIDERS);
  const status = enumField(record.status, STATUSES);
  if ((kind === "reply" && !["gmail", "outlook_email"].includes(provider))
    || (kind === "calendar" && !["google_calendar", "outlook_calendar"].includes(provider))
    || (status === "sent" && kind !== "reply")
    || (status === "calendar_created" && kind !== "calendar")) {
    throw invalidCommunicationData();
  }
  const executionAvailable = booleanField(record.execution_available);
  const executionUnavailableReason = optionalBoundedText(
    record.execution_unavailable_reason,
    1_000,
  );
  if (executionAvailable === Boolean(executionUnavailableReason)) throw invalidCommunicationData();
  const sourceMessageId = nullableBoundedText(record.source_message_id, 512);
  if ((kind === "reply" && sourceMessageId === null)
    || (kind === "calendar" && sourceMessageId !== null)
    || (executionAvailable && !EXECUTABLE_STATUSES.has(status))) {
    throw invalidCommunicationData();
  }
  const approvedAtMs = nullablePositiveTimestamp(record.approved_at_ms);
  const dispatchedAtMs = nullablePositiveTimestamp(record.dispatched_at_ms);
  const createdAtMs = positiveSafeInteger(record.created_at_ms);
  const updatedAtMs = positiveSafeInteger(record.updated_at_ms);
  const actionRevision = positiveRevision(record.action_revision);
  if (updatedAtMs < createdAtMs
    || approvedAtMs !== null && (approvedAtMs < createdAtMs || approvedAtMs > updatedAtMs)
    || dispatchedAtMs !== null
      && (dispatchedAtMs < createdAtMs || dispatchedAtMs > updatedAtMs)
    || status === "awaiting_approval" && (approvedAtMs !== null || dispatchedAtMs !== null)
    || status === "awaiting_approval" && updatedAtMs !== createdAtMs
    || status === "approved" && approvedAtMs !== updatedAtMs
    || ["dispatching", "sent", "calendar_created", "side_effect_unknown"].includes(status)
      && (approvedAtMs === null || dispatchedAtMs === null)
    || status === "needs_input" && (approvedAtMs !== null || dispatchedAtMs === null)) {
    throw invalidCommunicationData();
  }
  return {
    id: boundedText(record.id, 512),
    application_id: boundedText(record.application_id, 512),
    connection_id: boundedText(record.connection_id, 512),
    source_message_id: sourceMessageId,
    kind,
    provider,
    payload_sha256: sha256(record.payload_sha256),
    action_revision: actionRevision,
    status,
    execution_available: executionAvailable,
    execution_unavailable_reason: executionUnavailableReason,
    approved_at_ms: approvedAtMs,
    dispatched_at_ms: dispatchedAtMs,
    created_at_ms: createdAtMs,
    updated_at_ms: updatedAtMs,
  };
}

function decodeSourceContext(
  value: unknown,
  kind: CommunicationActionKind,
): CommunicationActionDetail["source_context"] {
  if (kind === "calendar") {
    if (value !== null) throw invalidCommunicationData();
    return null;
  }
  const record = objectRecord(value);
  assertExactKeys(record, ["sender", "reply_target", "subject", "received_at_ms"]);
  return {
    sender: canonicalEmail(record.sender),
    reply_target: canonicalEmail(record.reply_target),
    subject: boundedHeaderText(record.subject, 998, false),
    received_at_ms: positiveSafeInteger(record.received_at_ms),
  };
}

function objectRecord(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw invalidCommunicationData();
  }
  return value as Record<string, unknown>;
}

function assertExactKeys(record: Record<string, unknown>, keys: readonly string[]): void {
  const allowed = new Set(keys);
  if (Object.keys(record).some((key) => !allowed.has(key))
    || [...allowed].some((key) => !Object.prototype.hasOwnProperty.call(record, key))) {
    throw invalidCommunicationData();
  }
}

function enumField<T extends string>(value: unknown, allowed: Set<T>): T {
  if (typeof value !== "string" || !allowed.has(value as T)) throw invalidCommunicationData();
  return value as T;
}

function booleanField(value: unknown): boolean {
  if (typeof value !== "boolean") throw invalidCommunicationData();
  return value;
}

function boundedText(value: unknown, maxLength: number): string {
  if (typeof value !== "string") throw invalidCommunicationData();
  if (!value
    || value !== value.trim()
    || utf8Length(value) > maxLength
    || REVIEW_CRITICAL_CONTROL.test(value)) {
    throw invalidCommunicationData();
  }
  return value;
}

function canonicalBoundedText(value: unknown, maxLength: number): string {
  if (typeof value !== "string"
    || !value.trim()
    || utf8Length(value) > maxLength
    || REVIEW_CRITICAL_CONTROL.test(value)) {
    throw invalidCommunicationData();
  }
  if (value !== value.trim()) throw invalidCommunicationData();
  return value;
}

function boundedHeaderText(value: unknown, maxLength: number, allowEmpty: boolean): string {
  if (typeof value !== "string" || value !== value.trim() || utf8Length(value) > maxLength) {
    throw invalidCommunicationData();
  }
  if ((!allowEmpty && !value) || REVIEW_CRITICAL_CONTROL.test(value)) {
    throw invalidCommunicationData();
  }
  return value;
}

function exactFreeformText(value: unknown, maxLength: number): string {
  if (typeof value !== "string"
    || !value.trim()
    || utf8Length(value) > maxLength
    || containsUnsafeFreeformControl(value)) {
    throw invalidCommunicationData();
  }
  return value;
}

function optionalBoundedText(value: unknown, maxLength: number): string {
  if (typeof value !== "string") throw invalidCommunicationData();
  if (value !== value.trim()
    || utf8Length(value) > maxLength
    || REVIEW_CRITICAL_CONTROL.test(value)) {
    throw invalidCommunicationData();
  }
  return value;
}

function nullableBoundedText(value: unknown, maxLength: number): string | null {
  return value === null ? null : boundedText(value, maxLength);
}

function timestamp(value: unknown): number {
  if (typeof value !== "number"
    || !Number.isSafeInteger(value)
    || value < 0
    || value > MAX_DATE_TIMESTAMP_MS
    || Number.isNaN(new Date(value).getTime())) {
    throw invalidCommunicationData();
  }
  return value;
}

function nullablePositiveTimestamp(value: unknown): number | null {
  return value === null ? null : positiveSafeInteger(value);
}

function positiveSafeInteger(value: unknown): number {
  const parsed = timestamp(value);
  if (parsed === 0) throw invalidCommunicationData();
  return parsed;
}

function positiveRevision(value: unknown): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value <= 0) {
    throw invalidCommunicationData();
  }
  return value;
}

function stringArray(value: unknown, maxLength: number): string[] {
  if (!Array.isArray(value) || value.length > maxLength || value.some((item) => typeof item !== "string")) {
    throw invalidCommunicationData();
  }
  return value as string[];
}

function sha256(value: unknown): string {
  if (typeof value !== "string" || !/^[a-f\d]{64}$/.test(value)) {
    throw invalidCommunicationData();
  }
  return value;
}

function canonicalEmail(value: unknown): string {
  const email = canonicalBoundedText(value, 320);
  if (email !== email.toLowerCase()
    || /[\p{Cc}\p{Cf}\p{Cs}\p{Zl}\p{Zp}\s]/u.test(email)) {
    throw invalidCommunicationData();
  }
  const pieces = email.split("@");
  if (pieces.length !== 2) throw invalidCommunicationData();
  const [local, domain] = pieces;
  if (!local
    || !domain
    || local.startsWith(".")
    || local.endsWith(".")
    || local.includes("..")
    || domain.startsWith(".")
    || domain.endsWith(".")
    || domain.includes("..")
    || !domain.includes(".")) {
    throw invalidCommunicationData();
  }
  return email;
}

function validTimeZone(value: unknown): string {
  const timeZone = canonicalBoundedText(value, 64);
  if (!/^[A-Za-z0-9_+-]+(?:\/[A-Za-z0-9_+-]+)*$/.test(timeZone)) {
    throw invalidCommunicationData();
  }
  try {
    new Intl.DateTimeFormat("en-US", { timeZone }).format(0);
  } catch {
    throw invalidCommunicationData();
  }
  return timeZone;
}

function invalidCommunicationData(): Error {
  return new CommunicationVerificationError();
}

function canonicalJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return `{${Object.keys(record)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(record[key])}`)
      .join(",")}}`;
  }
  const encoded = JSON.stringify(value);
  if (encoded === undefined) throw invalidCommunicationData();
  return encoded;
}

function communicationPersistedProjectionsMatch(
  left: CommunicationActionSummary,
  right: CommunicationActionSummary,
): boolean {
  return left.status === right.status
    && left.approved_at_ms === right.approved_at_ms
    && left.dispatched_at_ms === right.dispatched_at_ms
    && left.updated_at_ms === right.updated_at_ms;
}

function communicationReadinessProjectionsMatch(
  left: CommunicationActionSummary,
  right: CommunicationActionSummary,
): boolean {
  return left.execution_available === right.execution_available
    && left.execution_unavailable_reason === right.execution_unavailable_reason;
}

function communicationReadinessProjection(
  action: CommunicationActionSummary,
): Pick<CommunicationActionSummary, "execution_available" | "execution_unavailable_reason"> {
  return {
    execution_available: action.execution_available,
    execution_unavailable_reason: action.execution_unavailable_reason,
  };
}

function containsUnsafeFreeformControl(value: string): boolean {
  for (const character of value) {
    if (["\t", "\n", "\r", "\u200c", "\u200d"].includes(character)) continue;
    if (REVIEW_CRITICAL_CONTROL.test(character)) return true;
  }
  return false;
}

function utf8Length(value: string): number {
  return new TextEncoder().encode(value).length;
}
