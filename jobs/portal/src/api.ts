import type {
  AccountSummary,
  ApproveApplicationResponse,
  ApplicationJobContext,
  ApplicationEvidence,
  ApplicationIdentity,
  AutoSubmitAuthorization,
  CareerFact,
  CareerProfile,
  CareerTrack,
  CareerTrackPolicyAuthority,
  CandidateEvent,
  CandidateEventInput,
  CommunicationActionSummary,
  DiscoverySource,
  DiscoverySourceCatalogResponse,
  AnswerMemory,
  AnswerMemorySaveInput,
  CompanyAnswerMemoryScope,
  Intervention,
  InterventionResolutionResult,
  JobApplication,
  JobPosting,
  UserJobInput,
  JobPreferences,
  JobsIntegration,
  JobsMatchesPage,
  JobsMatchesPageInfo,
  JobsWorkspace,
  MailboxConnection,
  MailboxMessage,
  MailboxSyncState,
  PacketCommitResult,
  QueueApplicationRunResponse,
  PrepareApplicationResponse,
  ResumeSourceMetadata,
  ResumeVersion,
  UploadResumeSourceResponse,
} from "./types";
import {
  decodeCommunicationActionDetail,
  decodeCommunicationActionSummaries,
} from "./lib/communication-actions";
import {
  decodeMailboxOAuthStart,
  decodeMailboxProviderAvailability,
} from "./lib/mailbox-oauth";
import {
  canonicalTaxonomyDescriptor,
  checkJobsTaxonomyResponse,
} from "./lib/canonical-taxonomy";
import {
  accessToken as storedAccessToken,
  loginUrl as storedLoginUrl,
  tokenStores,
} from "./auth";

export const accessToken = storedAccessToken;
export const loginUrl = storedLoginUrl;

const ACCESS_TOKEN_KEY = "bluey_access_token";
const REFRESH_TOKEN_KEY = "bluey_refresh_token";
const READ_TIMEOUT_MS = 20_000;
const WRITE_TIMEOUT_MS = 90_000;
const AUTH_TIMEOUT_MS = 15_000;
interface PortalCredentials {
  store: Storage;
  access: string;
  refresh: string;
}

interface PortalSession {
  credentials: PortalCredentials;
  refreshInFlight: Promise<boolean> | null;
}

let activeSession: PortalSession | null = null;

export class ApiError extends Error {
  status: number;
  payload: unknown;

  constructor(status: number, message: string, payload: unknown = null) {
    super(message);
    this.status = status;
    this.payload = payload;
  }
}

// Only the interview-prep client creates this error, from the exact refusal
// emitted before the Jobs router reserves usage or dispatches a provider.
export class InterviewPrepBalanceRequired extends ApiError {
  constructor() {
    super(402, "Add to your Bluey balance, then confirm again to retry this draft.");
  }
}

export type JobsBetaAccessState = "admitted" | "not_admitted" | "suspended";
export type JobsBetaAccessReason =
  | "admitted"
  | "verification_required"
  | "not_open"
  | "window_closed"
  | "capacity_reached"
  | "denied"
  | "suspended"
  | "unavailable";

export type JobsBetaAccess =
  | { schemaVersion: 1; access: "admitted"; reason: "admitted" }
  | {
      schemaVersion: 1;
      access: "not_admitted";
      reason: Exclude<JobsBetaAccessReason, "admitted" | "suspended">;
    }
  | { schemaVersion: 1; access: "suspended"; reason: "suspended" };

export interface JobsPortalLoadResult {
  betaAccess: JobsBetaAccess;
  workspace: JobsWorkspace | null;
  account: AccountSummary | null;
}

const BETA_ACCESS_STATES = new Set<JobsBetaAccessState>([
  "admitted",
  "not_admitted",
  "suspended",
]);
const BETA_ACCESS_REASONS = new Set<JobsBetaAccessReason>([
  "admitted",
  "verification_required",
  "not_open",
  "window_closed",
  "capacity_reached",
  "denied",
  "suspended",
  "unavailable",
]);
const NOT_ADMITTED_REASONS = new Set<JobsBetaAccessReason>([
  "verification_required",
  "not_open",
  "window_closed",
  "capacity_reached",
  "denied",
  "unavailable",
]);

function invalidBetaAccess(): never {
  throw new ApiError(
    0,
    "Bluey Jobs could not safely confirm public-beta access.",
  );
}

export function parseJobsBetaAccess(payload: unknown): JobsBetaAccess {
  if (!payload || typeof payload !== "object" || Array.isArray(payload))
    invalidBetaAccess();
  const record = payload as Record<string, unknown>;
  const keys = Object.keys(record).sort();
  if (
    keys.length !== 3 ||
    keys[0] !== "access" ||
    keys[1] !== "reason" ||
    keys[2] !== "schemaVersion"
  ) {
    invalidBetaAccess();
  }
  if (record.schemaVersion !== 1) invalidBetaAccess();
  if (
    typeof record.access !== "string" ||
    !BETA_ACCESS_STATES.has(record.access as JobsBetaAccessState)
  ) {
    invalidBetaAccess();
  }
  if (
    typeof record.reason !== "string" ||
    !BETA_ACCESS_REASONS.has(record.reason as JobsBetaAccessReason)
  ) {
    invalidBetaAccess();
  }

  const access = record.access as JobsBetaAccessState;
  const reason = record.reason as JobsBetaAccessReason;
  const consistent =
    (access === "admitted" && reason === "admitted") ||
    (access === "suspended" && reason === "suspended") ||
    (access === "not_admitted" && NOT_ADMITTED_REASONS.has(reason));
  if (!consistent) invalidBetaAccess();
  return { schemaVersion: 1, access, reason } as JobsBetaAccess;
}

