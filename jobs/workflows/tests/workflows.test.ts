import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  approvedExecutionChecksum,
  type ApplicationPacket,
  type NormalizedJob,
  type SubmissionReceipt,
} from "@bluey/jobs-automation";
import type {
  ApplicationWorkflowInput,
  InterventionResolution,
  WorkflowResumeCommandAuthority,
  WorkflowUpdateReceipt,
} from "../src/contracts.js";

const activities = {
  assertEntitlement: vi.fn(),
  loadPacket: vi.fn(),
  allocateBrowser: vi.fn(),
  persistSubmissionReceipt: vi.fn(),
  releaseBrowser: vi.fn(),
  recordState: vi.fn(),
  createIntervention: vi.fn(),
};
const runnerActivities = {
  runApplication: vi.fn(),
  resumeApplication: vi.fn(),
};
const opaqueActivities = {
  executeApplicationCommand: vi.fn(),
  resumeApplicationCommand: vi.fn(),
  publishApplicationIntervention: vi.fn(),
  finalizeApplicationCommand: vi.fn(),
};
const activityOptions: unknown[] = [];
let signalHandler: ((resolution: InterventionResolution) => void) | undefined;
let pendingResolution: InterventionResolution | undefined;
let updateHandler: ((authority: WorkflowResumeCommandAuthority) => WorkflowUpdateReceipt) | undefined;
let updateValidator: ((authority: WorkflowResumeCommandAuthority) => void) | undefined;
let pendingUpdates: WorkflowResumeCommandAuthority[] = [];
let updateErrors: Error[] = [];
let updateReceipts: WorkflowUpdateReceipt[] = [];
let conditionHook: (() => void) | undefined;

vi.mock("@temporalio/workflow", () => ({
  condition: vi.fn(async (predicate: () => boolean) => {
    if (pendingResolution) signalHandler?.(pendingResolution);
    conditionHook?.();
    while (!predicate() && pendingUpdates.length > 0) {
      const update = pendingUpdates.shift()!;
      try {
        updateValidator?.(update);
        const receipt = updateHandler?.(update);
        if (receipt) updateReceipts.push(receipt);
      } catch (error) {
        updateErrors.push(error as Error);
      }
    }
    return predicate();
  }),
  defineSignal: vi.fn(() => "resolveIntervention"),
  defineUpdate: vi.fn(() => "resolveInterventionV2"),
  ApplicationFailure: {
    nonRetryable: vi.fn((message: string, type: string) => Object.assign(new Error(message), { type })),
  },
  setHandler: vi.fn((definition: unknown, handler: (value: never) => unknown, options?: {
    validator?: (value: never) => void;
  }) => {
    if (definition === "resolveInterventionV2") {
      updateHandler = handler as unknown as typeof updateHandler;
      updateValidator = options?.validator as unknown as typeof updateValidator;
    } else {
      signalHandler = handler as unknown as typeof signalHandler;
    }
  }),
  proxyActivities: vi.fn((options: unknown) => {
    activityOptions.push(options);
    if (activityOptions.length === 1) return activities;
    if (activityOptions.length === 2) return runnerActivities;
    return opaqueActivities;
  }),
}));

const { applicationWorkflow, applicationWorkflowV2 } = await import("../src/workflows.js");

function workflowInput(): ApplicationWorkflowInput {
  const job: NormalizedJob = {
    externalId: "job-test",
    canonicalUrl: "https://boards.greenhouse.io/acme/jobs/123",
    source: "greenhouse",
    company: "Acme",
    title: "Engineer",
    location: "Remote",
    workplace: "remote",
    description: "Build things",
  };
  const packet: ApplicationPacket = {
    applicationId: "app-test",
    jobId: "job-test",
    resumeVersionId: "resume-test",
    approvedPacketChecksum: "",
    applicationIdentityId: "identity-test",
    browserProfileId: "profile-test",
    answers: {},
    verifiedClaimIds: [],
  };
  packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
  return {
    accountId: "acct-test",
    applicationId: "app-test",
    jobId: "job-test",
    canonicalJobKey: "canonical-job-test",
    packetId: "resume-test",
    applicationIdentityId: "identity-test",
    browserProfileId: "profile-test",
    packet,
    job,
    runner: "cloud",
    url: "https://boards.greenhouse.io/acme/jobs/123",
    idempotencyKey: "run-test",
  };
}

