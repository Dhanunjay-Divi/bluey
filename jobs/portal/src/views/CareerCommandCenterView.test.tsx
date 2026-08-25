import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { previewWorkspace } from "../data/preview";
import type { JobsWorkspace } from "../types";
import { CareerCommandCenterView } from "./CareerCommandCenterView";

const now = (previewWorkspace.discovery_sources[0].last_success_at_ms ?? Date.now()) + 60_000;

function renderCommandCenter(
  workspace: JobsWorkspace = previewWorkspace,
  previewSearch = "",
): string {
  return renderToStaticMarkup(
    <MemoryRouter>
      <CareerCommandCenterView
        workspace={workspace}
        previewSearch={previewSearch}
        nowMs={now}
      />
    </MemoryRouter>,
  );
}

describe("career command center", () => {
  it("renders a read-only operating view from the existing workspace", () => {
    const html = renderCommandCenter();

    expect(html).toContain("CAREER OPERATIONS");
    expect(html).toContain("Good to see you, Taylor.");
    expect(html).toContain("SETUP READINESS");
    expect(html).toContain("NEXT BEST ACTIONS");
    expect(html).toContain("DISCOVERY CONTROL PLANE");
    expect(html).toContain("APPLICATION OUTCOMES");
    expect(html).toContain("CONNECTED SERVICES");
    expect(html).toContain("PLAN &amp; EXECUTION");
    expect(html).toContain("Eligibility, approval, and communication sent are not inferred");
    expect(html).toContain("no policy, exact-approval, or communication receipts");
    expect(html).toContain("Candidate event · Confirmed");
    expect(html).toContain("Nothing runs from this page.");
    expect(html).toContain("3 of 5 required checks ready");
    expect(html).toContain("Application inbox (optional)");
    expect(html).toContain("Freshness does not establish eligibility.");
    expect(html).not.toMatch(/onClick|<button/i);
  });

  it("shows connection truth without claiming LinkedIn access", () => {
    const html = renderCommandCenter();

    expect(html).toContain("Gmail inbox");
    expect(html).toContain("taylor@example.com");
    expect(html).toContain("Connected");
    expect(html).toContain("Bluey Jobs has no official LinkedIn connector.");
    expect(html).toContain("Unavailable");
    expect(html).not.toMatch(/LinkedIn(?:.|\n){0,120}Connected/i);
  });

  it("renders exact server runner and plan availability instead of optimistic copy", () => {
    const locked: JobsWorkspace = {
      ...previewWorkspace,
      runner_availability: {
        ...previewWorkspace.runner_availability,
        cloud: {
          status: "invited_beta",
          available: false,
          plan_included: true,
          distribution_enabled: false,
          reason: "Background automation is not enabled for this account.",
          next_action: "Use Review first while access is unavailable.",
        },
        auto_submit_available: false,
        auto_submit_reason: "Auto-submit is unavailable while the managed runner is disabled.",
      },
    };
    const html = renderCommandCenter(locked);

    expect(html).toContain("Cloud plan");
    expect(html).toContain("24");
    expect(html).toContain("of 100 packets used");
    expect(html).toContain("Managed cloud runner");
    expect(html).toContain("Invited Beta");
    expect(html).toContain("Background automation is not enabled for this account.");
    expect(html).toContain("Use Review first while access is unavailable.");
    expect(html).toContain("Auto-submit is unavailable while the managed runner is disabled.");
  });

  it("shows local runner availability when cloud execution is unavailable", () => {
    const localOnly: JobsWorkspace = {
      ...previewWorkspace,
      runner_availability: {
        ...previewWorkspace.runner_availability,
        cloud: {
          ...previewWorkspace.runner_availability.cloud,
          available: false,
          status: "upgrade_required",
          reason: "Managed cloud execution is unavailable.",
          next_action: "Use an eligible local runner.",
        },
        local: {
          ...previewWorkspace.runner_availability.local,
          available: true,
          status: "available",
          reason: "A signed local browser runner is available.",
          next_action: "Approve an exact packet before queueing it locally.",
        },
        auto_submit_available: false,
        auto_submit_reason: "Auto-submit is unavailable while managed cloud execution is disabled.",
      },
    };
    const html = renderCommandCenter(localOnly);

    expect(html).toContain("Managed cloud runner");
    expect(html).toContain("Managed cloud execution is unavailable.");
    expect(html).toContain("Local browser runner");
    expect(html).toContain("A signed local browser runner is available.");
    expect(html).toContain("Approve an exact packet before queueing it locally.");
  });

  it("renders a paused source as paused even when its health field says healthy", () => {
    const pausedHealthy: JobsWorkspace = {
      ...previewWorkspace,
      discovery_sources: [{
        ...previewWorkspace.discovery_sources[0],
        status: "paused",
        health: "healthy",
      }],
    };
    const html = renderCommandCenter(pausedHealthy);

    expect(html).toContain('class="source-dot paused"');
    expect(html).toContain('class="source-state paused"');
    expect(html).not.toContain('class="source-dot healthy"');
  });

  it("does not claim a null server verification timestamp was verified", () => {
    const unverified: JobsWorkspace = {
      ...previewWorkspace,
      matches: [{
        ...previewWorkspace.matches[0],
        posted_at_ms: now - 60_000,
        last_verified_at_ms: null,
        availability_status: "active",
      }],
    };
    const html = renderCommandCenter(unverified);

    expect(html).toContain("Verification unavailable");
    expect(html).not.toContain("verified No successful sync yet");
    expect(html).toContain("Open the source details and verification evidence");
  });

  it("renders a stale stored-healthy source as degraded", () => {
    const staleHealthy: JobsWorkspace = {
      ...previewWorkspace,
      discovery_sources: [{
        ...previewWorkspace.discovery_sources[0],
        status: "active",
        health: "healthy",
        last_success_at_ms: now - 13 * 60 * 60_000,
      }],
    };
    const html = renderCommandCenter(staleHealthy);

    expect(html).toContain('class="source-dot degraded"');
    expect(html).toContain('class="source-state degraded"');
    expect(html).not.toContain('class="source-dot healthy"');
  });

  it("renders explicit empty and disconnected states", () => {
    const empty: JobsWorkspace = {
      ...previewWorkspace,
      matches: [],
      applications: [],
      application_evidence: [],
      interventions: [],
      candidate_events: [],
      mailbox_connections: [],
      integrations: [],
      discovery_sources: [],
    };
    const html = renderCommandCenter(empty);

    expect(html).toContain("No active match arrived in the last 24 hours");
    expect(html).toContain("No open interventions");
    expect(html).toContain("No discovery source is enrolled");
    expect(html).toContain("Connect an employer source from Matches.");
    expect(html).toContain("No recent candidate outcomes");
    expect(html).toContain("Preparation, provider, ambiguity, and correction evidence remain");
    expect(html).toContain("No Gmail or Outlook inbox is connected.");
    expect(html).toContain("No connected calendar account");
    expect(html).toContain("Unavailable");
    expect(html).toContain("Not connected");
  });

  it("shows connected Outlook Calendar even when Google Calendar is disconnected", () => {
    const dualCalendar: JobsWorkspace = {
      ...previewWorkspace,
      integrations: [
        {
          id: "google-calendar",
          provider: "google_calendar",
          status: "disconnected",
          account_label: "",
          capabilities: ["interview_calendar"],
          updated_at_ms: now,
        },
        {
          id: "outlook-calendar",
          provider: "outlook_calendar",
          status: "connected",
          account_label: "candidate@example.test",
          capabilities: ["interview_calendar"],
          updated_at_ms: now,
        },
      ],
    };
    const html = renderCommandCenter(dualCalendar);

    expect(html).toContain("Outlook Calendar");
    expect(html).toContain("candidate@example.test");
    expect(html).toContain("Google Calendar");
    expect(html).toContain("Disconnected");
  });

  it("preserves preview state on every deep link", () => {
    const html = renderCommandCenter(previewWorkspace, "?preview=1&scenario=command");

    expect(html).toContain("/matches?preview=1&amp;scenario=command");
    expect(html).toContain("/applications?preview=1&amp;scenario=command");
    expect(html).toContain("/settings?preview=1&amp;scenario=command");
    expect(html).toContain("/automation?preview=1&amp;scenario=command");
  });

  it("keeps the command center responsive at tablet and phone widths", () => {
    const css = readFileSync(
      new URL("./CareerCommandCenterView.css", import.meta.url),
      "utf8",
    );

    expect(css).toContain("@media (max-width: 1040px)");
    expect(css).toContain("@media (max-width: 640px)");
    expect(css).toMatch(/\.career-command-primary-grid,[\s\S]*grid-template-columns: 1fr/);
    expect(css).toMatch(/\.career-command-metrics[\s\S]*grid-template-columns: 1fr/);
  });

  it("keeps normal light-theme Command Center copy above WCAG AA contrast", () => {
    const css = readFileSync(
      new URL("./CareerCommandCenterView.css", import.meta.url),
      "utf8",
    );
    const foregrounds = ["#4f5d65", "#006a99"];
    const surfaces = ["#ffffff", "#f8fafb"];

    expect(css).toContain("--command-copy: var(--text-soft)");
    expect(css).toContain("--command-accent-copy: #006a99");
    for (const foreground of foregrounds) {
      for (const surface of surfaces) {
        expect(contrastRatio(foreground, surface)).toBeGreaterThanOrEqual(4.5);
      }
    }
  });
});

function contrastRatio(foreground: string, background: string): number {
  const foregroundLuminance = relativeLuminance(foreground);
  const backgroundLuminance = relativeLuminance(background);
  const lighter = Math.max(foregroundLuminance, backgroundLuminance);
  const darker = Math.min(foregroundLuminance, backgroundLuminance);
  return (lighter + 0.05) / (darker + 0.05);
}

function relativeLuminance(hex: string): number {
  const channels = hex.slice(1).match(/.{2}/g)?.map((channel) => parseInt(channel, 16) / 255);
  if (!channels || channels.length !== 3) throw new Error(`Invalid hex color: ${hex}`);
  const [red, green, blue] = channels.map((channel) =>
    channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4,
  );
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}