export interface InterviewPrepCompletionResponse {
  schema_version: 1;
  id: string;
  application_id: string;
  content: string;
  generated_at_ms: number;
  provider: string;
  model: string;
  cost_cents: number;
  balance_cents_after: number;
  trial_seconds_remaining: number;
  cost_label: string | null;
  confidence: number | null;
  grounding: {
    receipt_id: string;
    receipt_fingerprint: string;
    resume_version_id: string;
    resume_checksum: string;
    resume_document_sha256: string;
    answer_keys_used: string[];
    answer_keys_omitted: string[];
  };
}

const INTERVIEW_PREP_KEYS = [
  "application_id",
  "balance_cents_after",
  "confidence",
  "content",
  "cost_cents",
  "cost_label",
  "generated_at_ms",
  "grounding",
  "id",
  "model",
  "provider",
  "schema_version",
  "trial_seconds_remaining",
];
const INTERVIEW_PREP_GROUNDING_KEYS = [
  "answer_keys_omitted",
  "answer_keys_used",
  "receipt_fingerprint",
  "receipt_id",
  "resume_checksum",
  "resume_document_sha256",
  "resume_version_id",
];
const SHA256_HEX = /^[a-f0-9]{64}$/;
const PREP_ID = /^prep-[a-f0-9]{24}$/;
const CONTROL_CHARACTERS = /[\u0000-\u001f\u007f]/;
const UNSAFE_TEXT_CONTROL_CHARACTERS = /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/;

function invalidInterviewPrepResponse(): never {
  throw new ApiError(0, "Bluey Jobs returned an invalid interview preparation response.");
}

function hasExactKeys(record: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(record).sort();
  return keys.length === expected.length && keys.every((key, index) => key === expected[index]);
}

function boundedString(value: unknown, maximum: number): value is string {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    value.length <= maximum &&
    !CONTROL_CHARACTERS.test(value)
  );
}

function boundedText(value: unknown, maximum: number): value is string {
  return (
    typeof value === "string" &&
    value.trim().length > 0 &&
    value.length <= maximum &&
    !UNSAFE_TEXT_CONTROL_CHARACTERS.test(value)
  );
}

function boundedSafeInteger(value: unknown, minimum: number, maximum: number): value is number {
  return (
    typeof value === "number" &&
    Number.isSafeInteger(value) &&
    value >= minimum &&
    value <= maximum
  );
}

function validApplicationId(value: unknown): value is string {
  return (
    boundedString(value, 256) &&
    !value.includes("..") &&
    /^[A-Za-z0-9][A-Za-z0-9_./:-]*$/.test(value)
  );
}

function decodeAnswerKeys(value: unknown): string[] | null {
  if (!Array.isArray(value) || value.length > 1_024) return null;
  const keys: string[] = [];
  const seen = new Set<string>();
  for (const item of value) {
    if (!boundedString(item, 256) || seen.has(item)) return null;
    seen.add(item);
    keys.push(item);
  }
  return keys;
}

/** Decodes the closed v1 server response before interview content reaches the UI. */
export function decodeInterviewPrepResponse(
  payload: unknown,
  expectedApplicationId: string,
): InterviewPrepCompletionResponse {
  if (!validApplicationId(expectedApplicationId)) invalidInterviewPrepResponse();
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
    invalidInterviewPrepResponse();
  }
  const record = payload as Record<string, unknown>;
  if (!hasExactKeys(record, INTERVIEW_PREP_KEYS) || record.schema_version !== 1) {
    invalidInterviewPrepResponse();
  }
  if (
    typeof record.id !== "string" ||
    !PREP_ID.test(record.id) ||
    record.application_id !== expectedApplicationId ||
    !validApplicationId(record.application_id) ||
    !boundedText(record.content, 32_768) ||
    !boundedSafeInteger(record.generated_at_ms, 0, 8_640_000_000_000) ||
    !boundedString(record.provider, 128) ||
    !boundedString(record.model, 256) ||
    !boundedSafeInteger(record.cost_cents, 0, 1_000_000) ||
    !boundedSafeInteger(record.balance_cents_after, -1_000_000_000, 1_000_000_000) ||
    !boundedSafeInteger(record.trial_seconds_remaining, 0, 31_536_000) ||
    (record.cost_label !== null && !boundedString(record.cost_label, 128)) ||
    (record.confidence !== null &&
      (typeof record.confidence !== "number" ||
        !Number.isFinite(record.confidence) ||
        record.confidence < 0 ||
        record.confidence > 1))
  ) {
    invalidInterviewPrepResponse();
  }

  if (
    !record.grounding ||
    typeof record.grounding !== "object" ||
    Array.isArray(record.grounding)
  ) {
    invalidInterviewPrepResponse();
  }
  const grounding = record.grounding as Record<string, unknown>;
  if (
    !hasExactKeys(grounding, INTERVIEW_PREP_GROUNDING_KEYS) ||
    !boundedString(grounding.receipt_id, 256) ||
    typeof grounding.receipt_fingerprint !== "string" ||
    !SHA256_HEX.test(grounding.receipt_fingerprint) ||
    !boundedString(grounding.resume_version_id, 256) ||
    !boundedString(grounding.resume_checksum, 256) ||
    typeof grounding.resume_document_sha256 !== "string" ||
    !SHA256_HEX.test(grounding.resume_document_sha256)
  ) {
    invalidInterviewPrepResponse();
  }
  const answerKeysUsed = decodeAnswerKeys(grounding.answer_keys_used);
  const answerKeysOmitted = decodeAnswerKeys(grounding.answer_keys_omitted);
  if (
    !answerKeysUsed ||
    !answerKeysOmitted ||
    answerKeysUsed.some((key) => answerKeysOmitted.includes(key))
  ) {
    invalidInterviewPrepResponse();
  }

  return {
    schema_version: 1,
    id: record.id as string,
    application_id: record.application_id as string,
    content: record.content as string,
    generated_at_ms: record.generated_at_ms as number,
    grounding: {
      receipt_id: grounding.receipt_id as string,
      receipt_fingerprint: grounding.receipt_fingerprint as string,
      resume_version_id: grounding.resume_version_id as string,
      resume_checksum: grounding.resume_checksum as string,
      resume_document_sha256: grounding.resume_document_sha256 as string,
      answer_keys_used: answerKeysUsed,
      answer_keys_omitted: answerKeysOmitted,
    },
    provider: record.provider as string,
    model: record.model as string,
    cost_cents: record.cost_cents as number,
    balance_cents_after: record.balance_cents_after as number,
    trial_seconds_remaining: record.trial_seconds_remaining as number,
    cost_label: record.cost_label as string | null,
    confidence: record.confidence as number | null,
  };
}

