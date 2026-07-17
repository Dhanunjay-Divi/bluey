const CONTENT_NEUTRAL_RESUME_ACTIONS = new Set([
  "approve_submission",
  "approve_email_otp",
  "resume_browser_takeover",
  "browser_takeover_complete",
]);

export interface RunResolution {
  requestId: string;
  profileScope: string;
  action?: string;
  field?: string;
  answer?: string;
}

export type RunnerInterventionDecision =
  | { kind: "resume" }
  | { kind: "requires_reapproval"; reason: string };

export class RunnerInterventionPolicyError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "RunnerInterventionPolicyError";
  }
}

export function parseRunnerInterventionResolution(value: unknown): RunResolution {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new RunnerInterventionPolicyError("A run resolution is required");
  }
  const record = value as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    if (!["requestId", "profileScope", "action", "field", "answer"].includes(key)) {
      throw new RunnerInterventionPolicyError("The run resolution contains unsupported data");
    }
  }
  return {
    requestId: requiredText(record.requestId, "request ID", 240),
    profileScope: requiredText(record.profileScope, "profile scope", 40),
    ...optionalProperty(record, "action", 64),
    ...optionalProperty(record, "field", 1_000),
    ...optionalProperty(record, "answer", 10_000),
  };
}

/** Independent runner guard in case workflow or gateway policy is bypassed. */
export function decideRunnerInterventionResolution(
  resolution: RunResolution,
): RunnerInterventionDecision {
  if (hasText(resolution.field) || hasText(resolution.answer)) {
    return {
      kind: "requires_reapproval",
      reason: "A new application answer requires packet review and approval.",
    };
  }
  if (!resolution.action || !CONTENT_NEUTRAL_RESUME_ACTIONS.has(resolution.action)) {
    return {
      kind: "requires_reapproval",
      reason: "This intervention cannot resume the approved browser run.",
    };
  }
  return { kind: "resume" };
}

function hasText(value: string | undefined): boolean {
  return typeof value === "string" && value.trim().length > 0;
}

function requiredText(value: unknown, label: string, maximumLength: number): string {
  if (typeof value !== "string" || value.length === 0 || value.length > maximumLength) {
    throw new RunnerInterventionPolicyError(`The run ${label} is invalid`);
  }
  return value;
}

function optionalProperty(
  record: Record<string, unknown>,
  key: "action" | "field" | "answer",
  maximumLength: number,
): Partial<Pick<RunResolution, "action" | "field" | "answer">> {
  const value = record[key];
  if (value === undefined) return {};
  if (typeof value !== "string" || value.length > maximumLength) {
    throw new RunnerInterventionPolicyError(`The run ${key} is invalid`);
  }
  return { [key]: value };
}
