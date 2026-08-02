export interface JobsHandoffImportPayload {
  result_id: string;
  completed_at_ms: number;
  success: boolean;
  application_id: string | null;
  role: string | null;
  company: string | null;
  recovered: boolean;
  error: string | null;
}

export interface JobsHandoffNotice {
  id: number;
  payload: JobsHandoffImportPayload;
}

export interface JobsHandoffState {
  notice: JobsHandoffNotice | null;
  successful_import_revision: number;
  last_result_id: string | null;
}

export type JobsHandoffAction =
  | { type: "received"; id: number; payload: JobsHandoffImportPayload }
  | { type: "dismissed" };

export const INITIAL_JOBS_HANDOFF_STATE: JobsHandoffState = {
  notice: null,
  successful_import_revision: 0,
  last_result_id: null,
};

export function jobsHandoffReducer(
  state: JobsHandoffState,
  action: JobsHandoffAction,
): JobsHandoffState {
  switch (action.type) {
    case "received":
      if (action.payload.result_id === state.last_result_id) return state;
      return {
        notice: { id: action.id, payload: action.payload },
        successful_import_revision: action.payload.success
          ? state.successful_import_revision + 1
          : state.successful_import_revision,
        last_result_id: action.payload.result_id,
      };
    case "dismissed":
      if (!state.notice) return state;
      return { ...state, notice: null };
  }
}

export function normalizeJobsHandoffPayload(value: unknown): JobsHandoffImportPayload {
  if (!value || typeof value !== "object") return invalidPayload();
  const candidate = value as Record<string, unknown>;
  if (
    typeof candidate.success !== "boolean"
    || typeof candidate.result_id !== "string"
    || !/^[a-fA-F0-9]{32}$/u.test(candidate.result_id)
    || typeof candidate.completed_at_ms !== "number"
    || !Number.isSafeInteger(candidate.completed_at_ms)
  ) return invalidPayload();

  return {
    result_id: candidate.result_id,
    completed_at_ms: candidate.completed_at_ms,
    success: candidate.success,
    application_id: optionalString(candidate.application_id),
    role: optionalString(candidate.role),
    company: optionalString(candidate.company),
    recovered: candidate.recovered === true,
    error: typeof candidate.error === "string" ? candidate.error : null,
  };
}

export function jobsHandoffTargetLabel(payload: JobsHandoffImportPayload): string | null {
  const role = payload.role?.trim();
  const company = payload.company?.trim();
  if (role && company) return `${role} at ${company}`;
  return role || company || null;
}

export function safeJobsHandoffRetryError(error: unknown): string {
  const raw = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  const normalized = Array.from(raw)
    .filter((character) => !/\p{Cc}/u.test(character) || /\s/u.test(character))
    .join("")
    .split(/\s+/u)
    .filter(Boolean)
    .join(" ");
  if (!normalized) return "Bluey could not retry the saved handoff. Try again in a moment.";
  if (Array.from(normalized).length <= 500) return normalized;
  return `${Array.from(normalized).slice(0, 499).join("")}…`;
}

function optionalString(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const normalized = value.trim();
  return normalized || null;
}

function invalidPayload(): JobsHandoffImportPayload {
  return {
    result_id: "0".repeat(32),
    completed_at_ms: 0,
    success: false,
    application_id: null,
    role: null,
    company: null,
    recovered: false,
    error: "Bluey received an unreadable job handoff result.",
  };
}