function readPortalCredentials(): PortalCredentials | null {
  try {
    for (const store of tokenStores()) {
      const access = store.getItem(ACCESS_TOKEN_KEY);
      if (access) return { store, access, refresh: store.getItem(REFRESH_TOKEN_KEY) ?? "" };
    }
  } catch {
    // Storage denial is not authentication or refresh authority.
  }
  return null;
}

function sameCredentials(left: PortalCredentials, right: PortalCredentials | null): boolean {
  return right !== null && left.store === right.store && left.access === right.access && left.refresh === right.refresh;
}

function capturePortalSession(): PortalSession | null {
  const credentials = readPortalCredentials();
  if (!activeSession || !sameCredentials(activeSession.credentials, credentials)) {
    activeSession = credentials ? { credentials, refreshInFlight: null } : null;
  }
  return activeSession;
}

function isCurrentSession(session: PortalSession): boolean {
  return activeSession === session && sameCredentials(session.credentials, readPortalCredentials());
}

function requireCurrentSession(session: PortalSession | null): void {
  if (session && !isCurrentSession(session)) throw new ApiError(401, "Bluey Jobs session changed.");
}

function timeoutFor(init: RequestInit): number {
  const method = String(init.method || "GET").toUpperCase();
  return method === "GET" || method === "HEAD"
    ? READ_TIMEOUT_MS
    : WRITE_TIMEOUT_MS;
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

async function fetchWithTimeout(
  input: RequestInfo | URL,
  init: RequestInit = {},
  timeoutMs = timeoutFor(init),
): Promise<Response> {
  const controller = new AbortController();
  const timer = globalThis.setTimeout(() => controller.abort(), timeoutMs);
  const onAbort = () => controller.abort();
  init.signal?.addEventListener("abort", onAbort, { once: true });
  try {
    return await fetch(input, { ...init, signal: controller.signal });
  } finally {
    globalThis.clearTimeout(timer);
    init.signal?.removeEventListener("abort", onAbort);
  }
}

async function refreshAccessToken(session: PortalSession, attemptedToken: string): Promise<boolean> {
  if (!isCurrentSession(session)) return false;
  // A concurrent request already rotated this exact in-memory session.
  if (session.credentials.access !== attemptedToken) return true;
  if (session.refreshInFlight) return session.refreshInFlight;
  const initial = session.credentials;
  if (!initial.refresh) return false;
  session.refreshInFlight = (async () => {
    let response: Response;
    try {
      response = await fetchWithTimeout(
        "/auth/refresh",
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ refresh_token: initial.refresh }),
        },
        AUTH_TIMEOUT_MS,
      );
    } catch (error) {
      if (isAbortError(error)) return false;
      throw error;
    }
    if (!response.ok) return false;
    const payload = (await response.json()) as {
      access_token?: string;
      refresh_token?: string;
    };
    if (!payload || typeof payload.access_token !== "string" || !payload.access_token) return false;
    if (payload.refresh_token !== undefined
      && (typeof payload.refresh_token !== "string" || !payload.refresh_token)) return false;
    if (session.credentials !== initial || !isCurrentSession(session)) return false;
    const next = { store: initial.store, access: payload.access_token, refresh: payload.refresh_token ?? initial.refresh };
    // The final session check and updates are synchronous. Do not write
    // to the other storage area or re-read an unrelated token for replay.
    try {
      initial.store.setItem(ACCESS_TOKEN_KEY, next.access);
      initial.store.setItem(REFRESH_TOKEN_KEY, next.refresh);
    } catch {
      if (activeSession === session) activeSession = null;
      // A failed second write may have left a partial pair. Clear only
      // that pair; do not remove a replacement from another tab.
      try {
        const refresh = initial.store.getItem(REFRESH_TOKEN_KEY);
        if (initial.store.getItem(ACCESS_TOKEN_KEY) === next.access
          && (refresh === initial.refresh || refresh === next.refresh)) {
          initial.store.removeItem(ACCESS_TOKEN_KEY);
          initial.store.removeItem(REFRESH_TOKEN_KEY);
        }
      } catch {
        // Unavailable storage cannot authorize another request.
      }
      return false;
    }
    session.credentials = next;
    return true;
  })();
  try {
    return await session.refreshInFlight;
  } finally {
    session.refreshInFlight = null;
  }
}

