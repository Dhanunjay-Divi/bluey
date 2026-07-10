import { proxyActivities } from "@temporalio/workflow";
import type { JobsActivities, ApplicationWorkflowInput, ApplicationWorkflowResult } from "./contracts.js";

const activities = proxyActivities<JobsActivities>({
  startToCloseTimeout: "10 minutes",
  retry: {
    initialInterval: "2 seconds",
    backoffCoefficient: 2,
    maximumInterval: "1 minute",
    maximumAttempts: 4,
  },
});

export async function applicationWorkflow(
  input: ApplicationWorkflowInput,
): Promise<ApplicationWorkflowResult> {
  await activities.assertEntitlement(input);
  await activities.loadPacket(input);
  await activities.recordState(input.applicationId, "running");
  const { browserSessionId } = await activities.allocateBrowser(input);
  try {
    const receipt = await activities.runApplication({ ...input, browserSessionId });
    if (receipt.status === "needs_input") {
      const interventionId = await activities.createIntervention(input.applicationId, receipt);
      await activities.recordState(input.applicationId, "needs_input");
      return { state: "needs_input", receipt, interventionId };
    }
    if (receipt.status === "submitted") {
      await activities.persistReceipt({ ...input, receipt });
      await activities.recordState(input.applicationId, "submitted");
      return { state: "submitted", receipt };
    }
    await activities.recordState(input.applicationId, "failed");
    return { state: "failed", receipt };
  } finally {
    await activities.releaseBrowser(browserSessionId);
  }
}
