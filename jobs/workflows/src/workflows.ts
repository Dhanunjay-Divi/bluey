import { condition, defineSignal, proxyActivities, setHandler } from "@temporalio/workflow";
import {
  assertApprovedExecutionChecksum,
  createApprovedExecutionSnapshot,
  type SubmissionReceipt,
} from "@bluey/jobs-automation";
import type {
  ApplicationWorkflowInput,
  ApplicationWorkflowResult,
  InterventionResolution,
  JobsActivities,
  RunnerExecutionResult,
} from "./contracts.js";
import { decideInterventionResolution } from "./intervention-policy.js";

export const resolveInterventionSignal = defineSignal<[InterventionResolution]>("resolveIntervention");

const activities = proxyActivities<Omit<JobsActivities, "runApplication" | "resumeApplication">>({
  startToCloseTimeout: "10 minutes",
  retry: {
    initialInterval: "2 seconds",
    backoffCoefficient: 2,
    maximumInterval: "1 minute",
    maximumAttempts: 4,
  },
});

// Both calls recover a committed encrypted runner result before they invoke a
// browser step. Bounded Temporal retries therefore recover response loss
// without repeating an employer-facing Submit.
const runnerActivities = proxyActivities<Pick<JobsActivities, "runApplication" | "resumeApplication">>({
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
  let resolution: InterventionResolution | undefined;
  setHandler(resolveInterventionSignal, (value) => {
    resolution = value;
  });

  const approved = createApprovedExecutionSnapshot(input.packet, input.job);
  const approvedInput: ApplicationWorkflowInput = Object.freeze({
    ...input,
    packet: approved.approvedPacket,
    job: approved.approvedJob,
  });

  await activities.assertEntitlement(approvedInput);
  await activities.loadPacket(approvedInput);
  await activities.recordState(approvedInput, "running");
  const { browserSessionId } = await activities.allocateBrowser(approvedInput);
  assertApprovedExecutionChecksum(approvedInput.packet, approvedInput.job);
  let execution: RunnerExecutionResult;
  let resultRequestId = `${approvedInput.idempotencyKey}:initial`;
  try {
    execution = await runnerActivities.runApplication({ ...approvedInput, browserSessionId });
  } catch {
    return holdSideEffectUnknown(approvedInput);
  }
  for (let interventionCount = 0; interventionCount < 6; interventionCount += 1) {
    // Clear the previous answer before publishing the next intervention. A
    // fast user response that arrives after createIntervention must not be
    // erased before condition observes it.
    resolution = undefined;
    const step = await finishOrPause(
      approvedInput,
      browserSessionId,
      resultRequestId,
      execution,
    );
    if (step.result) return step.result;

    const resolved = await condition(() => resolution !== undefined, "24 hours");
    if (!resolved || !resolution) {
      await activities.recordState(approvedInput, "failed");
      await activities.releaseBrowser(browserSessionId);
      return {
        state: "failed",
        receipt: {
          status: "failed",
          issues: [{
            field: "intervention",
            message: "The application timed out while waiting for input.",
            severity: "blocking",
          }],
        },
      };
    }
    const decision = decideInterventionResolution(resolution);
    if (decision.kind === "requires_reapproval") {
      await activities.recordState(approvedInput, "needs_confirmation");
      await activities.releaseBrowser(browserSessionId);
      return {
        state: "needs_confirmation",
        receipt: requiresReapprovalReceipt(execution.receipt, decision.reason),
        interventionId: step.interventionId,
        requiresReapproval: true,
      };
    }
    try {
      assertApprovedExecutionChecksum(approvedInput.packet, approvedInput.job);
    } catch {
      await activities.recordState(approvedInput, "needs_confirmation");
      await activities.releaseBrowser(browserSessionId);
      return {
        state: "needs_confirmation",
        receipt: requiresReapprovalReceipt(
          execution.receipt,
          "The approved application packet changed and must be reviewed again.",
        ),
        interventionId: step.interventionId,
        requiresReapproval: true,
      };
    }
    await activities.recordState(approvedInput, "running");
    resultRequestId = `${approvedInput.idempotencyKey}:resume:${interventionCount + 1}`;
    try {
      execution = await runnerActivities.resumeApplication({
        ...approvedInput,
        browserSessionId,
        requestId: resultRequestId,
        resolution: decision.resolution,
      });
    } catch {
      return holdSideEffectUnknown(approvedInput);
    }
  }
  await activities.recordState(approvedInput, "failed");
  await activities.releaseBrowser(browserSessionId);
  return {
    state: "failed",
    receipt: {
      status: "failed",
      issues: [{
        field: "intervention",
        message: "This application required too many manual interventions.",
        severity: "blocking",
      }],
    },
  };
}

async function finishOrPause(
  input: ApplicationWorkflowInput,
  browserSessionId: string,
  resultRequestId: string,
  execution: RunnerExecutionResult,
): Promise<{ result?: ApplicationWorkflowResult; interventionId?: string }> {
  const receipt = execution.receipt;
  if (receipt.status === "needs_input") {
    const interventionId = await activities.createIntervention(input, receipt);
    await activities.recordState(input, "needs_input");
    return { interventionId };
  }
  if (receipt.status === "submitted") {
    // This activity is read-only against the runner and retries the exact
    // canonical receipt request. Its failure remains a recoverable workflow
    // failure; it is not evidence that the employer side effect is unknown.
    await activities.persistSubmissionReceipt({ ...input, browserSessionId, resultRequestId });
    // Receipt persistence is the canonical submitted transition. Browser
    // cleanup may retry, but a cleanup failure must never downgrade a committed
    // employer submission to side_effect_unknown.
    try {
      await activities.releaseBrowser(browserSessionId);
    } catch {
      // The runner's normal expiry/reconciliation path owns deferred cleanup.
    }
    return { result: { state: "submitted", receipt } };
  }
  await activities.recordState(input, "failed");
  await activities.releaseBrowser(browserSessionId);
  return { result: { state: "failed", receipt } };
}

async function holdSideEffectUnknown(
  input: ApplicationWorkflowInput,
): Promise<ApplicationWorkflowResult> {
  await activities.recordState(input, "side_effect_unknown");
  return {
    state: "side_effect_unknown",
    receipt: {
      status: "failed",
      issues: [{
        field: "submission",
        message: "Bluey lost confirmation during an employer-facing step. "
          + "The run is held for reconciliation and will not submit again automatically.",
        severity: "blocking",
      }],
    },
  };
}

function requiresReapprovalReceipt(receipt: SubmissionReceipt, reason: string): SubmissionReceipt {
  return {
    ...receipt,
    issues: [
      ...receipt.issues,
      { field: "approved_packet", message: reason, severity: "blocking" },
    ],
  };
}
