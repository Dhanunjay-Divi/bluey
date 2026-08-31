import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { previewWorkspace } from "../data/preview";
import type {
  Intervention,
  JobApplication,
  JobsWorkspace,
  RunnerChannelAvailability,
} from "../types";
import {
  AutomationView,
  automationAction,
  queueCloudApplication,
  submissionApprovalEnabled,
  submissionApprovalTargetMatches,
} from "./AutomationView";

const availableCloud: RunnerChannelAvailability = {
  status: "available",
  available: true,
  plan_included: true,
  distribution_enabled: true,
  reason: "Background automation is ready.",
  next_action: "Queue an approved application.",
};

const unavailableCloud: RunnerChannelAvailability = {
  status: "limited_beta",
  available: false,
  plan_included: true,
  distribution_enabled: false,
  reason: "Background automation is not enabled for this account.",
  next_action: "Use Review first while access is unavailable.",
};

function workspaceWith(
  cloud: RunnerChannelAvailability,
  overrides: Partial<JobsWorkspace> = {},
): JobsWorkspace {
  return {
    ...previewWorkspace,
    ...overrides,
    runner_availability: {
      ...previewWorkspace.runner_availability,
      cloud,
    },
  };
}

function renderAutomation(workspace: JobsWorkspace, previewSearch = ""): string {
  return renderToStaticMarkup(
    <AutomationView
      workspace={workspace}
      previewSearch={previewSearch}
      onQueueCloud={vi.fn(async () => undefined)}
      onResolveIntervention={vi.fn(async () => undefined)}
    />,
  );
}

