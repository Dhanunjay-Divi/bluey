export type BlueyJobsProtocolCommand =
  | { action: "open" }
  | { action: "takeover" }
  | { action: "run"; runId: string; ticket: string }
  | { action: "resume"; runId: string; ticket: string };

export function parseBlueyJobsProtocol(rawUrl: string): BlueyJobsProtocolCommand {
  const url = new URL(rawUrl);
  if (url.protocol !== "bluey-jobs:") throw new Error("Invalid Bluey Browser link");
  if (url.hostname === "open" || url.hostname === "takeover") {
    return { action: url.hostname };
  }
  if (url.hostname !== "run" && url.hostname !== "resume") {
    throw new Error("Invalid Bluey Browser action");
  }
  const runId = url.pathname.split("/").filter(Boolean)[0] || "";
  const ticket = url.searchParams.get("ticket") || "";
  if (!/^[A-Za-z0-9_-]{3,160}$/.test(runId)) throw new Error("Invalid Bluey Browser run");
  if (!/^[a-f0-9]{64}$/i.test(ticket)) throw new Error("Invalid Bluey Browser ticket");
  return { action: url.hostname, runId, ticket };
}