const needsAnswerReceipt: SubmissionReceipt = {
  status: "needs_input",
  issues: [],
  intervention: {
    kind: "unknown_question",
    title: "Answer required",
    detail: "The employer requires an answer.",
    resolution: { kind: "answer", resumeAfter: true },
  },
};

describe("application workflow irreversible boundary", () => {
  beforeEach(() => {
    activities.assertEntitlement.mockReset();
    activities.loadPacket.mockReset();
    activities.allocateBrowser.mockReset();
    activities.persistSubmissionReceipt.mockReset();
    activities.releaseBrowser.mockReset();
    activities.recordState.mockReset();
    activities.createIntervention.mockReset();
    runnerActivities.runApplication.mockReset();
    runnerActivities.resumeApplication.mockReset();
    opaqueActivities.executeApplicationCommand.mockReset();
    opaqueActivities.resumeApplicationCommand.mockReset();
    opaqueActivities.publishApplicationIntervention.mockReset();
    opaqueActivities.finalizeApplicationCommand.mockReset();
    signalHandler = undefined;
    pendingResolution = undefined;
    updateHandler = undefined;
    updateValidator = undefined;
    pendingUpdates = [];
    updateErrors = [];
    updateReceipts = [];
    conditionHook = undefined;
    activities.allocateBrowser.mockResolvedValue({ browserSessionId: "browser-test" });
    activities.persistSubmissionReceipt.mockResolvedValue(undefined);
    activities.createIntervention.mockResolvedValue("intervention-test");
  });

  it("configures runner and persistence recovery with bounded retries", () => {
    expect(activityOptions).toHaveLength(3);
    expect(activityOptions[0]).toMatchObject({ retry: { maximumAttempts: 4 } });
    expect(activityOptions[1]).toMatchObject({
      retry: {
        initialInterval: "2 seconds",
        maximumInterval: "1 minute",
        maximumAttempts: 4,
      },
    });
    expect(activityOptions[2]).toMatchObject({
      startToCloseTimeout: "10 minutes",
      retry: {
        initialInterval: "2 seconds",
        maximumInterval: "1 minute",
      },
    });
    expect((activityOptions[2] as { retry: Record<string, unknown> }).retry)
      .not.toHaveProperty("maximumAttempts");
  });

  it("holds an uncertain employer-side failure without releasing the browser", async () => {
    runnerActivities.runApplication.mockRejectedValueOnce(new Error("connection lost after submit"));
    const input = workflowInput();

    const result = await applicationWorkflow(input);

    expect(runnerActivities.runApplication).toHaveBeenCalledTimes(1);
    expect(runnerActivities.resumeApplication).not.toHaveBeenCalled();
    expect(activities.recordState).toHaveBeenNthCalledWith(1, input, "running");
    expect(activities.recordState).toHaveBeenNthCalledWith(2, input, "side_effect_unknown");
    expect(activities.releaseBrowser).not.toHaveBeenCalled();
    expect(result.state).toBe("side_effect_unknown");
    expect(result.receipt?.issues?.[0]?.message).toContain("will not submit again automatically");
  });

  it("requires packet reapproval for an answer-bearing intervention and never resumes", async () => {
    const input = workflowInput();
    runnerActivities.runApplication.mockResolvedValueOnce({ receipt: needsAnswerReceipt });
    pendingResolution = {
      action: "answer",
      field: "salary_expectation",
      answer: "$150,000",
    };

    const result = await applicationWorkflow(input);

    expect(result).toMatchObject({
      state: "needs_confirmation",
      interventionId: "intervention-test",
      requiresReapproval: true,
    });
    expect(runnerActivities.resumeApplication).not.toHaveBeenCalled();
    expect(activities.releaseBrowser).toHaveBeenCalledWith("browser-test");
    expect(activities.recordState).toHaveBeenCalledWith(
      expect.objectContaining({ applicationId: "app-test" }),
      "needs_confirmation",
    );
    expect(input.packet.answers).toEqual({});
  });

  it("accepts a submission only after exact durable-result persistence succeeds", async () => {
    const input = workflowInput();
    runnerActivities.runApplication.mockResolvedValueOnce({
      receipt: { status: "submitted", issues: [] },
    });

    const result = await applicationWorkflow(input);

    expect(activities.persistSubmissionReceipt).toHaveBeenCalledWith({
      ...input,
      browserSessionId: "browser-test",
      resultRequestId: "run-test:initial",
    });
    expect(activities.recordState).toHaveBeenCalledTimes(1);
    expect(activities.recordState).toHaveBeenCalledWith(input, "running");
    expect(activities.releaseBrowser).toHaveBeenCalledWith("browser-test");
    expect(result).toEqual({
      state: "submitted",
      receipt: { status: "submitted", issues: [] },
    });
  });

  it("leaves canonical persistence outages recoverable instead of labeling them unknown", async () => {
    const input = workflowInput();
    runnerActivities.runApplication.mockResolvedValueOnce({
      receipt: { status: "submitted", issues: [] },
    });
    activities.persistSubmissionReceipt.mockRejectedValueOnce(
      new Error("response lost after canonical receipt commit"),
    );

    await expect(applicationWorkflow(input)).rejects.toThrow("canonical receipt commit");

    expect(activities.releaseBrowser).not.toHaveBeenCalled();
    expect(activities.recordState).toHaveBeenCalledTimes(1);
    expect(activities.recordState).toHaveBeenCalledWith(input, "running");
    expect(activities.recordState).not.toHaveBeenCalledWith(input, "side_effect_unknown");
  });

  it("does not invoke receipt persistence for a failed runner result", async () => {
    const input = workflowInput();
    runnerActivities.runApplication.mockResolvedValueOnce({
      receipt: { status: "failed", issues: [] },
    });

    const result = await applicationWorkflow(input);

    expect(result.state).toBe("failed");
    expect(activities.persistSubmissionReceipt).not.toHaveBeenCalled();
    expect(activities.createIntervention).not.toHaveBeenCalled();
    expect(activities.releaseBrowser).toHaveBeenCalledWith("browser-test");
  });

  it("keeps canonical submitted state when browser cleanup fails", async () => {
    const input = workflowInput();
    runnerActivities.runApplication.mockResolvedValueOnce({
      receipt: { status: "submitted", issues: [] },
    });
    activities.releaseBrowser.mockRejectedValueOnce(new Error("runner unavailable"));

    const result = await applicationWorkflow(input);

    expect(result.state).toBe("submitted");
    expect(activities.persistSubmissionReceipt).toHaveBeenCalledTimes(1);
    expect(activities.recordState).toHaveBeenCalledTimes(1);
    expect(activities.recordState).not.toHaveBeenCalledWith(input, "side_effect_unknown");
  });

  it("resumes a content-neutral final review with the original approved packet", async () => {
    const input = workflowInput();
    runnerActivities.runApplication.mockResolvedValueOnce({ receipt: needsAnswerReceipt });
    runnerActivities.resumeApplication.mockResolvedValueOnce({
      receipt: { status: "failed", issues: [] },
    });
    pendingResolution = { action: "approve_submission" };

    const result = await applicationWorkflow(input);

    expect(result.state).toBe("failed");
    expect(runnerActivities.resumeApplication).toHaveBeenCalledTimes(1);
    const resumed = runnerActivities.resumeApplication.mock.calls[0]![0];
    expect(resumed.packet.answers).toEqual(input.packet.answers);
    expect(resumed.packet.approvedPacketChecksum).toBe(input.packet.approvedPacketChecksum);
    expect(resumed.resolution).toEqual({ action: "approve_submission" });
  });

  it("persists a resumed submission by its exact durable result request ID", async () => {
    const input = workflowInput();
    runnerActivities.runApplication.mockResolvedValueOnce({ receipt: needsAnswerReceipt });
    runnerActivities.resumeApplication.mockResolvedValueOnce({
      receipt: { status: "submitted", issues: [] },
    });
    pendingResolution = { action: "approve_submission" };

    const result = await applicationWorkflow(input);

    expect(result.state).toBe("submitted");
    expect(activities.persistSubmissionReceipt).toHaveBeenCalledWith({
      ...input,
      browserSessionId: "browser-test",
      resultRequestId: "run-test:resume:1",
    });
  });
});