describe("cloud-first application automation", () => {
  it("renders cloud queue access as the only automation path", () => {
    const html = renderAutomation(workspaceWith(availableCloud, {
      browser_sessions: [],
      interventions: [],
    }));

    expect(html).toContain("APPLICATION AUTOMATION");
    expect(html).toContain(">Automation<");
    expect(html).toContain("Background automation");
    expect(html).toContain("Start cloud automation");
    expect(html).toContain("Encrypted cloud execution");
    expect(html).not.toMatch(/install|download|run locally|local runner|Bluey Browser/i);
  });

  it("shows a truthful Review fallback and no queue control when cloud access is unavailable", () => {
    const html = renderAutomation(workspaceWith(unavailableCloud, {
      browser_sessions: [],
      interventions: [],
    }), "?preview=1&scenario=runner-beta");

    expect(html).toContain("Background automation is not enabled for this account.");
    expect(html).toContain(
      'href="/jobs/applications?preview=1&amp;scenario=runner-beta"',
    );
    expect(html).toContain("Review applications");
    expect(html).not.toContain("Start cloud automation");
  });

  it("derives queue versus Review behavior only from server-authored cloud availability", () => {
    expect(automationAction(availableCloud)).toEqual({ kind: "queue" });
    expect(automationAction(unavailableCloud, "?preview=1&scenario=runner-beta")).toEqual({
      kind: "review",
      href: "/jobs/applications?preview=1&scenario=runner-beta",
    });
  });

  it("runs only the supplied cloud callback and propagates queue failures", async () => {
    const queued = {
      ...previewWorkspace.applications[0],
      state: "queued",
    } satisfies JobApplication;
    const callback = vi.fn(async () => undefined);

    await queueCloudApplication(availableCloud, queued, callback);
    expect(callback).toHaveBeenCalledOnce();
    expect(callback).toHaveBeenCalledWith(queued);

    const unavailableCallback = vi.fn(async () => undefined);
    await expect(queueCloudApplication(unavailableCloud, queued, unavailableCallback))
      .rejects.toThrow("Background automation is not enabled for this account.");
    expect(unavailableCallback).not.toHaveBeenCalled();

    const failedCallback = vi.fn(async () => {
      throw new Error("Queue request failed safely.");
    });
    await expect(queueCloudApplication(availableCloud, queued, failedCallback))
      .rejects.toThrow("Queue request failed safely.");
  });

  it("preserves active-run takeover and final-review approval controls", () => {
    const title = "Review the Greenhouse application";
    const detail =
      "Review every employer-facing field and document in the preserved form, then approve submission.";
    const html = renderAutomation(workspaceWith(availableCloud, {
      browser_sessions: [{
        id: "cloud-run-1",
        runner: "cloud",
        status: "needs_input",
        current_company: "Meridian Financial",
        current_step: "Final application review",
        application_id: "app-2",
        takeover_url: "https://jobs-browser.bluey.sh/sessions/cloud-run-1",
        created_at_ms: Date.now() - 60_000,
        updated_at_ms: Date.now(),
      }],
      interventions: [{
        id: "intervention-final-review",
        application_id: "app-2",
        kind: "browser_takeover",
        status: "open",
        title,
        detail,
        choices: [],
        resolution_kind: "browser_takeover",
        resume_after_resolution: true,
        provider: "",
        provider_message_id: "",
        metadata: {
          receipt: {
            status: "needs_input",
            issues: [],
            intervention: {
              kind: "browser_takeover",
              title,
              detail,
              takeoverUrl: "https://jobs-browser.bluey.sh/sessions/cloud-run-1",
              resolution: { kind: "browser_takeover", resumeAfter: true },
            },
          },
        },
        created_at_ms: Date.now(),
      }],
    }));

    expect(html).toContain("ACTIVE CLOUD AUTOMATION RUN");
    expect(html).toContain("Review form");
    expect(html).toContain("Approve submission");
    expect(html).not.toMatch(/pause run|resume run/i);
  });

  it("withholds final approval unless the intervention matches the active takeover capability", () => {
    const title = "Review the Greenhouse application";
    const detail =
      "Review every employer-facing field and document in the preserved form, then approve submission.";
    const html = renderAutomation(workspaceWith(availableCloud, {
      browser_sessions: [{
        id: "cloud-run-1",
        runner: "cloud",
        status: "needs_input",
        current_company: "Meridian Financial",
        current_step: "Final application review",
        application_id: "app-2",
        takeover_url: "https://jobs-browser.bluey.sh/sessions/cloud-run-1",
        created_at_ms: Date.now() - 60_000,
        updated_at_ms: Date.now(),
      }],
      interventions: [{
        id: "intervention-final-review",
        application_id: "app-2",
        kind: "browser_takeover",
        status: "open",
        title,
        detail,
        choices: [],
        resolution_kind: "browser_takeover",
        resume_after_resolution: true,
        provider: "",
        provider_message_id: "",
        metadata: {
          receipt: {
            status: "needs_input",
            issues: [],
            intervention: {
              kind: "browser_takeover",
              title,
              detail,
              takeoverUrl: "https://jobs-browser.bluey.sh/sessions/different-run",
              resolution: { kind: "browser_takeover", resumeAfter: true },
            },
          },
        },
        created_at_ms: Date.now(),
      }],
    }));

    expect(html).not.toContain("Approve submission");
    expect(html).toContain("Take over");
  });

  it("keeps the bounded email-code approval and takeover path", () => {
    const intervention: Intervention = {
      id: "intervention-email-code",
      application_id: "app-2",
      kind: "email_otp",
      status: "open",
      title: "Approve email code",
      detail: "Bluey found a current verification message.",
      choices: [],
      resolution_kind: "email_otp_approval",
      resume_after_resolution: true,
      provider: "gmail",
      provider_message_id: "message-1",
      expires_at_ms: Date.now() + 60_000,
      metadata: {},
      created_at_ms: Date.now(),
    };
    const html = renderAutomation(workspaceWith(availableCloud, {
      browser_sessions: [{
        id: "cloud-run-1",
        runner: "cloud",
        status: "needs_input",
        current_company: "Meridian Financial",
        current_step: "Email verification",
        application_id: "app-2",
        takeover_url: "https://jobs-browser.bluey.sh/sessions/cloud-run-1",
        created_at_ms: Date.now() - 60_000,
        updated_at_ms: Date.now(),
      }],
      interventions: [intervention],
    }));

    expect(html).toContain("Use email code");
    expect(html).toContain("Take over");
  });

  it("requires explicit confirmation and an open intervention before submission approval", () => {
    expect(submissionApprovalEnabled(false, false, true)).toBe(false);
    expect(submissionApprovalEnabled(true, true, true)).toBe(false);
    expect(submissionApprovalEnabled(true, false, false)).toBe(false);
    expect(submissionApprovalEnabled(true, false, true)).toBe(true);
  });

  it("does not carry final-review confirmation across an intervention replacement", () => {
    const title = "Review the Greenhouse application";
    const detail =
      "Review every employer-facing field and document in the preserved form, then approve submission.";
    const intervention = (id: string, takeoverUrl: string): Intervention => ({
      id,
      application_id: `application-${id}`,
      kind: "browser_takeover",
      status: "open",
      title,
      detail,
      choices: [],
      resolution_kind: "browser_takeover",
      resume_after_resolution: true,
      provider: "",
      provider_message_id: "",
      metadata: {
        receipt: {
          status: "needs_input",
          issues: [],
          intervention: {
            kind: "browser_takeover",
            title,
            detail,
            takeoverUrl,
            resolution: { kind: "browser_takeover", resumeAfter: true },
          },
        },
      },
      created_at_ms: Date.now(),
    });
    const urlA = "https://jobs-browser.bluey.sh/sessions/run-a";
    const urlB = "https://jobs-browser.bluey.sh/sessions/run-b";
    const targetA = { interventionId: "review-a", takeoverUrl: urlA };

    expect(submissionApprovalTargetMatches(
      targetA,
      intervention("review-a", urlA),
      urlA,
    )).toBe(true);
    expect(submissionApprovalTargetMatches(
      targetA,
      intervention("review-b", urlB),
      urlB,
    )).toBe(false);
    expect(submissionApprovalEnabled(true, false, false)).toBe(false);
  });

  it("labels a retained device session as recovery rather than cloud execution", () => {
    const html = renderAutomation(workspaceWith(availableCloud, {
      browser_sessions: [{
        id: "device-run-1",
        runner: "local",
        status: "needs_input",
        current_company: "Meridian Financial",
        current_step: "Recovery required",
        application_id: "app-2",
        created_at_ms: Date.now() - 60_000,
        updated_at_ms: Date.now(),
      }],
      interventions: [],
    }));

    expect(html).toContain("RETAINED DEVICE-SESSION RECOVERY");
    expect(html).toContain("New runs use managed cloud automation.");
    expect(html).not.toContain("ACTIVE CLOUD AUTOMATION RUN");
  });

  it("contains no dormant installer or local-queue implementation", () => {
    const source = readFileSync(new URL("./AutomationView.tsx", import.meta.url), "utf8");

    for (const forbidden of [
      "onQueueLocal",
      "BrowserInstallContent",
      "detectLocalBrowserTarget",
      "localBrowserRelease",
      "bluey-jobs://",
      "Set up browser",
      "Run locally",
    ]) {
      expect(source).not.toContain(forbidden);
    }
  });
});
