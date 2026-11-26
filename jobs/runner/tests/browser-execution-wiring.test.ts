import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const server = readFileSync(new URL("../src/server.ts", import.meta.url), "utf8");
const execution = readFileSync(
  new URL("../src/browser-run-execution.ts", import.meta.url),
  "utf8",
);

describe("production browser execution wiring", () => {
  it("routes both existing fresh and resumed executions through the shared body", () => {
    expect(server.match(/await executeRun\(/gu)).toHaveLength(2);
    const start = server.indexOf("async function executeRun(");
    const end = server.indexOf("function durableResultIdentity(", start);
    expect(start).toBeGreaterThan(0);
    expect(end).toBeGreaterThan(start);
    const wrapper = server.slice(start, end);
    expect(wrapper).toContain("return executeBrowserRun(");
    expect(wrapper).not.toContain("executeApplication(");
    expect(wrapper).toContain("withTrackedProfileResidency(lease, paths.scope");
    expect(wrapper).toContain("assertManagedProfileBinding(paths, storage)");
    expect(wrapper).toContain("assertTrackedAccountWorkMayContinue(input.browserSessionId, lease)");
    expect(wrapper).toContain("writeCloudCheckpoint({");
    expect(wrapper).toContain("writeManagedReceiptArtifact(paths, lease, runId, name, contents)");
    expect(wrapper).toContain("takeoverOrigin: () => process.env.BLUEY_JOBS_TAKEOVER_ORIGIN");
  });

  it("retains real checksum, document, provider, proof and receipt operations in the shared body", () => {
    for (const operation of [
      "assertApprovedExecutionChecksum(input.packet, input.job)",
      "materializeApplicationDocuments(",
      "assertMaterializedDocumentSnapshot(documents.resume)",
      "prepareFreshCloudPage(context, input.job)",
      "wrapActiveCloudPage(context, input.job)",
      "providerRegistryForExecution(",
      "executeApplication(adapterContext",
      "createFinalSubmitProof(",
      "authorizeCloudFinalSubmitBeforeCheckpoint(",
      "await lease.afterFinalSubmit(outcome)",
      "createApplicationReceipt({",
      "submissionReceiptAuthority(",
    ]) {
      expect(execution).toContain(operation);
    }
    expect(execution).toContain('import type {');
    expect(execution).not.toContain('from "node:http"');
    expect(execution).not.toContain("process.env");
  });
});
