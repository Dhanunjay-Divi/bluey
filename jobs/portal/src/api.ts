import type {
  AccountSummary,
  ApproveApplicationResponse,
  ApplicationEvidence,
  ApplicationIdentity,
  BrowserSession,
  CareerFact,
  CareerProfile,
  CareerTrack,
  AnswerMemory,
  Intervention,
  InterventionResolutionResult,
  JobApplication,
  JobPosting,
  UserJobInput,
  JobPreferences,
  JobsIntegration,
  JobsWorkspace,
  MailboxConnection,
  PacketCommitResult,
  QueueApplicationRunResponse,
  PrepareApplicationResponse,
  ResumeVersion,
} from "./types";

const ACCESS_TOKEN_KEY = "bluey_access_token";
const REFRESH_TOKEN_KEY = "bluey_refresh_token";
const AUTH_PERSISTENCE_KEY = "bluey_auth_persistence";
let refreshAccessTokenPromise: Promise<string> | null = null;

export class ApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export interface InterviewPrepCompletionResponse {
  schema_version: number;
  id: string;
  application_id: string;
  content: string;
  generated_at_ms: number;
  provider: string;
  model: string;
  cost_cents: number;
  balance_cents_after: number;
  trial_seconds_remaining: number;
  grounding?: {
    receipt_id: string;
    receipt_fingerprint: string;
    resume_version_id: string;
    resume_checksum: string;
    resume_document_sha256: string;
    answer_keys_used: string[];
    answer_keys_omitted: string[];
  };
}

function storageOrder(): Storage[] {
  return localStorage.getItem(AUTH_PERSISTENCE_KEY) === "session"
    ? [sessionStorage, localStorage]
    : [localStorage, sessionStorage];
}

export function accessToken(): string {
  for (const store of storageOrder()) {
    const token = store.getItem(ACCESS_TOKEN_KEY);
    if (token) return token;
  }
  return "";
}

function refreshToken(): string {
  for (const store of storageOrder()) {
    const token = store.getItem(REFRESH_TOKEN_KEY);
    if (token) return token;
  }
  return "";
}

function persistTokens(payload: { access_token: string; refresh_token?: string }): void {
  const persistent = localStorage.getItem(AUTH_PERSISTENCE_KEY) !== "session";
  const store = persistent ? localStorage : sessionStorage;
  const other = persistent ? sessionStorage : localStorage;
  other.removeItem(ACCESS_TOKEN_KEY);
  other.removeItem(REFRESH_TOKEN_KEY);
  store.setItem(ACCESS_TOKEN_KEY, payload.access_token);
  if (payload.refresh_token) store.setItem(REFRESH_TOKEN_KEY, payload.refresh_token);
}

async function refreshAccessToken(): Promise<string> {
  if (refreshAccessTokenPromise) return refreshAccessTokenPromise;
  refreshAccessTokenPromise = (async () => {
    const token = refreshToken();
    if (!token) return "";
    const response = await fetch("/auth/refresh", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ refresh_token: token }),
    });
    if (!response.ok) return "";
    const payload = (await response.json()) as { access_token?: string; refresh_token?: string };
    if (!payload.access_token) return "";
    persistTokens({ access_token: payload.access_token, refresh_token: payload.refresh_token });
    return payload.access_token;
  })();
  try {
    return await refreshAccessTokenPromise;
  } finally {
    refreshAccessTokenPromise = null;
  }
}

async function request<T>(path: string, init: RequestInit = {}, retried = false): Promise<T> {
  const headers = new Headers(init.headers);
  if (!(init.body instanceof FormData)) headers.set("Content-Type", "application/json");
  const token = accessToken();
  if (token) headers.set("Authorization", `Bearer ${token}`);
  const response = await fetch(path, { ...init, headers });
  if (response.status === 401 && !retried) {
    const refreshed = await refreshAccessToken();
    if (refreshed) return request<T>(path, init, true);
  }
  let body: unknown = null;
  if (response.status !== 204) {
    const text = await response.text();
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
    throw new ApiError(response.status, message);
  }
  return body as T;
}

