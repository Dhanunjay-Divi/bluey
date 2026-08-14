import {
  ApplicationFailure,
  condition,
  defineSignal,
  defineUpdate,
  proxyActivities,
  setHandler,
} from "@temporalio/workflow";
import {
  assertApprovedExecutionChecksum,
  createApprovedExecutionSnapshot,
  type SubmissionReceipt,
} from "@bluey/jobs-automation";
import type {
  ManagedCloudReleaseMemoAuthority,
} from "@bluey/jobs-automation/managed-cloud-execution";
import type {
  ApplicationWorkflowInput,
  ApplicationWorkflowResult,
  InterventionResolution,
  JobsActivities,
  OpaqueWorkflowActivities,
  RunnerExecutionResult,
  WorkflowCommandAuthority,
  WorkflowCommandStep,
  WorkflowPublishedIntervention,
  WorkflowResumeCommandAuthority,
  WorkflowUpdateReceipt,
} from "./contracts.js";
import { decideInterventionResolution } from "./intervention-policy.js";

export const resolveInterventionSignal = defineSignal<[InterventionResolution]>("resolveIntervention");
export const resolveInterventionUpdate = defineUpdate<
  WorkflowUpdateReceipt,
  [WorkflowResumeCommandAuthority]
>("resolveInterventionV2");

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

const opaqueActivities = proxyActivities<OpaqueWorkflowActivities>({
  startToCloseTimeout: "10 minutes",
  retry: {
    initialInterval: "2 seconds",
    backoffCoefficient: 2,
    maximumInterval: "1 minute",
  },
});

/**
 * Protocol-v2 workflow. Only opaque random identifiers and an authenticated
 * digest are visible to Temporal; private execution data is loaded inside the
 * activities and never crosses the deterministic workflow boundary.
 */
export async function applicationWorkflowV2(
  authority: WorkflowCommandAuthority,
  managedCloudRelease?: ManagedCloudReleaseMemoAuthority,
): Promise<{ state: "submitted" | "failed" | "side_effect_unknown" }> {
  assertWorkflowAuthority(authority);
  if (managedCloudRelease !== undefined) {
    assertManagedCloudReleaseMemo(managedCloudRelease);
  }
  let openInterventionId: string | undefined;
  let pendingResolution: WorkflowResumeCommandAuthority | undefined;
  const acceptedUpdates = new Map<string, WorkflowUpdateReceipt>();

  setHandler(
    resolveInterventionUpdate,
    (value): WorkflowUpdateReceipt => {
      const existing = acceptedUpdates.get(value.requestId);
      if (existing) return existing;
      const receipt = Object.freeze({ ...value, outcome: "accepted" as const });
      acceptedUpdates.set(value.requestId, receipt);
      pendingResolution = Object.freeze({ ...value });
      return receipt;
    },
    {
      validator: (value) => {
        assertResumeAuthority(value);
        const existing = acceptedUpdates.get(value.requestId);
        if (existing) {
          if (!sameResumeAuthority(existing, value)) throwIdentityConflict();
          return;
        }
        if (value.workflowId !== authority.workflowId
          || value.interventionId !== openInterventionId
          || pendingResolution !== undefined) {
          throwIdentityConflict();
        }
      },
    },
  );

  let command: WorkflowCommandAuthority | WorkflowResumeCommandAuthority = authority;
  let step = assertCommandStep(await (managedCloudRelease === undefined
    ? opaqueActivities.executeApplicationCommand(authority)
    : opaqueActivities.executeManagedApplicationCommand({
      command: authority,
      managedCloudRelease,
    })));

  for (let interventionCount = 0; step.state === "intervention_prepared";) {
    openInterventionId = step.interventionId;
    if (interventionCount >= 6) {
      // The seventh prompt is still hidden preparation state. It was never
      // published, so it is not valid public intervention authority for
      // finalization.
      openInterventionId = undefined;
      return assertTerminalResult(await opaqueActivities.finalizeApplicationCommand({
        command,
        terminalState: "failed",
        reasonCode: "intervention_limit",
      }));
    }
    interventionCount += 1;
    pendingResolution = undefined;
    assertPublishedIntervention(
      await opaqueActivities.publishApplicationIntervention({
        command,
        interventionId: openInterventionId,
      }),
      openInterventionId,
    );
    const resolved = await condition(() => pendingResolution !== undefined, "24 hours");
    if (!resolved || !pendingResolution) {
      const terminalInterventionId = openInterventionId;
      openInterventionId = undefined;
      pendingResolution = undefined;
      return assertTerminalResult(await opaqueActivities.finalizeApplicationCommand({
        command,
        terminalState: "failed",
        reasonCode: "intervention_timeout",
        openInterventionId: terminalInterventionId,
      }));
    }
    const resumeCommand = pendingResolution;
    openInterventionId = undefined;
    pendingResolution = undefined;
    command = resumeCommand;
    step = assertCommandStep(await (managedCloudRelease === undefined
      ? opaqueActivities.resumeApplicationCommand({
        workflow: authority,
        command: resumeCommand,
      })
      : opaqueActivities.resumeManagedApplicationCommand({
        workflow: authority,
        command: resumeCommand,
        managedCloudRelease,
      })));
  }
  if (step.state === "submitted") return { state: "submitted" };
  return assertTerminalResult(await opaqueActivities.finalizeApplicationCommand({
    command,
    terminalState: step.state,
    reasonCode: step.state === "failed" ? "runner_failed" : "runner_ambiguous",
  }));
}

