import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  approvedExecutionChecksum,
  type ApplicationPacket,
  type NormalizedJob,
  type SubmissionReceipt,
} from "@bluey/jobs-automation";
import type { ApplicationWorkflowInput, InterventionResolution } from "../src/contracts.js";

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
const activityOptions: unknown[] = [];
let signalHandler: ((resolution: InterventionResolution) => void) | undefined;
let pendingResolution: InterventionResolution | undefined;

vi.mock("@temporalio/workflow", () => ({
  condition: vi.fn(async (predicate: () => boolean) => {
    if (pendingResolution) signalHandler?.(pendingResolution);
    return predicate();
  }),
  defineSignal: vi.fn(() => "resolveIntervention"),
  setHandler: vi.fn((_signal: unknown, handler: (resolution: InterventionResolution) => void) => {
    signalHandler = handler;
  }),
  proxyActivities: vi.fn((options: unknown) => {
    activityOptions.push(options);
    return activityOptions.length === 1 ? activities : runnerActivities;
  }),
}));

const { applicationWorkflow } = await import("../src/workflows.js");

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
    signalHandler = undefined;
    pendingResolution = undefined;
    activities.allocateBrowser.mockResolvedValue({ browserSessionId: "browser-test" });
    activities.persistSubmissionReceipt.mockResolvedValue(undefined);
    activities.createIntervention.mockResolvedValue("intervention-test");
  });

  it("configures runner and persistence recovery with bounded retries", () => {
    expect(activityOptions).toHaveLength(2);
    expect(activityOptions[0]).toMatchObject({ retry: { maximumAttempts: 4 } });
    expect(activityOptions[1]).toMatchObject({
      retry: {
        initialInterval: "2 seconds",
        maximumInterval: "1 minute",
        maximumAttempts: 4,
      },
    });
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