export const jobsApi = {
  workspace: () => request<JobsWorkspace>("/api/jobs/workspace"),
  account: () => request<AccountSummary>("/account/me"),
  completeOnboarding: (profile: CareerProfile, preferences: JobPreferences, track: CareerTrack) =>
    request<JobsWorkspace>("/api/jobs/onboarding/complete", {
      method: "POST",
      body: JSON.stringify({ profile, preferences, track }),
    }),
  prepareInterview: (applicationId: string) =>
    request<InterviewPrepCompletionResponse>(`/api/jobs/applications/${encodeURIComponent(applicationId)}/interview-prep`, {
      method: "POST",
      body: "{}",
    }),
  saveProfile: (profile: CareerProfile) =>
    request<CareerProfile>("/api/jobs/profile", { method: "PUT", body: JSON.stringify(profile) }),
  savePreferences: (preferences: JobPreferences) =>
    request<JobPreferences>("/api/jobs/preferences", {
      method: "PUT",
      body: JSON.stringify(preferences),
    }),
  saveFact: (fact: CareerFact) => request<CareerFact>("/api/jobs/facts", {
    method: "POST",
    body: JSON.stringify({
      id: fact.id,
      category: fact.category,
      label: fact.label,
      value: fact.value,
    }),
  }),
  deleteFact: (id: string) => request<void>(`/api/jobs/facts/${encodeURIComponent(id)}`, { method: "DELETE" }),
  saveTrack: (track: CareerTrack) =>
    request<CareerTrack>(track.id ? `/api/jobs/tracks/${encodeURIComponent(track.id)}` : "/api/jobs/tracks", {
      method: track.id ? "PUT" : "POST",
      body: JSON.stringify(track),
    }),
  deleteTrack: (id: string) =>
    request<void>(`/api/jobs/tracks/${encodeURIComponent(id)}`, { method: "DELETE" }),
  saveMatch: (job: UserJobInput) =>
    request<JobPosting>("/api/jobs/matches", { method: "POST", body: JSON.stringify(job) }),
  prepareApplication: (jobId: string, mode: string, submissionMode: string) =>
    request<PrepareApplicationResponse>("/api/jobs/applications", {
      method: "POST",
      body: JSON.stringify({ job_id: jobId, mode, submission_mode: submissionMode }),
    }),
  updateApplication: (id: string, state: string, submissionMode?: string) =>
    request<JobApplication>(`/api/jobs/applications/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify({ state, submission_mode: submissionMode }),
    }),
  commitPacket: (id: string) =>
    request<PacketCommitResult>(`/api/jobs/applications/${encodeURIComponent(id)}/commit`, {
      method: "POST",
    }),
  approveApplication: (id: string) =>
    request<ApproveApplicationResponse>(`/api/jobs/applications/${encodeURIComponent(id)}/approve`, {
      method: "POST",
    }),
  queueApplicationRun: (id: string, runner: "local" | "cloud" = "cloud") =>
    request<QueueApplicationRunResponse>(`/api/jobs/applications/${encodeURIComponent(id)}/runs`, {
      method: "POST",
      body: JSON.stringify({ runner }),
    }),
  applicationEvidence: (id: string) =>
    request<ApplicationEvidence[]>(`/api/jobs/applications/${encodeURIComponent(id)}/evidence`),
  saveApplicationEvidence: (id: string, evidence: ApplicationEvidence) =>
    request<ApplicationEvidence>(`/api/jobs/applications/${encodeURIComponent(id)}/evidence`, {
      method: "POST",
      body: JSON.stringify(evidence),
    }),
  resumeVersion: (id: string) =>
    request<ResumeVersion>(`/api/jobs/resume-versions/${encodeURIComponent(id)}`),
  saveBrowserSession: (session: BrowserSession) =>
    request<BrowserSession>("/api/jobs/browser-sessions", {
      method: "POST",
      body: JSON.stringify(session),
    }),
  resolveIntervention: (
    id: string,
    status: string,
    action = "",
    resolution?: { answer?: string; remember?: boolean; scope?: string; scope_id?: string },
  ) =>
    request<InterventionResolutionResult>(`/api/jobs/interventions/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify({ status, action, ...resolution }),
    }),
  saveAnswerMemory: (answer: AnswerMemory) =>
    request<AnswerMemory>(answer.id ? `/api/jobs/answers/${encodeURIComponent(answer.id)}` : "/api/jobs/answers", {
      method: answer.id ? "PUT" : "POST",
      body: JSON.stringify(answer),
    }),
  deleteAnswerMemory: (id: string) =>
    request<void>(`/api/jobs/answers/${encodeURIComponent(id)}`, { method: "DELETE" }),
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
    request<ApplicationIdentity>(`/api/jobs/application-identities/${encodeURIComponent(identity.id)}`, {
      method: "PUT",
      body: JSON.stringify(identity),
    }),
  verifyApplicationIdentity: (id: string, code: string) =>
    request<ApplicationIdentity>(`/api/jobs/application-identities/${encodeURIComponent(id)}/verify`, {
      method: "POST",
      body: JSON.stringify({ code }),
    }),
  resendApplicationIdentity: (id: string) =>
    request<ApplicationIdentity>(`/api/jobs/application-identities/${encodeURIComponent(id)}/resend`, {
      method: "POST",
    }),
  deleteApplicationIdentity: (id: string) =>
    request<void>(`/api/jobs/application-identities/${encodeURIComponent(id)}`, { method: "DELETE" }),
  requestMailboxConnection: (connection: MailboxConnection) =>
    request<MailboxConnection>("/api/jobs/mailbox-connections", {
      method: "POST",
      body: JSON.stringify(connection),
    }),
  deleteMailboxConnection: (id: string) =>
    request<void>(`/api/jobs/mailbox-connections/${encodeURIComponent(id)}`, { method: "DELETE" }),
};

export function loginUrl(): string {
  return `/login?next=${encodeURIComponent("/jobs")}`;
}
