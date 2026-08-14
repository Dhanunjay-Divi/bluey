import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("protocol-v2 Temporal history source boundary", () => {
  it("keeps gateway arguments, memo, Update, and results free of private field names", () => {
    const gateway = [
      readFileSync(new URL("../src/gateway.ts", import.meta.url), "utf8"),
      readFileSync(new URL("../src/gateway-service.ts", import.meta.url), "utf8"),
      readFileSync(new URL("../src/gateway-cleanup-service.ts", import.meta.url), "utf8"),
    ].join("\n");
    const workflows = readFileSync(new URL("../src/workflows.ts", import.meta.url), "utf8");
    const v2 = workflows.slice(
      workflows.indexOf("export async function applicationWorkflowV2"),
      workflows.indexOf("export async function applicationWorkflow("),
    );

    for (const privateField of [
      "accountId",
      "applicationId",
      "applicationIdentityId",
      "browserProfileId",
      "browserSessionId",
      "canonicalJobKey",
      "packetId",
      "takeoverUrl",
      "resumeVersionId",
    ]) {
      expect(gateway).not.toContain(privateField);
      expect(v2).not.toContain(privateField);
    }
    expect(gateway).not.toContain("/workflows/applications");
    expect(gateway).not.toContain("resolveInterventionSignal");
    expect(gateway).not.toContain("console.error");
  });
});
