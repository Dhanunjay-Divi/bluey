import { fileURLToPath } from "node:url";
import { bundleWorkflowCode } from "@temporalio/worker";
import { describe, expect, it } from "vitest";

describe("production Temporal workflow bundle", () => {
  it("bundles both workflow protocols with the real SDK determinism boundary", async () => {
    const diagnostics: string[] = [];
    const collect = (message: string) => {
      const text = message.replace(/\x1b\[[0-9;]*m/g, "");
      for (const line of text.split("\n")) {
        if (/ERROR|UnhandledScheme|Can't resolve|Module build failed/i.test(line)) {
          if (diagnostics.length < 16) diagnostics.push(line.slice(0, 500));
        }
      }
    };
    try {
      const bundle = await bundleWorkflowCode({
        workflowsPath: fileURLToPath(new URL("../src/workflows.ts", import.meta.url)),
        failureConverterPath: fileURLToPath(new URL("../src/failure-converter.ts", import.meta.url)),
        logger: {
          log: (_level, message) => collect(message),
          trace: () => undefined,
          debug: () => undefined,
          info: () => undefined,
          warn: () => undefined,
          error: collect,
        },
      });
      expect(bundle.code).toContain("applicationWorkflowV2");
      expect(bundle.code).toContain("applicationWorkflow");
    } catch (error) {
      throw new Error(`${error instanceof Error ? error.message : "Workflow bundle failed"}\n${diagnostics.join("\n")}`);
    }
  }, 30_000);
});