async function request<T>(
  path: string,
  init: RequestInit = {},
  retrySession?: PortalSession,
): Promise<T> {
  const session = retrySession ?? capturePortalSession();
  if (retrySession) requireCurrentSession(retrySession);
  const headers = new Headers(init.headers);
  if (!(init.body instanceof FormData))
    headers.set("Content-Type", "application/json");
  const token = session?.credentials.access ?? "";
  if (token) headers.set("Authorization", `Bearer ${token}`);
  let response: Response;
  try {
    response = await fetchWithTimeout(path, { ...init, headers });
  } catch (error) {
    if (isAbortError(error)) {
      throw new ApiError(
        0,
        "Bluey Jobs is taking longer than expected. Please try again.",
      );
    }
    throw error;
  }
  requireCurrentSession(session);
  if (response.status === 401 && !retrySession && session) {
    const refreshed = await refreshAccessToken(session, token);
    if (refreshed && isCurrentSession(session)) return request<T>(path, init, session);
  }
  let body: unknown = null;
  if (response.status !== 204) {
    const text = await response.text();
    requireCurrentSession(session);
    try {
      body = text ? JSON.parse(text) : null;
    } catch {
      body = text;
    }
  }
  if (!response.ok) {
    const message =
      typeof body === "string"
        ? body
        : typeof body === "object" && body && "message" in body
          ? String((body as { message: unknown }).message)
          : "Bluey Jobs could not finish that request.";
    throw new ApiError(response.status, message, body);
  }
  return body as T;
}

async function taxonomyWriteHeaders(): Promise<Headers> {
  const local = await canonicalTaxonomyDescriptor();
  const server = await request<unknown>("/api/jobs/taxonomy");
  const check = checkJobsTaxonomyResponse(server, local);
  if (check.status !== "current" || !local.digest_sha256) {
    throw new ApiError(
      409,
      "Career Track taxonomy changed. Refresh Bluey Jobs and review this Track before saving.",
    );
  }
  return new Headers({
    "X-Bluey-Jobs-Taxonomy-Version": local.taxonomy_version,
    "X-Bluey-Jobs-Taxonomy-SHA256": local.digest_sha256,
  });
}

type WireCareerTrackPolicyAuthority = Omit<
  CareerTrackPolicyAuthority,
  "applicationIdentityId"
> & {
  application_identity_id?: unknown;
  applicationIdentityId?: unknown;
};

function policyAuthorityFromWire(
  authority: WireCareerTrackPolicyAuthority,
): CareerTrackPolicyAuthority {
  const {
    application_identity_id: applicationIdentityIdFromWire,
    applicationIdentityId: existingApplicationIdentityId,
    ...rest
  } = authority;
  return {
    ...rest,
    applicationIdentityId:
      typeof applicationIdentityIdFromWire === "string"
        ? applicationIdentityIdFromWire
        : typeof existingApplicationIdentityId === "string"
          ? existingApplicationIdentityId
          : "",
  };
}

function trackFromWire(track: CareerTrack): CareerTrack {
  if (!track.policy?.authority) return track;
  return {
    ...track,
    policy: {
      ...track.policy,
      authority: policyAuthorityFromWire(
        track.policy.authority as unknown as WireCareerTrackPolicyAuthority,
      ),
    },
  };
}

function workspaceFromWire(workspace: JobsWorkspace): JobsWorkspace {
  const matchesPage = matchesPageInfoFromWire(
    (workspace as Partial<JobsWorkspace>).matches_page,
    Array.isArray(workspace.matches) ? workspace.matches.length : 0,
  );
  if (Array.isArray(workspace.matches) && workspace.matches.length > matchesPage.page_size) {
    throw new ApiError(0, "Bluey Jobs returned an invalid match page.");
  }
  const applicationJobs = applicationJobsFromWire(workspace);
  if (!Array.isArray(workspace.tracks)) {
    return {
      ...workspace,
      application_jobs: applicationJobs,
      matches_page: matchesPage,
    };
  }
  return {
    ...workspace,
    application_jobs: applicationJobs,
    tracks: workspace.tracks.map(trackFromWire),
    matches_page: matchesPage,
  };
}

function applicationJobsFromWire(workspace: JobsWorkspace): ApplicationJobContext[] {
  const raw = (workspace as Partial<JobsWorkspace>).application_jobs;
  if (raw === undefined) return [];
  if (!Array.isArray(raw) || !Array.isArray(workspace.matches)) {
    throw new ApiError(0, "Bluey Jobs returned invalid application job context.");
  }
  const matchIds = new Set(workspace.matches.map((job) => job.id));
  const seen = new Set<string>();
  for (const value of raw) {
    const keys = value && typeof value === "object"
      ? Object.keys(value).sort().join(",")
      : "";
    if (
      !value
      || typeof value !== "object"
      || keys !== "company,id,location,title,track_id"
      || typeof value.id !== "string"
      || value.id.length === 0
      || typeof value.company !== "string"
      || value.company.trim().length === 0
      || [...value.company].length > 200
      || typeof value.title !== "string"
      || value.title.trim().length === 0
      || [...value.title].length > 500
      || typeof value.location !== "string"
      || [...value.location].length > 500
      || typeof value.track_id !== "string"
      || [...value.track_id].length > 500
      || seen.has(value.id)
      || matchIds.has(value.id)
    ) {
      throw new ApiError(0, "Bluey Jobs returned invalid application job context.");
    }
    seen.add(value.id);
  }
  if (Array.isArray(workspace.applications)) {
    for (const application of workspace.applications) {
      if (!matchIds.has(application.job_id) && !seen.has(application.job_id)) {
        throw new ApiError(0, "Bluey Jobs returned incomplete application job context.");
      }
    }
  }
  return raw;
}

