import type {
  ControllerActivityItem,
  ControllerPrimaryAction,
  ControllerStatus,
  ControllerViewState,
} from "./controller-contract.js";

const MAX_TITLE = 88;
const MAX_DETAIL = 240;
const MAX_CONTEXT = 100;
const MAX_ACTIVITY = 6;

const BASE_ACTIVITY: ControllerActivityItem[] = [
  { label: "Prepare documents", state: "pending" },
  { label: "Fill application", state: "pending" },
  { label: "Review safely", state: "pending" },
  { label: "Save result", state: "pending" },
];

export interface SafeJobContext {
  company?: string;
  role?: string;
  identityAvailable?: boolean;
}

export function readyControllerState(input: {
  backgroundEnabled?: boolean;
  online?: boolean;
  paused?: boolean;
} = {}): ControllerViewState {
  if (input.paused || input.online === false) {
    return pausedControllerState({
      backgroundEnabled: Boolean(input.backgroundEnabled),
      online: input.online !== false,
      reason: input.online === false ? "offline" : "manual",
    });
  }
  return sanitizeControllerState({
    version: 1,
    status: "ready",
    mode: "local",
    modeLabel: "Local",
    title: "Ready when you are",
    detail: "Start a reviewed application from Bluey Jobs and it will open in its separate browser profile.",
    footnote: "Runs quietly while this computer is awake.",
    currentRunCount: 0,
    progressStep: 0,
    primaryAction: "open_jobs",
    primaryLabel: "Open Bluey Jobs",
    canOpenBrowser: false,
    canPause: true,
    paused: false,
    canStop: false,
    backgroundEnabled: Boolean(input.backgroundEnabled),
    loginItemSupported: false,
    online: input.online ?? true,
    activity: BASE_ACTIVITY,
  });
}

export function runningControllerState(input: {
  context?: SafeJobContext;
  currentRunCount: number;
  progressStep?: 0 | 1 | 2;
  backgroundEnabled: boolean;
  online: boolean;
  paused?: boolean;
  detail?: string;
}): ControllerViewState {
  const step = input.progressStep ?? 0;
  return sanitizeControllerState({
    version: 1,
    status: "running",
    mode: "local",
    modeLabel: "Local",
    title: "Application in progress",
    detail: input.detail || "Bluey is working through the reviewed application in an isolated browser profile.",
    footnote: input.paused
      ? "New applications are paused. This protected step will finish safely."
      : "Runs quietly while this computer is awake.",
    ...safeContext(input.context),
    currentRunCount: input.currentRunCount,
    progressStep: step,
    primaryAction: "open_browser",
    primaryLabel: "Open application browser",
    canOpenBrowser: true,
    canPause: true,
    paused: Boolean(input.paused),
    canStop: true,
    backgroundEnabled: input.backgroundEnabled,
    loginItemSupported: false,
    online: input.online,
    activity: activityFor(step, "running"),
  });
}

export function needsYouControllerState(input: {
  context?: SafeJobContext;
  currentRunCount: number;
  interventionKind?: string;
  backgroundEnabled: boolean;
  online: boolean;
  recovered?: boolean;
}): ControllerViewState {
  const copy = interventionCopy(input.interventionKind, Boolean(input.recovered));
  return sanitizeControllerState({
    version: 1,
    status: "needs_you",
    mode: "local",
    modeLabel: "Local",
    title: copy.title,
    detail: copy.detail,
    footnote: "Bluey will wait. It will not guess or submit this step for you.",
    ...safeContext(input.context),
    currentRunCount: input.currentRunCount,
    progressStep: 2,
    primaryAction: "continue",
    primaryLabel: "Continue application",
    canOpenBrowser: true,
    canPause: true,
    paused: false,
    canStop: true,
    backgroundEnabled: input.backgroundEnabled,
    loginItemSupported: false,
    online: input.online,
    activity: activityFor(2, "needs_you"),
  });
}

