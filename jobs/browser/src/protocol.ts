import { parseLocalRunCapability } from "./local-capabilities.js";

export type BlueyJobsProtocolCommand =
  | { action: "open" }
  | { action: "takeover" }
  | { action: "run"; runId: string; ticket: string }
  | { action: "resume"; runId: string; capability: string };

export function parseBlueyJobsProtocol(rawUrl: string, nowMs = Date.now()): BlueyJobsProtocolCommand {
  const url = new URL(rawUrl);
  if (url.protocol !== "bluey-jobs:") throw new Error("Invalid Bluey Browser link");
  if (url.hostname === "open" || url.hostname === "takeover") {
    return { action: url.hostname };
  }
  if (url.hostname !== "run" && url.hostname !== "resume") {
    throw new Error("Invalid Bluey Browser action");
  }
  const runId = url.pathname.split("/").filter(Boolean)[0] || "";
  if (!/^[A-Za-z0-9_-]{3,160}$/.test(runId)) throw new Error("Invalid Bluey Browser run");
  if (url.hostname === "resume") {
    const capability = url.searchParams.get("capability") || "";
    parseLocalRunCapability(capability, "resume", runId, nowMs);
    return { action: "resume", runId, capability };
  }
  const ticket = url.searchParams.get("ticket") || "";
  if (!/^[a-f0-9]{64}$/i.test(ticket)) throw new Error("Invalid Bluey Browser ticket");
  return { action: "run", runId, ticket };
}