function matchesPageInfoFromWire(
  value: unknown,
  fallbackSize: number,
): JobsMatchesPageInfo {
  if (value === undefined) {
    return {
      schema_version: 1,
      page_size: Math.max(1, fallbackSize),
      has_more: false,
      next_cursor: null,
    };
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new ApiError(0, "Bluey Jobs returned an invalid match page.");
  }
  const record = value as Record<string, unknown>;
  const keys = Object.keys(record).sort();
  if (
    keys.join(",") !== "has_more,next_cursor,page_size,schema_version" ||
    record.schema_version !== 1 ||
    !Number.isInteger(record.page_size) ||
    Number(record.page_size) < 1 ||
    Number(record.page_size) > 100 ||
    typeof record.has_more !== "boolean" ||
    (record.next_cursor !== null && typeof record.next_cursor !== "string") ||
    (typeof record.next_cursor === "string" && record.next_cursor.length > 2_048) ||
    (record.has_more &&
      (typeof record.next_cursor !== "string" || record.next_cursor.length === 0)) ||
    (!record.has_more && record.next_cursor !== null)
  ) {
    throw new ApiError(0, "Bluey Jobs returned an invalid match page.");
  }
  return record as unknown as JobsMatchesPageInfo;
}

export function matchesPageFromWire(value: unknown): JobsMatchesPage {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new ApiError(0, "Bluey Jobs returned an invalid match page.");
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).sort().join(",") !== "matches,page" ||
    !Array.isArray(record.matches)
  ) {
    throw new ApiError(0, "Bluey Jobs returned an invalid match page.");
  }
  const page = matchesPageInfoFromWire(record.page, record.matches.length);
  if (record.matches.length > page.page_size) {
    throw new ApiError(0, "Bluey Jobs returned an invalid match page.");
  }
  return { matches: record.matches as JobPosting[], page };
}

function trackToWire(track: CareerTrack): CareerTrack {
  if (!track.policy?.authority) return track;
  const { applicationIdentityId, ...authority } = track.policy.authority;
  return {
    ...track,
    policy: {
      ...track.policy,
      authority: {
        ...authority,
        application_identity_id: applicationIdentityId,
      } as unknown as CareerTrackPolicyAuthority,
    },
  };
}

async function requestDownload(
  path: string,
  fallbackName: string,
  retrySession?: PortalSession,
): Promise<{ blob: Blob; fileName: string }> {
  const session = retrySession ?? capturePortalSession();
  if (retrySession) requireCurrentSession(retrySession);
  const headers = new Headers();
  const token = session?.credentials.access ?? "";
  if (token) headers.set("Authorization", `Bearer ${token}`);
  let response: Response;
  try {
    response = await fetchWithTimeout(path, { headers }, WRITE_TIMEOUT_MS);
  } catch (error) {
    if (isAbortError(error)) {
      throw new ApiError(
        0,
        "Bluey could not finish the download in time. Please try again.",
      );
    }
    throw error;
  }
  requireCurrentSession(session);
  if (response.status === 401 && !retrySession && session) {
    const refreshed = await refreshAccessToken(session, token);
    if (refreshed && isCurrentSession(session)) return requestDownload(path, fallbackName, session);
  }
  if (!response.ok) {
    const text = await response.text();
    requireCurrentSession(session);
    throw new ApiError(
      response.status,
      text || "Bluey could not download that file.",
    );
  }
  const disposition = response.headers.get("content-disposition") || "";
  const match = disposition.match(/filename="([^"]+)"/i);
  const blob = await response.blob();
  requireCurrentSession(session);
  return { blob, fileName: match?.[1] || fallbackName };
}

function resumeMediaType(file: File): string {
  const extension = file.name.split(".").pop()?.toLowerCase();
  if (extension === "docx") {
    return "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
  }
  if (extension === "pdf") return "application/pdf";
  if (extension === "txt") return "text/plain";
  return file.type || "application/octet-stream";
}

async function fileBase64(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  const chunks: string[] = [];
  for (let offset = 0; offset < bytes.length; offset += 32_768) {
    chunks.push(
      String.fromCharCode(...bytes.subarray(offset, offset + 32_768)),
    );
  }
  return btoa(chunks.join(""));
}