export function pausedControllerState(input: {
  backgroundEnabled: boolean;
  online: boolean;
  reason: "manual" | "offline" | "unavailable" | "stop_requested" | "device_unavailable";
  currentRunCount?: number;
  context?: SafeJobContext;
}): ControllerViewState {
  const copy = input.reason === "offline"
    ? {
        title: "Waiting for a connection",
        detail: "Bluey will not start a new application while this computer is offline.",
        label: "Check again",
      }
    : input.reason === "device_unavailable"
      ? {
          title: "Waiting for this computer",
          detail: "Bluey will not start a new application while the screen is locked or the system is suspended.",
          label: "Check again",
        }
    : input.reason === "unavailable"
      ? {
          title: "Applications are unavailable",
          detail: "Open Bluey Jobs to check sign-in, application identity, and plan access.",
          label: "Open Bluey Jobs",
        }
      : input.reason === "stop_requested"
        ? {
            title: "Stop requested",
            detail: "Bluey will not start another application. A protected current step is not interrupted.",
            label: "Resume applications",
          }
        : {
            title: "Applications paused",
            detail: "Bluey will not claim or start another local application until you resume.",
            label: "Resume applications",
          };
  const hasRun = (input.currentRunCount ?? 0) > 0;
  return sanitizeControllerState({
    version: 1,
    status: "paused",
    mode: "local",
    modeLabel: "Local",
    title: copy.title,
    detail: copy.detail,
    footnote: "Runs quietly while this computer is awake.",
    ...safeContext(input.context),
    currentRunCount: input.currentRunCount ?? 0,
    progressStep: hasRun ? 2 : 0,
    primaryAction: input.reason === "unavailable" ? "open_jobs" : "resume",
    primaryLabel: copy.label,
    canOpenBrowser: hasRun,
    canPause: true,
    paused: true,
    canStop: hasRun && input.reason !== "stop_requested",
    backgroundEnabled: input.backgroundEnabled,
    loginItemSupported: false,
    online: input.online,
    activity: hasRun ? activityFor(2, "paused") : BASE_ACTIVITY,
  });
}

export function completedControllerState(input: {
  submitted: boolean;
  backgroundEnabled: boolean;
  online: boolean;
  paused?: boolean;
  context?: SafeJobContext;
}): ControllerViewState {
  return sanitizeControllerState({
    version: 1,
    status: "completed",
    mode: "local",
    modeLabel: "Local",
    title: input.submitted ? "Application submitted" : "Application stopped",
    detail: input.submitted
      ? "Bluey saved the reviewed result and evidence in your application history."
      : "Bluey stopped before recording a submission. Open Jobs to review the result.",
    footnote: "No resume, answers, or account details are shown in this controller.",
    ...safeContext(input.context),
    currentRunCount: 0,
    progressStep: 3,
    primaryAction: "open_jobs",
    primaryLabel: "View application history",
    canOpenBrowser: false,
    canPause: true,
    paused: Boolean(input.paused),
    canStop: false,
    backgroundEnabled: input.backgroundEnabled,
    loginItemSupported: false,
    online: input.online,
    activity: activityFor(3, "completed"),
  });
}

export function failedUnknownControllerState(input: {
  unknown: boolean;
  active: boolean;
  backgroundEnabled: boolean;
  online: boolean;
  context?: SafeJobContext;
}): ControllerViewState {
  return sanitizeControllerState({
    version: 1,
    status: "failed_unknown",
    mode: "local",
    modeLabel: "Local",
    title: input.unknown ? "Confirm the application result" : "Application needs review",
    detail: input.unknown
      ? "Bluey cannot safely determine whether the final action completed. It will not try again automatically."
      : "Bluey stopped safely. Open Jobs to review the recorded issue before trying again.",
    footnote: input.unknown
      ? "This application is held for reconciliation."
      : "The durable application history remains the source of truth.",
    ...safeContext(input.context),
    currentRunCount: input.active ? 1 : 0,
    progressStep: 2,
    primaryAction: input.active ? "continue" : "open_jobs",
    primaryLabel: input.active ? "Review in browser" : "Open Bluey Jobs",
    canOpenBrowser: input.active,
    canPause: true,
    paused: false,
    canStop: input.active,
    backgroundEnabled: input.backgroundEnabled,
    loginItemSupported: false,
    online: input.online,
    activity: activityFor(2, "failed_unknown"),
  });
}

export function withShellFlags(
  state: ControllerViewState,
  flags: { backgroundEnabled: boolean; online: boolean; paused: boolean },
): ControllerViewState {
  return sanitizeControllerState({
    ...state,
    backgroundEnabled: flags.backgroundEnabled,
    online: flags.online,
    paused: flags.paused,
  });
}

