import type { InterventionResolution } from "./contracts.js";

const CONTENT_NEUTRAL_RESUME_ACTIONS = new Set([
  "approve_submission",
  "approve_email_otp",
  "resume_browser_takeover",
  "browser_takeover_complete",
]);

export class InterventionPolicyError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "InterventionPolicyError";
  }
}

export type InterventionResolutionDecision =
  | { kind: "resume"; resolution: InterventionResolution }
  | { kind: "requires_reapproval"; resolution: InterventionResolution; reason: string };

export function parseInterventionResolution(value: unknown): InterventionResolution {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new InterventionPolicyError("An intervention resolution is required");
  }
  const record = value as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    if (!["action", "field", "answer"].includes(key)) {
      throw new InterventionPolicyError("The intervention resolution contains unsupported data");
    }
  }
  if (typeof record.action !== "string") {
    throw new InterventionPolicyError("An intervention action is required");
  }
  const action = record.action.trim();
  if (!/^[a-z][a-z0-9_]{1,63}$/.test(action)) {
    throw new InterventionPolicyError("The intervention action is invalid");
  }
  const field = optionalText(record.field, "field", 1_000);
  const answer = optionalText(record.answer, "answer", 10_000);
  return {
    action,
    ...(field !== undefined ? { field } : {}),
    ...(answer !== undefined ? { answer } : {}),
  };
}

export function decideInterventionResolution(
  value: InterventionResolution,
): InterventionResolutionDecision {
  const resolution = parseInterventionResolution(value);
  if (hasText(resolution.field) || hasText(resolution.answer)) {
    return {
      kind: "requires_reapproval",
      resolution,
      reason: "A new application answer changes the approved packet and must be reviewed again.",
    };
  }
  if (!CONTENT_NEUTRAL_RESUME_ACTIONS.has(resolution.action)) {
    return {
      kind: "requires_reapproval",
      resolution,
      reason: "This intervention is not a content-neutral resume action.",
    };
  }
  return { kind: "resume", resolution };
}

function optionalText(value: unknown, label: string, maximumLength: number): string | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "string" || value.length > maximumLength) {
    throw new InterventionPolicyError(`The intervention ${label} is invalid`);
  }
  return value;
}

function hasText(value: string | undefined): boolean {
  return typeof value === "string" && value.trim().length > 0;
}