export const jobsApi = {
  betaAccess: async () => {
    try {
      return parseJobsBetaAccess(
        await request<unknown>("/api/jobs/beta-access", { cache: "no-store" }),
      );
    } catch (error) {
      if (error instanceof ApiError && error.status === 503) {
        return parseJobsBetaAccess(error.payload);
      }
      throw error;
    }
  },
  workspace: async () =>
    workspaceFromWire(
      await request<JobsWorkspace>("/api/jobs/workspace", { cache: "no-store" }),
    ),
  matchesPage: async (cursor: string, limit = 50) => {
    return matchesPageFromWire(
      await request<unknown>("/api/jobs/matches/page", {
        method: "POST",
        cache: "no-store",
        body: JSON.stringify({ cursor, limit }),
      }),
    );
  },
  matchDetail: (jobId: string) =>
    request<JobPosting>(`/api/jobs/matches/${encodeURIComponent(jobId)}`, {
      cache: "no-store",
    }),
  account: () => request<AccountSummary>("/account/me"),
  completeOnboarding: async (
    profile: CareerProfile,
    preferences: JobPreferences,
    track: CareerTrack,
  ) =>
    workspaceFromWire(
      await request<JobsWorkspace>("/api/jobs/onboarding/complete", {
        method: "POST",
        headers: await taxonomyWriteHeaders(),
        body: JSON.stringify({
          profile,
          preferences,
          track: trackToWire(track),
        }),
      }),
    ),
  prepareInterview: async (applicationId: string, prepare: true, receiptFingerprint: string) => {
    if (prepare !== true) {
      return Promise.reject(new Error("Interview preparation requires opt-in."));
    }
    if (typeof receiptFingerprint !== "string" || receiptFingerprint.length !== 64
      || !SHA256_HEX.test(receiptFingerprint)) {
      throw new Error("Refresh Bluey Jobs to confirm the submitted receipt before preparing.");
    }
    let payload: unknown;
    try {
      payload = await request<unknown>(
        `/api/jobs/applications/${encodeURIComponent(applicationId)}/interview-prep`,
        { method: "POST", body: JSON.stringify({ prepare, receipt_fingerprint: receiptFingerprint }) },
      );
    } catch (cause) {
      // Status alone is insufficient. Never infer replay authority from a
      // timeout, unknown gateway reply, or a different payment refusal.
      if (cause instanceof ApiError && cause.status === 402 && cause.payload === "insufficient balance") {
        throw new InterviewPrepBalanceRequired();
      }
      throw cause;
    }
    const draft = decodeInterviewPrepResponse(payload, applicationId);
    if (draft.grounding.receipt_fingerprint !== receiptFingerprint) {
      throw new ApiError(0, "The submitted receipt changed. Reopen Interview Prep to continue.");
    }
    return draft;
  },
  saveProfile: (profile: CareerProfile) =>
    request<CareerProfile>("/api/jobs/profile", {
      method: "PUT",
      body: JSON.stringify(profile),
    }),
  resumeSource: () =>
    request<ResumeSourceMetadata | null>("/api/jobs/resume-source"),
  uploadResumeSource: async (
    file: File,
    profile: CareerProfile,
    pageCount?: number,
    requestId = crypto.randomUUID(),
  ) =>
    request<UploadResumeSourceResponse>("/api/jobs/resume-source", {
      method: "POST",
      body: JSON.stringify({
        request_id: requestId,
        file_name: file.name,
        media_type: resumeMediaType(file),
        bytes_base64: await fileBase64(file),
        page_count: pageCount,
        profile,
      }),
    }),
  downloadTemplateResume: (resumeVersionId: string) =>
    requestDownload(
      `/api/jobs/resume-versions/${encodeURIComponent(resumeVersionId)}/template-docx`,
      "Bluey-tailored-resume.docx",
    ),
  savePreferences: (preferences: JobPreferences) =>
    request<JobPreferences>("/api/jobs/preferences", {
      method: "PUT",
      body: JSON.stringify(preferences),
    }),
  saveFact: (fact: CareerFact) =>
    request<CareerFact>("/api/jobs/facts", {
      method: "POST",
      body: JSON.stringify({
        id: fact.id,
        category: fact.category,
        label: fact.label,
        value: fact.value,
      }),
    }),
  deleteFact: (id: string) =>
    request<void>(`/api/jobs/facts/${encodeURIComponent(id)}`, {
      method: "DELETE",
    }),
  saveTrack: async (track: CareerTrack) => {
    const creating = track.created_at_ms <= 0;
    const saved = await request<CareerTrack>(
      creating
        ? "/api/jobs/tracks"
        : `/api/jobs/tracks/${encodeURIComponent(track.id)}`,
      {
        method: creating ? "POST" : "PUT",
        headers: await taxonomyWriteHeaders(),
        body: JSON.stringify(trackToWire(track)),
      },
    );
    return trackFromWire(saved);
  },
  deleteTrack: (id: string) =>
    request<void>(`/api/jobs/tracks/${encodeURIComponent(id)}`, {
      method: "DELETE",
    }),
  authorizeTrackAutoSubmit: (id: string) =>
    request<AutoSubmitAuthorization>(
      `/api/jobs/tracks/${encodeURIComponent(id)}/auto-submit`,
      { method: "POST" },
    ),
  revokeTrackAutoSubmit: (id: string) =>
    request<void>(`/api/jobs/tracks/${encodeURIComponent(id)}/auto-submit`, {
      method: "DELETE",
    }),
  saveMatch: (job: UserJobInput) =>
    request<JobPosting>("/api/jobs/matches", {
      method: "POST",
      body: JSON.stringify(job),
    }),
  verifySelectedCuratedLead: async (jobId: string) => {
    const posting = await request<JobPosting>(
      `/api/jobs/matches/${encodeURIComponent(jobId)}/verify-original`,
      { method: "POST", cache: "no-store" },
    );
    if (!posting || posting.id !== jobId || typeof posting.source !== "string"
      || !posting.source.endsWith("_import")) {
      throw new ApiError(0, "Bluey Jobs could not confirm this match. Refresh Matches before trying again.");
    }
    return posting;
  },
  loadSmartRecruitersRequirements: async (
    jobId: string,
    trackId: string,
    source: string,
    canonicalUrl: string,
  ) => {
    const posting = await request<JobPosting>(
      `/api/jobs/matches/${encodeURIComponent(jobId)}/load-requirements`,
      { method: "POST", cache: "no-store" },
    );
    if (!posting || posting.id !== jobId || posting.track_id !== trackId
      || posting.source !== source || posting.canonical_url !== canonicalUrl
      || !isNonActionableRequirementsLoadResult(posting)) {
      throw new ApiError(0, "Bluey Jobs could not confirm this match. Refresh Matches before trying again.");
    }
    return posting;
  },
  searchDiscoveryCatalog: (query: string, trackId: string, provider = "all") =>
    request<DiscoverySourceCatalogResponse>(
      `/api/jobs/discovery/catalog?q=${encodeURIComponent(query)}&track_id=${encodeURIComponent(trackId)}&provider=${encodeURIComponent(provider)}`,
    ),
  connectDiscoverySource: (trackId: string, catalogEntryId: string) =>
    request<DiscoverySource>("/api/jobs/discovery/sources", {
      method: "POST",
      body: JSON.stringify({
        track_id: trackId,
        catalog_entry_id: catalogEntryId,
      }),
    }),
  prepareApplication: (jobId: string, mode: string, submissionMode: string) =>
    request<PrepareApplicationResponse>("/api/jobs/applications", {
      method: "POST",
      body: JSON.stringify({
        job_id: jobId,
        mode,
        submission_mode: submissionMode,
      }),
    }),
  updateApplication: (id: string, state: string, submissionMode?: string) =>
    request<JobApplication>(
      `/api/jobs/applications/${encodeURIComponent(id)}`,
      {
        method: "PATCH",
        body: JSON.stringify({ state, submission_mode: submissionMode }),
      },
    ),
  commitPacket: (id: string) =>
    request<PacketCommitResult>(
      `/api/jobs/applications/${encodeURIComponent(id)}/commit`,
      {
        method: "POST",
      },
    ),
  approveApplication: (id: string) =>
    request<ApproveApplicationResponse>(
      `/api/jobs/applications/${encodeURIComponent(id)}/approve`,
      {
        method: "POST",
      },
    ),
  reconcileSubmissionNotSubmitted: (id: string) =>
    request<JobApplication>(
      `/api/jobs/applications/${encodeURIComponent(id)}/reconcile-submission`,
      {
        method: "POST",
        body: JSON.stringify({ outcome: "not_submitted", confirmed: true }),
      },
    ),
  queueApplicationRun: (id: string, runner: "local" | "cloud" = "cloud") =>
    request<QueueApplicationRunResponse>(
      `/api/jobs/applications/${encodeURIComponent(id)}/runs`,
      {
        method: "POST",
        body: JSON.stringify({ runner }),
      },
    ),
  applicationEvidence: (id: string) =>
    request<ApplicationEvidence[]>(
      `/api/jobs/applications/${encodeURIComponent(id)}/evidence`,
    ),
  downloadApplicationEvidence: (
    applicationId: string,
    evidenceId: string,
    fallbackName: string,
  ) =>
    requestDownload(
      `/api/jobs/applications/${encodeURIComponent(applicationId)}/evidence/${encodeURIComponent(evidenceId)}/download`,
      fallbackName,
    ),
  resumeVersion: (id: string) =>
    request<ResumeVersion>(
      `/api/jobs/resume-versions/${encodeURIComponent(id)}`,
    ),
  resolveIntervention: (
    id: string,
    status: string,
    action = "",
    resolution?: {
      answer?: string;
      remember?: boolean;
      scope?: string;
      scope_id?: string;
      expected_company_scope?: string;
    },
  ) =>
    request<InterventionResolutionResult>(
      `/api/jobs/interventions/${encodeURIComponent(id)}`,
      {
        method: "PATCH",
        body: JSON.stringify({ status, action, ...resolution }),
      },
    ),
  answerCompanyScope: (jobId: string) =>
    request<CompanyAnswerMemoryScope>(
      `/api/jobs/answer-company-scopes/${encodeURIComponent(jobId)}`,
      { cache: "no-store" },
    ),
  saveAnswerMemory: (answer: AnswerMemorySaveInput) =>
    request<AnswerMemory>(
      answer.id
        ? `/api/jobs/answers/${encodeURIComponent(answer.id)}`
        : "/api/jobs/answers",
      {
        method: answer.id ? "PUT" : "POST",
        body: JSON.stringify(answer),
      },
    ),
  deleteAnswerMemory: (id: string) =>
    request<void>(`/api/jobs/answers/${encodeURIComponent(id)}`, {
      method: "DELETE",
    }),
  saveCandidateEvent: (event: CandidateEventInput) =>
    request<CandidateEvent>("/api/jobs/candidate-events", {
      method: "POST",
      body: JSON.stringify(event),
    }),
  saveIntegration: (integration: JobsIntegration) =>
    request<JobsIntegration>("/api/jobs/integrations", {
      method: "PUT",
      body: JSON.stringify(integration),
    }),
  createApplicationIdentity: (identity: ApplicationIdentity) =>
    request<ApplicationIdentity>("/api/jobs/application-identities", {
      method: "POST",
      body: JSON.stringify(identity),
    }),
  updateApplicationIdentity: (identity: ApplicationIdentity) =>
    request<ApplicationIdentity>(
      `/api/jobs/application-identities/${encodeURIComponent(identity.id)}`,
      {
        method: "PUT",
        body: JSON.stringify(identity),
      },
    ),
  verifyApplicationIdentity: (id: string, code: string) =>
    request<ApplicationIdentity>(
      `/api/jobs/application-identities/${encodeURIComponent(id)}/verify`,
      {
        method: "POST",
        body: JSON.stringify({ code }),
      },
    ),
  resendApplicationIdentity: (id: string) =>
    request<ApplicationIdentity>(
      `/api/jobs/application-identities/${encodeURIComponent(id)}/resend`,
      {
        method: "POST",
      },
    ),
  deleteApplicationIdentity: (id: string) =>
    request<void>(
      `/api/jobs/application-identities/${encodeURIComponent(id)}`,
      { method: "DELETE" },
    ),
  mailboxOAuthProviders: async () => {
    const payload = await request<unknown>("/api/jobs/mailbox-oauth/config");
    return decodeMailboxProviderAvailability(payload);
  },
  startMailboxOAuth: async (provider: MailboxConnection["provider"]) => {
    const payload = await request<unknown>(
      `/api/jobs/mailbox-oauth/${encodeURIComponent(provider)}/start`,
      {
        method: "POST",
      },
    );
    return decodeMailboxOAuthStart(payload);
  },
  startMailboxCommunicationAuthorization: async (connectionId: string) => {
    const payload = await request<unknown>(
      `/api/jobs/mailbox-connections/${encodeURIComponent(connectionId)}/communication-authorization/start`,
      { method: "POST" },
    );
    return decodeMailboxOAuthStart(payload);
  },
  mailboxSyncState: (id: string) =>
    request<MailboxSyncState>(
      `/api/jobs/mailbox-connections/${encodeURIComponent(id)}/sync-state`,
    ),
  syncMailbox: (id: string) =>
    request<MailboxSyncState>(
      `/api/jobs/mailbox-connections/${encodeURIComponent(id)}/sync`,
      {
        method: "POST",
      },
    ),
  mailboxMessages: (connectionId?: string, status?: string, limit = 50) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (connectionId) params.set("connection_id", connectionId);
    if (status) params.set("status", status);
    return request<MailboxMessage[]>(
      `/api/jobs/mailbox-messages?${params.toString()}`,
    );
  },
  communicationActions: async (applicationId?: string, limit = 100) => {
    const params = new URLSearchParams({
      limit: String(Math.min(Math.max(limit, 1), 100)),
    });
    if (applicationId) params.set("application_id", applicationId);
    const payload = await request<unknown>(
      `/api/jobs/communication-actions?${params.toString()}`,
      { cache: "no-store" },
    );
    return decodeCommunicationActionSummaries(payload);
  },
  communicationAction: async (id: string) => {
    const payload = await request<unknown>(
      `/api/jobs/communication-actions/${encodeURIComponent(id)}`,
      { cache: "no-store" },
    );
    return decodeCommunicationActionDetail(payload);
  },
  approveCommunicationAction: async (
    action: Pick<
      CommunicationActionSummary,
      "id" | "action_revision" | "payload_sha256"
    >,
  ) => {
    const payload = await request<unknown>(
      `/api/jobs/communication-actions/${encodeURIComponent(action.id)}/approve`,
      {
        method: "POST",
        cache: "no-store",
        body: JSON.stringify({
          action_revision: action.action_revision,
          payload_sha256: action.payload_sha256,
        }),
      },
    );
    return decodeCommunicationActionDetail(payload);
  },
  cancelCommunicationAction: async (
    action: Pick<
      CommunicationActionSummary,
      "id" | "action_revision" | "payload_sha256"
    >,
  ) => {
    const payload = await request<unknown>(
      `/api/jobs/communication-actions/${encodeURIComponent(action.id)}/cancel`,
      {
        method: "POST",
        cache: "no-store",
        body: JSON.stringify({
          action_revision: action.action_revision,
          payload_sha256: action.payload_sha256,
        }),
      },
    );
    return decodeCommunicationActionDetail(payload);
  },
  deleteMailboxConnection: (id: string) =>
    request<void>(`/api/jobs/mailbox-connections/${encodeURIComponent(id)}`, {
      method: "DELETE",
    }),
};

