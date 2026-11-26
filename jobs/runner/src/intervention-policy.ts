import {
  parseManagedCloudReleaseMemo,
  type ManagedCloudReleaseMemoAuthority,
} from "@bluey/jobs-automation/managed-cloud-execution";

const CONTENT_NEUTRAL_RESUME_ACTIONS = new Set([
  "approve_submission",
  "approve_email_otp",
  "resume_browser_takeover",
  "browser_takeover_complete",
]);

export interface RunResolution {
  accountId: string;
  applicationId: string;
  applicationIdentityId: string;
  runId: string;
  requestId: string;
  profileScope: string;
  action?: string;
  field?: string;
  answer?: string;
  managedCloudRelease?: ManagedCloudReleaseMemoAuthority;
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

export function parseRunnerInterventionResolution(
  value: unknown,
  managedRuntimeConfigured = false,
): RunResolution {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new RunnerInterventionPolicyError("A run resolution is required");
  }
  const record = value as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    if (![
      "accountId",
      "applicationId",
      "applicationIdentityId",
      "runId",
      "requestId",
      "profileScope",
      "action",
      "field",
      "answer",
      ...(managedRuntimeConfigured ? ["managedCloudRelease"] : []),
    ].includes(key)) {
      throw new RunnerInterventionPolicyError("The run resolution contains unsupported data");
    }
  }
  let managedCloudRelease: ManagedCloudReleaseMemoAuthority | undefined;
  if (managedRuntimeConfigured) {
    try {
      managedCloudRelease = parseManagedCloudReleaseMemo(record.managedCloudRelease);
    } catch {
      throw new RunnerInterventionPolicyError(
        "The managed-cloud release authority is invalid",
      );
    }
  }
  return {
    accountId: requiredText(record.accountId, "account ID", 160),
    applicationId: requiredText(record.applicationId, "application ID", 160),
    applicationIdentityId: requiredText(
      record.applicationIdentityId,
      "application identity ID",
      160,
    ),
    runId: requiredText(record.runId, "run ID", 160),
    requestId: requiredText(record.requestId, "request ID", 240),
    profileScope: requiredText(record.profileScope, "profile scope", 40),
    ...optionalProperty(record, "action", 64),
    ...optionalProperty(record, "field", 1_000),
    ...optionalProperty(record, "answer", 10_000),
    ...(managedCloudRelease ? { managedCloudRelease } : {}),
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
