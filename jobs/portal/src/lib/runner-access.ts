import type { RunnerAvailability, RunnerChannelAvailability } from "../types";

export const lockedRunnerAvailability: RunnerAvailability = {
  local: {
    status: "invited_beta",
    available: false,
    plan_included: false,
    distribution_enabled: false,
    reason: "Bluey Browser availability could not be confirmed.",
    next_action: "Use Review first while Bluey refreshes your runner access.",
  },
  cloud: {
    status: "invited_beta",
    available: false,
    plan_included: false,
    distribution_enabled: false,
    reason: "Background runner availability could not be confirmed.",
    next_action: "Use Review first while Bluey refreshes your runner access.",
  },
  auto_submit_available: false,
  auto_submit_reason:
    "Auto-submit is unavailable until Bluey confirms runner access. Review first and job-site handoff remain available.",
};

export function runnerAvailabilityOrLocked(
  availability: RunnerAvailability | undefined,
): RunnerAvailability {
  return availability ?? lockedRunnerAvailability;
}

export const runnerLandingCopy = {
  flowTitle: "Review before a runner starts",
  flowBody:
    "Approve the exact application kit first. Local and cloud runners are opening gradually to invited beta accounts.",
  plansTitle: "Start free. Join runner beta when it opens.",
  proSummary: "Local browser access for invited beta accounts",
  proDetails: "3 Career Tracks · 50 applications · invited local runner beta",
  cloudSummary: "Background runner access for invited beta accounts",
  cloudDetails: "5 Career Tracks · 100 applications · invited cloud runner beta",
  accessNote:
    "Runner access appears only after it is enabled for your account. Every plan keeps the exact resume and answers used for each application.",
} as const;

export interface RunnerAccessCopy {
  badge: string;
  description: string;
  points: readonly string[];
  action: string;
}

export function localRunnerAccessCopy(access: RunnerChannelAvailability): RunnerAccessCopy {
  if (access.available) {
    return {
      badge: "Enabled",
      description:
        "Applications run on your computer in a separate Jobs browser. Take over at any moment.",
      points: [
        "Keeps your job-site sign-ins ready",
        "Uses the same reviewed application packet",
        "Stops when your computer is off",
      ],
      action: "Run locally",
    };
  }
  if (access.status === "upgrade_required") {
    return {
      badge: "Plan upgrade",
      description: access.reason,
      points: [
        access.next_action,
        "Review tailored packets in the portal now",
        "No browser run starts without runner access",
      ],
      action: "View plans",
    };
  }
  return {
    badge: "Invited beta",
    description: access.reason,
    points: [
      access.next_action,
      "Review tailored packets in the portal now",
      "No browser run starts until this release enables it",
    ],
    action: "Review applications",
  };
}

export function cloudRunnerAccessCopy(access: RunnerChannelAvailability): RunnerAccessCopy {
  if (access.available) {
    return {
      badge: "Enabled",
      description:
        "Bluey continues from an encrypted, isolated browser profile while your computer is off.",
      points: [
        "Offers one-click email-code approval",
        "Pauses for security checks and user handoff",
        "Stores an evidence-backed submission receipt",
      ],
      action: "Queue a run",
    };
  }
  if (access.status === "upgrade_required") {
    return {
      badge: "Plan upgrade",
      description: access.reason,
      points: [
        access.next_action,
        "Review tailored packets in the portal now",
        "No cloud run starts without runner access",
      ],
      action: "View plans",
    };
  }
  return {
    badge: "Invited beta",
    description: access.reason,
    points: [
      access.next_action,
      "Review tailored packets in the portal now",
      "No cloud run starts until this release enables it",
    ],
    action: "Review applications",
  };
}
