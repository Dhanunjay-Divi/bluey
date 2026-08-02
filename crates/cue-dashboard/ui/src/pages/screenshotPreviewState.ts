export const MAX_SCREENSHOT_TITLE_CHARS = 160;

export type ScreenshotCaptureRequest = "selection" | "full_screen";
export type ScreenshotOperation = "capture" | "attach" | "discard";
export type ScreenshotErrorKind = "permission" | "cancelled" | "ambiguous" | "other";
export type ScreenshotPhase =
  | "idle"
  | "capturing"
  | "preview"
  | "attaching"
  | "discarding"
  | "attached";

export interface ScreenshotPreview {
  operation_id: string;
  path: string;
  file_size_bytes: number;
  created_at: string | number;
  capture_kind: string;
}

export interface ContextArtifactSummary {
  id?: string;
  kind?: string;
  path?: string;
  title?: string;
  size_bytes?: number | null;
  processing_status?: string;
  created_at?: string;
}

export interface ScreenshotContextAttachReceipt {
  operation_id: string;
  session_id: string;
  artifact: ContextArtifactSummary;
  already_attached: boolean;
  active: boolean;
}

export interface ScreenshotFlowError {
  operation: ScreenshotOperation;
  kind: ScreenshotErrorKind;
  title: string;
  message: string;
  detail?: string;
}

export interface ScreenshotFlowState {
  phase: ScreenshotPhase;
  preview: ScreenshotPreview | null;
  title: string;
  capture_request: ScreenshotCaptureRequest | null;
  attached_receipt: ScreenshotContextAttachReceipt | null;
  error: ScreenshotFlowError | null;
  notice: string | null;
}

export type ScreenshotFlowAction =
  | { type: "capture_started"; request: ScreenshotCaptureRequest }
  | { type: "capture_succeeded"; preview: ScreenshotPreview }
  | { type: "title_changed"; title: string }
  | { type: "attach_started" }
  | { type: "attach_succeeded"; receipt: ScreenshotContextAttachReceipt }
  | { type: "discard_started" }
  | { type: "discard_succeeded" }
  | { type: "operation_failed"; error: ScreenshotFlowError }
  | { type: "reset" };

export const INITIAL_SCREENSHOT_FLOW_STATE: ScreenshotFlowState = {
  phase: "idle",
  preview: null,
  title: "",
  capture_request: null,
  attached_receipt: null,
  error: null,
  notice: null,
};

export function screenshotFlowReducer(
  state: ScreenshotFlowState,
  action: ScreenshotFlowAction,
): ScreenshotFlowState {
  switch (action.type) {
    case "capture_started":
      if (state.preview || state.phase === "attaching" || state.phase === "discarding") return state;
      return {
        ...INITIAL_SCREENSHOT_FLOW_STATE,
        phase: "capturing",
        capture_request: action.request,
      };
    case "capture_succeeded":
      if (state.phase !== "capturing") return state;
      return {
        ...state,
        phase: "preview",
        preview: action.preview,
        title: defaultScreenshotTitle(action.preview),
        capture_request: null,
        error: null,
        notice: null,
      };
    case "title_changed":
      if (state.phase !== "preview") return state;
      return { ...state, title: action.title, error: null };
    case "attach_started":
      if (state.phase !== "preview" || !state.preview) return state;
      return { ...state, phase: "attaching", error: null, notice: null };
    case "attach_succeeded":
      if (state.phase !== "attaching" || !state.preview) return state;
      return {
        ...INITIAL_SCREENSHOT_FLOW_STATE,
        phase: "attached",
        attached_receipt: action.receipt,
        notice: screenshotReceiptNotice(action.receipt),
      };
    case "discard_started":
      if (state.phase !== "preview" || !state.preview) return state;
      return { ...state, phase: "discarding", error: null, notice: null };
    case "discard_succeeded":
      if (state.phase !== "discarding" || !state.preview) return state;
      return {
        ...INITIAL_SCREENSHOT_FLOW_STATE,
        notice: "Preview discarded. It was not attached to the session.",
      };
    case "operation_failed":
      if (action.error.operation === "capture") {
        if (state.phase !== "capturing") return state;
        return {
          ...INITIAL_SCREENSHOT_FLOW_STATE,
          error: action.error,
        };
      }
      if (action.error.operation === "attach" && state.phase === "attaching" && state.preview) {
        return { ...state, phase: "preview", error: action.error };
      }
      if (action.error.operation === "discard" && state.phase === "discarding" && state.preview) {
        return { ...state, phase: "preview", error: action.error };
      }
      return state;
    case "reset":
      if (state.preview || state.phase === "attaching" || state.phase === "discarding") return state;
      return INITIAL_SCREENSHOT_FLOW_STATE;
  }
}