function assertCommandStep(value: WorkflowCommandStep): WorkflowCommandStep {
  if (value.state === "intervention_prepared") {
    if (!hasExactKeys(value, ["interventionId", "state"])
      || !OPAQUE_ID.test(value.interventionId)) {
      throwInvalidAuthority();
    }
    return value;
  }
  if (!hasExactKeys(value, ["state"])
    || (value.state !== "submitted"
      && value.state !== "failed"
      && value.state !== "side_effect_unknown")) {
    throwInvalidAuthority();
  }
  return value;
}

function assertPublishedIntervention(
  value: WorkflowPublishedIntervention,
  expectedInterventionId: string,
): void {
  if (!hasExactKeys(value, ["interventionId", "state"])
    || value.state !== "needs_input"
    || value.interventionId !== expectedInterventionId) {
    throwIdentityConflict();
  }
}

function assertTerminalResult(
  value: Awaited<ReturnType<OpaqueWorkflowActivities["finalizeApplicationCommand"]>>,
): { state: "failed" | "side_effect_unknown" } {
  if (!hasExactKeys(value, ["state"])
    || (value.state !== "failed" && value.state !== "side_effect_unknown")) {
    throwIdentityConflict();
  }
  return value;
}

function assertWorkflowAuthority(value: WorkflowCommandAuthority): void {
  if (!hasExactKeys(value, ["payloadDigest", "requestId", "schemaVersion", "workflowId"])
    || value.schemaVersion !== 2
    || !OPAQUE_ID.test(value.requestId)
    || !OPAQUE_ID.test(value.workflowId)
    || !DIGEST.test(value.payloadDigest)) {
    throwInvalidAuthority();
  }
}

function assertManagedCloudReleaseMemo(value: ManagedCloudReleaseMemoAuthority): void {
  if (!hasExactKeys(value, [
    "activationExpiresAtMs",
    "activationSha256",
    "bindingSha256",
    "channelSequence",
    "cohortSha256",
    "failureConverterSha256",
    "headRevision",
    "manifestSha256",
    "readinessSha256",
    "releaseId",
    "releaseSequence",
    "resolvedAtMs",
    "scope",
    "taskQueueSha256",
    "transitionSha256",
    "trustGeneration",
    "version",
  ])
    || value.version !== 1
    || !hasExactKeys(value.scope, ["channel", "environment", "region"])
    || (value.scope.environment !== "staging" && value.scope.environment !== "production")
    || (value.scope.channel !== "canary" && value.scope.channel !== "general")
    || !/^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/.test(value.scope.region)
    || !OPAQUE_ID.test(value.releaseId)
    || ![
      value.activationSha256,
      value.bindingSha256,
      value.cohortSha256,
      value.failureConverterSha256,
      value.manifestSha256,
      value.readinessSha256,
      value.taskQueueSha256,
      value.transitionSha256,
    ].every((digest) => DIGEST.test(digest))
    || ![
      value.activationExpiresAtMs,
      value.channelSequence,
      value.headRevision,
      value.releaseSequence,
      value.resolvedAtMs,
      value.trustGeneration,
    ].every((number) => Number.isSafeInteger(number) && number > 0)
    || value.resolvedAtMs >= value.activationExpiresAtMs) {
    throwInvalidAuthority();
  }
}

function assertResumeAuthority(value: WorkflowResumeCommandAuthority): void {
  if (!hasExactKeys(
    value,
    ["interventionId", "payloadDigest", "requestId", "schemaVersion", "workflowId"],
  )
    || value.schemaVersion !== 2
    || !OPAQUE_ID.test(value.requestId)
    || !OPAQUE_ID.test(value.workflowId)
    || !DIGEST.test(value.payloadDigest)
    || !OPAQUE_ID.test(value.interventionId)) {
    throwInvalidAuthority();
  }
}

function sameResumeAuthority(
  left: WorkflowResumeCommandAuthority,
  right: WorkflowResumeCommandAuthority,
): boolean {
  return left.schemaVersion === right.schemaVersion
    && left.requestId === right.requestId
    && left.workflowId === right.workflowId
    && left.payloadDigest === right.payloadDigest
    && left.interventionId === right.interventionId;
}

function hasExactKeys(value: unknown, expected: readonly string[]): boolean {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const keys = Object.keys(value).sort();
  return keys.length === expected.length && keys.every((key, index) => key === expected[index]);
}

function throwInvalidAuthority(): never {
  throw ApplicationFailure.nonRetryable("invalid_authority", "invalid_authority");
}

function throwIdentityConflict(): never {
  throw ApplicationFailure.nonRetryable("identity_conflict", "identity_conflict");
}

const OPAQUE_ID = /^[A-Za-z0-9_-]{20,200}$/;
const DIGEST = /^[a-f0-9]{64}$/;

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