export function isNonActionableRequirementsLoadResult(posting: JobPosting): boolean {
  const eligibility = posting.eligibility;
  return Boolean(
    eligibility
    && typeof posting.description === "string"
    && posting.description.trim() !== ""
    && Array.isArray(eligibility.review_reasons)
    && !eligibility.review_reasons
      .some(({ code }) => code === "posting_requirements_not_loaded")
    && eligibility.can_prepare === false
    && eligibility.can_queue_local === false
    && eligibility.can_queue_cloud === false
    && eligibility.can_auto_submit === false,
  );
}

export async function loadJobsPortal(): Promise<JobsPortalLoadResult> {
  const betaAccess = await jobsApi.betaAccess();
  if (betaAccess.access !== "admitted") {
    return { betaAccess, workspace: null, account: null };
  }
  const [workspace, account] = await Promise.all([
    jobsApi.workspace(),
    jobsApi.account(),
  ]);
  return { betaAccess, workspace, account };
}

export function signOutOfBluey(): void {
  const credentials = readPortalCredentials();
  const token = credentials?.access;
  const refresh = credentials?.refresh;
  activeSession = null;
  localStorage.removeItem(ACCESS_TOKEN_KEY);
  localStorage.removeItem(REFRESH_TOKEN_KEY);
  sessionStorage.removeItem(ACCESS_TOKEN_KEY);
  sessionStorage.removeItem(REFRESH_TOKEN_KEY);
  if (token) {
    void fetch("/auth/logout", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ refresh_token: refresh || null }),
    }).catch(() => undefined);
  }
  window.location.href = "/";
}
