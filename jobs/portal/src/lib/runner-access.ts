import type { RunnerAvailability, RunnerChannelAvailability } from "../types";

export const lockedRunnerAvailability: RunnerAvailability = {
  local: {
    status: "invited_beta",
    available: false,
    plan_included: false,
    distribution_enabled: false,
    reason: "This automation channel is unavailable in the web experience.",
    next_action: "Review the application kit and continue on the original job site.",
  },
  cloud: {
    status: "invited_beta",
    available: false,
    plan_included: false,
    distribution_enabled: false,
    reason: "Cloud automation availability could not be confirmed.",
    next_action: "Use Review first while Bluey refreshes cloud automation access.",
  },
  auto_submit_available: false,
  auto_submit_reason:
    "Auto-submit is unavailable until Bluey confirms cloud automation access. Review first and job-site handoff remain available.",
};

export function runnerAvailabilityOrLocked(
  availability: RunnerAvailability | undefined,
): RunnerAvailability {
  return availability ?? lockedRunnerAvailability;
}

export const runnerLandingCopy = {
  flowTitle: "Review before cloud automation starts",
  flowBody:
    "Approve the exact application kit first. Cloud automation is opening gradually to invited beta accounts; Review first and job-site handoff stay available to everyone.",
  plansTitle: "Start free. Request cloud automation beta access.",
  proSummary: "Tailored applications in the web portal",
  proDetails: "3 Career Tracks · 50 application kits · web review and job-site handoff",
  cloudSummary: "Cloud automation for invited beta accounts",
  cloudDetails: "5 Career Tracks · 100 application kits · invited cloud automation beta",
  accessNote:
    "Cloud automation appears only after it is enabled for your account. Every plan keeps the exact resume and answers used for each application.",
} as const;

export interface RunnerAccessCopy {
  badge: string;
  description: string;
  points: readonly string[];
  action: string;
}

export function cloudRunnerAccessCopy(access: RunnerChannelAvailability): RunnerAccessCopy {
  if (access.available) {
    return {
      badge: "Enabled for this account",
      description:
        "For this enabled account, Bluey can continue in an encrypted, isolated cloud session.",
      points: [
        "Offers one-click email-code approval",
        "Pauses for security checks and secure takeover",
        "Stores an evidence-backed submission receipt",
      ],
      action: "Start cloud automation",
    };
  }
  if (access.status === "upgrade_required") {
    return {
      badge: "Plan upgrade",
      description: access.reason,
      points: [
        access.next_action,
        "Review tailored packets in the web portal now",
        "Continue safely on the original job site",
      ],
      action: "View plans",
    };
  }
  return {
    badge: "Invited beta",
    description: access.reason,
    points: [
      access.next_action,
      "Review tailored packets in the web portal now",
      "No cloud automation starts until access is enabled",
    ],
    action: "Review applications",
  };
}