export function sanitizeControllerState(input: ControllerViewState): ControllerViewState {
  const status = statusValue(input.status);
  const primaryAction = primaryActionValue(input.primaryAction);
  const step = clampInteger(input.progressStep, 0, 3) as 0 | 1 | 2 | 3;
  return {
    version: 1,
    status,
    mode: "local",
    modeLabel: "Local",
    title: safeDisplayText(input.title, MAX_TITLE) || "Bluey Browser",
    detail: safeDisplayText(input.detail, MAX_DETAIL),
    footnote: safeDisplayText(input.footnote, MAX_DETAIL),
    ...(safeDisplayText(input.company, MAX_CONTEXT) ? {
      company: safeDisplayText(input.company, MAX_CONTEXT),
    } : {}),
    ...(safeDisplayText(input.role, MAX_CONTEXT) ? {
      role: safeDisplayText(input.role, MAX_CONTEXT),
    } : {}),
    ...(safeDisplayText(input.identityLabel, MAX_CONTEXT) ? {
      identityLabel: safeDisplayText(input.identityLabel, MAX_CONTEXT),
    } : {}),
    currentRunCount: clampInteger(input.currentRunCount, 0, 99),
    progressStep: step,
    primaryAction,
    ...(primaryAction !== "none" && safeDisplayText(input.primaryLabel, MAX_TITLE) ? {
      primaryLabel: safeDisplayText(input.primaryLabel, MAX_TITLE),
    } : {}),
    canOpenBrowser: Boolean(input.canOpenBrowser),
    canPause: Boolean(input.canPause),
    paused: Boolean(input.paused),
    canStop: Boolean(input.canStop),
    backgroundEnabled: Boolean(input.backgroundEnabled),
    loginItemSupported: Boolean(input.loginItemSupported),
    online: Boolean(input.online),
    activity: input.activity.slice(0, MAX_ACTIVITY).map((item) => ({
      label: safeDisplayText(item.label, MAX_CONTEXT),
      state: ["pending", "active", "done", "attention"].includes(item.state)
        ? item.state
        : "pending",
    })),
  };
}

export function safeDisplayText(value: unknown, maxLength: number): string {
  if (typeof value !== "string") return "";
  return value
    .replace(/[\u0000-\u001f\u007f-\u009f]/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, maxLength);
}

function safeContext(context?: SafeJobContext): Pick<ControllerViewState, "company" | "role" | "identityLabel"> {
  if (!context) return {};
  return {
    ...(context.company ? { company: context.company } : {}),
    ...(context.role ? { role: context.role } : {}),
    ...(context.identityAvailable ? { identityLabel: "Verified application identity" } : {}),
  };
}

function interventionCopy(kind: string | undefined, recovered: boolean): { title: string; detail: string } {
  if (recovered) {
    return {
      title: "Application recovered",
      detail: "Bluey restored this safe pre-submit application and is waiting for you to continue.",
    };
  }
  switch (kind) {
    case "captcha":
      return { title: "CAPTCHA needs you", detail: "Complete the visible verification in the application browser." };
    case "two_factor":
      return { title: "Two-factor check", detail: "Complete the sign-in verification in the application browser." };
    case "assessment":
      return { title: "Assessment needs you", detail: "Complete the employer assessment yourself, then return here." };
    case "missing_fact":
    case "unknown_question":
    case "sensitive_question":
      return { title: "A question needs you", detail: "Review the highlighted question in the application browser." };
    case "side_effect_unknown":
      return {
        title: "Confirm the application result",
        detail: "Bluey will not retry because the final application state is uncertain.",
      };
    default:
      return { title: "Application needs you", detail: "Complete the visible step in the application browser." };
  }
}

function activityFor(
  step: 0 | 1 | 2 | 3,
  status: ControllerStatus,
): ControllerActivityItem[] {
  return BASE_ACTIVITY.map((item, index) => ({
    label: item.label,
    state: index < step
      ? "done"
      : index === step
        ? status === "needs_you" || status === "failed_unknown"
          ? "attention"
          : status === "paused"
            ? "pending"
            : status === "completed"
              ? "done"
              : "active"
        : "pending",
  }));
}

function statusValue(value: unknown): ControllerStatus {
  return ["ready", "running", "needs_you", "paused", "completed", "failed_unknown"]
    .includes(String(value))
    ? value as ControllerStatus
    : "failed_unknown";
}

function primaryActionValue(value: unknown): ControllerPrimaryAction {
  return ["none", "open_jobs", "open_browser", "continue", "resume"].includes(String(value))
    ? value as ControllerPrimaryAction
    : "none";
}

function clampInteger(value: unknown, minimum: number, maximum: number): number {
  const numeric = typeof value === "number" && Number.isFinite(value) ? Math.trunc(value) : minimum;
  return Math.min(maximum, Math.max(minimum, numeric));
}
