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

export function localRunnerAccessCopy(enabled: boolean): RunnerAccessCopy {
  return enabled
    ? {
        badge: "Enabled",
        description:
          "Applications run on your computer in a separate Jobs browser. Take over at any moment.",
        points: [
          "Keeps your job-site sign-ins ready",
          "Uses the same reviewed application packet",
          "Stops when your computer is off",
        ],
        action: "Run locally",
      }
    : {
        badge: "Invited beta",
        description:
          "Local runner access is opening gradually to invited beta accounts.",
        points: [
          "Request access from Settings",
          "Review tailored packets in the portal now",
          "No browser run starts until access is enabled",
        ],
        action: "Request access",
      };
}

export function cloudRunnerAccessCopy(enabled: boolean): RunnerAccessCopy {
  return enabled
    ? {
        badge: "Enabled",
        description:
          "Bluey continues from an encrypted, isolated browser profile while your computer is off.",
        points: [
          "Offers one-click email-code approval",
          "Pauses for security checks and user handoff",
          "Stores an evidence-backed submission receipt",
        ],
        action: "Queue a run",
      }
    : {
        badge: "Invited beta",
        description:
          "Background runner access is opening gradually to invited beta accounts.",
        points: [
          "Request access from Settings",
          "Review tailored packets in the portal now",
          "No cloud run starts until access is enabled",
        ],
        action: "Request access",
      };
}
