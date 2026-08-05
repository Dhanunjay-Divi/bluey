import {
  FinalSubmitMarkerError,
  finalSubmitMarkerExists,
} from "./irreversible-submit.js";

export type LocalFailureCode =
  | "browser_execution_failed"
  | "configuration_invalid"
  | "identity_busy"
  | "identity_mismatch"
  | "launch_expired"
  | "launch_mismatch"
  | "manual_submission_observed"
  | "result_delivery_failed"
  | "run_not_active"
  | "run_request_invalid"
  | "submit_marker_state_unavailable"
  | "submit_outcome_unknown";

export interface LocalFailureClassification {
  status: "failed" | "side_effect_unknown";
  code: LocalFailureCode;
  message: string;
  preservePage: boolean;
}

export type LocalSideEffectReason = Extract<
  LocalFailureCode,
  "manual_submission_observed" | "submit_marker_state_unavailable" | "submit_outcome_unknown"
>;

const SAFE_MESSAGES: Record<LocalFailureCode, string> = {
  browser_execution_failed: "Bluey Browser could not finish this application safely.",
  configuration_invalid: "Bluey Browser is not configured for this application launch.",
  identity_busy: "Finish the other application using this application identity first.",
  identity_mismatch: "This application does not match the isolated browser identity.",
  launch_expired: "This application launch expired. Start it again from Bluey Jobs.",
  launch_mismatch: "This application launch did not match the requested run.",
  manual_submission_observed: [
    "Bluey observed employer submission confirmation outside its authorized submit path.",
    "Review the preserved browser; Bluey will not retry automatically.",
  ].join(" "),
  result_delivery_failed: "Bluey Browser could not safely save the local result.",
  run_not_active: "This local application is no longer active.",
  run_request_invalid: "Bluey Browser received an invalid application launch.",
  submit_marker_state_unavailable: "Bluey cannot safely determine whether this application was submitted. It will not submit again automatically.",
  submit_outcome_unknown: "Bluey cannot confirm whether the employer received this application. Review the preserved browser; Bluey will not submit again automatically.",
};

export class LocalBrowserError extends Error {
  readonly code: LocalFailureCode;

  constructor(code: LocalFailureCode) {
    super(SAFE_MESSAGES[code]);
    this.name = "LocalBrowserError";
    this.code = code;
  }
}

export function safeLocalFailure(error: unknown): Pick<LocalFailureClassification, "code" | "message"> {
  if (error instanceof LocalBrowserError) {
    return { code: error.code, message: SAFE_MESSAGES[error.code] };
  }
  return {
    code: "browser_execution_failed",
    message: SAFE_MESSAGES.browser_execution_failed,
  };
}

export function isLocalSideEffectReason(value: LocalFailureCode): value is LocalSideEffectReason {
  return value === "manual_submission_observed"
    || value === "submit_marker_state_unavailable"
    || value === "submit_outcome_unknown";
}

export async function classifyLocalFailure(
  runDirectory: string | undefined,
  error: unknown,
): Promise<LocalFailureClassification> {
  if (error instanceof LocalBrowserError && isLocalSideEffectReason(error.code)) {
    return {
      status: "side_effect_unknown",
      code: error.code,
      message: SAFE_MESSAGES[error.code],
      preservePage: true,
    };
  }
  if (runDirectory) {
    try {
      if (await finalSubmitMarkerExists(runDirectory)) {
        return {
          status: "side_effect_unknown",
          code: "submit_outcome_unknown",
          message: SAFE_MESSAGES.submit_outcome_unknown,
          preservePage: true,
        };
      }
    } catch {
      return {
        status: "side_effect_unknown",
        code: "submit_marker_state_unavailable",
        message: SAFE_MESSAGES.submit_marker_state_unavailable,
        preservePage: true,
      };
    }
  }

  const safe = error instanceof FinalSubmitMarkerError
    ? { code: "browser_execution_failed" as const, message: SAFE_MESSAGES.browser_execution_failed }
    : safeLocalFailure(error);
  return { status: "failed", ...safe, preservePage: false };
}
