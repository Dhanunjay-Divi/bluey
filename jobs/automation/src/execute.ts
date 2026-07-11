import { AdapterRegistry } from "./adapters.js";
import type { AdapterContext, ApplicationAdapter, SubmissionReceipt } from "./contracts.js";
import { createStandardAdapters } from "./standard-adapters.js";

export interface ExecutionResult {
  adapter: string;
  adapterVersion: string;
  receipt: SubmissionReceipt;
}

export function createDefaultAdapterRegistry(fallback?: ApplicationAdapter): AdapterRegistry {
  const registry = new AdapterRegistry();
  for (const adapter of createStandardAdapters()) {
    if (fallback && adapter.kind === "semantic") continue;
    registry.register(adapter);
  }
  if (fallback) registry.register(fallback);
  return registry;
}

export async function executeApplication(
  context: AdapterContext,
  registry = createDefaultAdapterRegistry(),
): Promise<ExecutionResult> {
  const adapter = registry.resolve(context.page.url());
  await context.log("adapter_selected", { kind: adapter.kind, version: adapter.version });
  await adapter.prepare(context);
  await adapter.fill(context);
  const issues = await adapter.validate(context);
  if (issues.some((issue) => issue.severity === "blocking")) {
    const field = issues[0]?.field || "required field";
    const kind = issueKind(field);
    const receipt: SubmissionReceipt = {
      status: "needs_input",
      issues,
      intervention: {
        kind,
        title: kind === "sensitive_question"
          ? "Your choice is needed"
          : kind === "unknown_question"
            ? "A new question needs your answer"
            : "One detail is missing",
        detail: issues[0]?.message || "Complete the required application field.",
        field,
        resolution: { kind: "answer", resumeAfter: true },
      },
    };
    await context.log("application_execution_finished", {
      adapter: adapter.kind,
      status: receipt.status,
      issue_count: receipt.issues.length,
    });
    return { adapter: adapter.kind, adapterVersion: adapter.version, receipt };
  }
  const receipt = await adapter.submit(context);
  await context.log("application_execution_finished", {
    adapter: adapter.kind,
    status: receipt.status,
    issue_count: receipt.issues.length,
  });
  return { adapter: adapter.kind, adapterVersion: adapter.version, receipt };
}

function issueKind(field: string): "missing_fact" | "unknown_question" | "sensitive_question" {
  if (/gender|race|ethnic|disab|veteran|sexual orientation|religion/i.test(field)) {
    return "sensitive_question";
  }
  if (/name|email|phone|mobile|location|city|address|linkedin|portfolio|website|salary|compensation|sponsorship|visa|authoriz/i.test(field)) {
    return "missing_fact";
  }
  return "unknown_question";
}