function opaqueAuthority() {
  return {
    schemaVersion: 2 as const,
    requestId: `wfreq-v2-${"a".repeat(32)}`,
    workflowId: `bluey-jobs-v2-${"b".repeat(32)}`,
    payloadDigest: "c".repeat(64),
  };
}

function resumeAuthority(
  marker: string,
  interventionId: string,
): WorkflowResumeCommandAuthority {
  return {
    schemaVersion: 2,
    requestId: `wfreq-v2-${marker.repeat(32)}`,
    workflowId: opaqueAuthority().workflowId,
    payloadDigest: marker.repeat(64),
    interventionId,
  };
}

describe("application workflow v2 opaque authority", () => {
  beforeEach(() => {
    opaqueActivities.executeApplicationCommand.mockReset();
    opaqueActivities.resumeApplicationCommand.mockReset();
    opaqueActivities.publishApplicationIntervention.mockReset();
    opaqueActivities.finalizeApplicationCommand.mockReset();
    opaqueActivities.publishApplicationIntervention.mockImplementation(
      async ({ interventionId }: { interventionId: string }) => ({
        state: "needs_input",
        interventionId,
      }),
    );
    opaqueActivities.finalizeApplicationCommand.mockImplementation(
      async ({ terminalState }: { terminalState: "failed" | "side_effect_unknown" }) => ({
        state: terminalState,
      }),
    );
    updateHandler = undefined;
    updateValidator = undefined;
    pendingUpdates = [];
    updateErrors = [];
    updateReceipts = [];
    conditionHook = undefined;
  });

  it("serializes only opaque authority and returns a closed result", async () => {
    const authority = opaqueAuthority();
    opaqueActivities.executeApplicationCommand.mockResolvedValueOnce({ state: "submitted" });

    await expect(applicationWorkflowV2(authority)).resolves.toEqual({ state: "submitted" });

    expect(opaqueActivities.executeApplicationCommand).toHaveBeenCalledWith(authority);
    expect(JSON.stringify(opaqueActivities.executeApplicationCommand.mock.calls)).not.toContain("account");
    expect(JSON.stringify(opaqueActivities.executeApplicationCommand.mock.calls)).not.toContain("https://");
  });

  it.each([
    ["failed", "runner_failed"],
    ["side_effect_unknown", "runner_ambiguous"],
  ] as const)("durably finalizes a classified %s runner result", async (state, reasonCode) => {
    const authority = opaqueAuthority();
    opaqueActivities.executeApplicationCommand.mockResolvedValueOnce({ state });

    await expect(applicationWorkflowV2(authority)).resolves.toEqual({ state });

    expect(opaqueActivities.finalizeApplicationCommand).toHaveBeenCalledWith({
      command: authority,
      terminalState: state,
      reasonCode,
    });
  });

  it("rejects unknown workflow argument fields before executing an activity", async () => {
    await expect(applicationWorkflowV2({
      ...opaqueAuthority(),
      accountId: "private-account",
    } as never)).rejects.toThrow("invalid_authority");

    expect(opaqueActivities.executeApplicationCommand).not.toHaveBeenCalled();
  });

  it("rejects workflow authority shorter than the signed-route minimum", async () => {
    await expect(applicationWorkflowV2({
      ...opaqueAuthority(),
      requestId: "wfreq-v2-short",
    })).rejects.toThrow("invalid_authority");

    expect(opaqueActivities.executeApplicationCommand).not.toHaveBeenCalled();
  });

  it("rejects delayed intervention A after B opens and resumes only exact commands", async () => {
    const interventionA = `intervention-${"a".repeat(32)}`;
    const interventionB = `intervention-${"b".repeat(32)}`;
    const commandA = resumeAuthority("d", interventionA);
    const delayedA = resumeAuthority("e", interventionA);
    const commandB = resumeAuthority("f", interventionB);
    pendingUpdates = [commandA, delayedA, commandB];
    opaqueActivities.executeApplicationCommand.mockResolvedValueOnce({
      state: "intervention_prepared",
      interventionId: interventionA,
    });
    opaqueActivities.resumeApplicationCommand
      .mockResolvedValueOnce({ state: "intervention_prepared", interventionId: interventionB })
      .mockResolvedValueOnce({ state: "submitted" });

    await expect(applicationWorkflowV2(opaqueAuthority())).resolves.toEqual({ state: "submitted" });

    expect(updateErrors).toHaveLength(1);
    expect(updateErrors[0]?.message).toBe("identity_conflict");
    expect(updateReceipts.map((receipt) => receipt.requestId)).toEqual([
      commandA.requestId,
      commandB.requestId,
    ]);
    expect(opaqueActivities.resumeApplicationCommand).toHaveBeenNthCalledWith(1, {
      workflow: opaqueAuthority(),
      command: commandA,
    });
    expect(opaqueActivities.resumeApplicationCommand).toHaveBeenNthCalledWith(2, {
      workflow: opaqueAuthority(),
      command: commandB,
    });
  });

  it("returns the original receipt for an exact duplicate and rejects changed content", async () => {
    const intervention = `intervention-${"a".repeat(32)}`;
    opaqueActivities.executeApplicationCommand.mockResolvedValueOnce({
      state: "intervention_prepared",
      interventionId: intervention,
    });
    const original = resumeAuthority("d", intervention);
    conditionHook = () => {
      conditionHook = undefined;
      updateValidator?.(original);
      const first = updateHandler?.(original);
      updateValidator?.(original);
      const duplicate = updateHandler?.(original);
      expect(duplicate).toEqual(first);
      expect(() => updateValidator?.({ ...original, payloadDigest: "e".repeat(64) }))
        .toThrow("identity_conflict");
    };
    opaqueActivities.resumeApplicationCommand.mockResolvedValueOnce({ state: "failed" });
    await expect(applicationWorkflowV2(opaqueAuthority())).resolves.toEqual({ state: "failed" });
  });

  it("rejects an Update before an intervention is prepared and accepts its exact retry", async () => {
    const intervention = `intervention-${"a".repeat(32)}`;
    const command = resumeAuthority("d", intervention);
    opaqueActivities.executeApplicationCommand.mockImplementationOnce(async () => {
      try {
        updateValidator?.(command);
      } catch (error) {
        updateErrors.push(error as Error);
      }
      return { state: "intervention_prepared", interventionId: intervention };
    });
    pendingUpdates = [command];
    opaqueActivities.resumeApplicationCommand.mockResolvedValueOnce({ state: "failed" });

    await expect(applicationWorkflowV2(opaqueAuthority())).resolves.toEqual({ state: "failed" });

    expect(updateErrors.map((error) => error.message)).toEqual(["identity_conflict"]);
    expect(updateReceipts).toHaveLength(1);
    expect(updateReceipts[0]?.requestId).toBe(command.requestId);
  });

  it("sets exact Update authority before publishing the prepared intervention", async () => {
    const intervention = `intervention-${"a".repeat(32)}`;
    const command = resumeAuthority("d", intervention);
    opaqueActivities.executeApplicationCommand.mockResolvedValueOnce({
      state: "intervention_prepared",
      interventionId: intervention,
    });
    opaqueActivities.publishApplicationIntervention.mockImplementationOnce(async (input) => {
      updateValidator?.(command);
      updateHandler?.(command);
      return { state: "needs_input", interventionId: input.interventionId };
    });
    opaqueActivities.resumeApplicationCommand.mockResolvedValueOnce({ state: "failed" });

    await expect(applicationWorkflowV2(opaqueAuthority())).resolves.toEqual({ state: "failed" });

    expect(opaqueActivities.publishApplicationIntervention).toHaveBeenCalledWith({
      command: opaqueAuthority(),
      interventionId: intervention,
    });
    expect(opaqueActivities.resumeApplicationCommand).toHaveBeenCalledWith({
      workflow: opaqueAuthority(),
      command,
    });
  });

  it("accepts an Update after publish side effect but before the activity response", async () => {
    const intervention = `intervention-${"a".repeat(32)}`;
    const command = resumeAuthority("d", intervention);
    opaqueActivities.executeApplicationCommand.mockResolvedValueOnce({
      state: "intervention_prepared",
      interventionId: intervention,
    });
    opaqueActivities.publishApplicationIntervention.mockImplementationOnce(async (input) => {
      // The server has committed public visibility at this point. Temporal has
      // not received the activity completion yet, but the workflow already set
      // the exact open ID before it scheduled publication.
      updateValidator?.(command);
      const receipt = updateHandler?.(command);
      if (receipt) updateReceipts.push(receipt);
      return { state: "needs_input", interventionId: input.interventionId };
    });
    opaqueActivities.resumeApplicationCommand.mockResolvedValueOnce({ state: "failed" });

    await expect(applicationWorkflowV2(opaqueAuthority())).resolves.toEqual({ state: "failed" });

    expect(updateReceipts).toEqual([{ ...command, outcome: "accepted" }]);
    expect(opaqueActivities.resumeApplicationCommand).toHaveBeenCalledWith({
      workflow: opaqueAuthority(),
      command,
    });
  });

  it("durably finalizes and closes the exact intervention after its timeout", async () => {
    const intervention = `intervention-${"a".repeat(32)}`;
    const lateCommand = resumeAuthority("d", intervention);
    opaqueActivities.executeApplicationCommand.mockResolvedValueOnce({
      state: "intervention_prepared",
      interventionId: intervention,
    });
    opaqueActivities.finalizeApplicationCommand.mockImplementationOnce(async (input) => {
      expect(() => updateValidator?.(lateCommand)).toThrow("identity_conflict");
      return { state: input.terminalState };
    });

    await expect(applicationWorkflowV2(opaqueAuthority())).resolves.toEqual({ state: "failed" });

    expect(opaqueActivities.publishApplicationIntervention).toHaveBeenCalledTimes(1);
    expect(opaqueActivities.finalizeApplicationCommand).toHaveBeenCalledWith({
      command: opaqueAuthority(),
      terminalState: "failed",
      reasonCode: "intervention_timeout",
      openInterventionId: intervention,
    });
  });

  it("never sends an unpublished seventh intervention as finalization authority", async () => {
    const interventions = Array.from(
      { length: 7 },
      (_, index) => `intervention-${String(index).repeat(32)}`,
    );
    const commands = ["0", "1", "2", "3", "4", "5"].map((marker, index) =>
      resumeAuthority(marker, interventions[index]!));
    pendingUpdates = [...commands];
    opaqueActivities.executeApplicationCommand.mockResolvedValueOnce({
      state: "intervention_prepared",
      interventionId: interventions[0],
    });
    for (let index = 1; index < interventions.length; index += 1) {
      opaqueActivities.resumeApplicationCommand.mockResolvedValueOnce({
        state: "intervention_prepared",
        interventionId: interventions[index],
      });
    }

    await expect(applicationWorkflowV2(opaqueAuthority())).resolves.toEqual({ state: "failed" });

    expect(opaqueActivities.publishApplicationIntervention).toHaveBeenCalledTimes(6);
    expect(opaqueActivities.publishApplicationIntervention).not.toHaveBeenCalledWith(
      expect.objectContaining({ interventionId: interventions[6] }),
    );
    expect(opaqueActivities.finalizeApplicationCommand).toHaveBeenCalledWith({
      command: commands[5],
      terminalState: "failed",
      reasonCode: "intervention_limit",
    });
    expect(opaqueActivities.finalizeApplicationCommand.mock.calls[0]?.[0])
      .not.toHaveProperty("openInterventionId");
  });

  it("fails closed when publication echoes a different prepared intervention", async () => {
    const intervention = `intervention-${"a".repeat(32)}`;
    opaqueActivities.executeApplicationCommand.mockResolvedValueOnce({
      state: "intervention_prepared",
      interventionId: intervention,
    });
    opaqueActivities.publishApplicationIntervention.mockResolvedValueOnce({
      state: "needs_input",
      interventionId: `intervention-${"b".repeat(32)}`,
    });

    await expect(applicationWorkflowV2(opaqueAuthority())).rejects.toThrow("identity_conflict");

    expect(opaqueActivities.finalizeApplicationCommand).not.toHaveBeenCalled();
  });
});
