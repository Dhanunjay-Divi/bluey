import type { ApplicationState, SubmissionReceipt } from "@bluey/jobs-automation";
import type { ApplicationWorkflowInput } from "./contracts.js";

const apiOrigin = process.env.BLUEY_JOBS_API_ORIGIN || "http://127.0.0.1:8080";
const serviceToken = process.env.BLUEY_JOBS_WORKER_TOKEN || "";

export async function assertEntitlement(input: ApplicationWorkflowInput): Promise<void> {
  await event(input.applicationId, "entitlement_checked", { runner: input.runner });
}

export async function loadPacket(input: ApplicationWorkflowInput): Promise<void> {
  await event(input.applicationId, "packet_loaded", { packet_id: input.packetId });
}

export async function allocateBrowser(input: ApplicationWorkflowInput): Promise<{ browserSessionId: string }> {
  const browserSessionId = `${input.runner}-${input.applicationId}`;
  await event(input.applicationId, "browser_allocated", { browser_session_id: browserSessionId });
  return { browserSessionId };
}

export async function runApplication(input: ApplicationWorkflowInput & { browserSessionId: string }): Promise<SubmissionReceipt> {
  await event(input.applicationId, "runner_requested", {
    runner: input.runner,
    browser_session_id: input.browserSessionId,
    url: input.url,
  });
  return {
    status: "needs_input",
    issues: [{ field: "runner", message: "No browser pool activity adapter is configured.", severity: "blocking" }],
    intervention: {
      kind: "browser_takeover",
      title: "Application runner needs setup",
      detail: "Connect an enabled browser pool before this workflow can submit.",
    },
  };
}

export async function persistReceipt(input: ApplicationWorkflowInput & { receipt: SubmissionReceipt }): Promise<void> {
  await event(input.applicationId, "receipt_persisted", input.receipt);
}

export async function releaseBrowser(browserSessionId: string): Promise<void> {
  await event(browserSessionId, "browser_released", {});
}

export async function recordState(applicationId: string, state: ApplicationState): Promise<void> {
  await event(applicationId, "state_changed", { state });
}

export async function createIntervention(applicationId: string, receipt: SubmissionReceipt): Promise<string> {
  const id = `intervention-${applicationId}`;
  await event(applicationId, "intervention_created", { id, intervention: receipt.intervention });
  return id;
}

async function event(runId: string, type: string, body: unknown): Promise<void> {
  if (!serviceToken) throw new Error("BLUEY_JOBS_WORKER_TOKEN is required");
  const response = await fetch(`${apiOrigin}/api/jobs/internal/runs/${encodeURIComponent(runId)}/events`, {
    method: "POST",
    headers: { Authorization: `Bearer ${serviceToken}`, "Content-Type": "application/json" },
    body: JSON.stringify({ type, body }),
  });
  if (!response.ok) throw new Error(`Jobs API returned ${response.status}`);
}