export function screenshotReceiptNotice(receipt: ScreenshotContextAttachReceipt): string {
  if (!receipt.active) {
    return receipt.already_attached
      ? "Screenshot attachment confirmed in a saved, non-active session."
      : "Screenshot saved to a non-active session.";
  }
  return receipt.already_attached
    ? "Screenshot attachment confirmed in the current session."
    : "Screenshot attached to the current session.";
}

export function normalizeScreenshotTitle(value: string): string {
  return Array.from(value)
    .filter((character) => !/\p{Cc}/u.test(character) || /\s/u.test(character))
    .join("")
    .split(/\s+/u)
    .filter(Boolean)
    .join(" ");
}

export function validateScreenshotTitle(value: string): string | null {
  const normalized = normalizeScreenshotTitle(value);
  if (!normalized) return "Enter a title before attaching this screenshot.";
  if (unicodeCharCount(normalized) > MAX_SCREENSHOT_TITLE_CHARS) {
    return `Title must be ${MAX_SCREENSHOT_TITLE_CHARS} characters or fewer.`;
  }
  return null;
}

export function defaultScreenshotTitle(preview: ScreenshotPreview): string {
  return isFullScreenCapture(preview.capture_kind) ? "Full screen context" : "Selected screen context";
}

export function captureKindLabel(captureKind: string): string {
  if (isFullScreenCapture(captureKind)) return "Full screen";
  if (/window/i.test(captureKind) && !/region|selection/i.test(captureKind)) return "Window";
  return "Selected region or window";
}

export function classifyScreenshotError(
  operation: ScreenshotOperation,
  error: unknown,
): ScreenshotFlowError {
  const detail = errorMessage(error);
  const lower = detail.toLowerCase();
  const mentionsCancellation = /cancel(?:led|ed)?|user abort|selection closed/.test(lower);
  const mentionsPermission = /permission|not authorized|access denied|screen recording|privacy setting/.test(lower);

  if (
    operation === "capture"
    && ((mentionsCancellation && mentionsPermission) || /(?:failed|unavailable).*or permission/.test(lower))
  ) {
    return {
      operation,
      kind: "ambiguous",
      title: "Capture did not complete",
      message: "The system reported either a cancelled picker or unavailable screen-capture permission. Nothing was captured or attached. If you did not cancel, check system privacy settings before retrying.",
      detail,
    };
  }

  if (operation === "capture" && mentionsCancellation) {
    return {
      operation,
      kind: "cancelled",
      title: "Capture cancelled",
      message: "Nothing was captured, attached, or uploaded. You can try again when ready.",
    };
  }

  if (operation === "capture" && mentionsPermission) {
    return {
      operation,
      kind: "permission",
      title: "Screen capture permission is required",
      message: "Allow Bluey to capture the screen in system privacy settings, then retry. Your current preview, if any, remains unattached.",
      detail,
    };
  }

  if (
    operation === "attach" &&
    /could not confirm|cannot confirm|whether the screenshot was attached|unreadable screenshot receipt|preserved.*retry|safe retry/.test(
      lower,
    )
  ) {
    return {
      operation,
      kind: "ambiguous",
      title: "Attachment needs reconciliation",
      message:
        "Bluey did not receive a trustworthy receipt, so the retained copy may already be attached. Retry Attach to reconcile the same operation safely. Keep this preview: discarding it cannot undo a possible attachment and would remove your retry record.",
      detail,
    };
  }

  const operationLabel = operation === "capture" ? "capture" : operation === "attach" ? "attachment" : "discard";
  return {
    operation,
    kind: "other",
    title: `Screenshot ${operationLabel} failed`,
    message:
      operation === "attach"
        ? "The preview is still available locally. Retry attaching it or discard it."
        : operation === "discard"
          ? "The preview was not deleted. Retry discarding it or attach it to the current session."
          : "No screenshot was attached. Review the error and try again.",
    detail,
  };
}

export function formatScreenshotBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "Unknown size";
  if (bytes < 1_024) return `${Math.round(bytes)} B`;
  if (bytes < 1_048_576) return `${(bytes / 1_024).toFixed(bytes < 10_240 ? 1 : 0)} KB`;
  return `${(bytes / 1_048_576).toFixed(1)} MB`;
}

export function unicodeCharCount(value: string): number {
  return Array.from(value).length;
}

export function isWindowsUserAgent(userAgent: string): boolean {
  return /Windows/i.test(userAgent);
}

function isFullScreenCapture(captureKind: string): boolean {
  return /full[_ -]?screen/i.test(captureKind);
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error) || "Unknown screenshot error.";
  } catch {
    return "Unknown screenshot error.";
  }
}
