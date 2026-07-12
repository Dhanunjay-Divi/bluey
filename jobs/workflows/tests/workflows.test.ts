import { beforeEach, describe, expect, it, vi } from "vitest";

const activities = {
  assertEntitlement: vi.fn(),
  loadPacket: vi.fn(),
  allocateBrowser: vi.fn(),
  persistReceipt: vi.fn(),
  releaseBrowser: vi.fn(),
  recordState: vi.fn(),
  createIntervention: vi.fn(),
};
const irreversibleActivities = {
  runApplication: vi.fn(),
  resumeApplication: vi.fn(),
};
const activityOptions: unknown[] = [];

vi.mock("@temporalio/workflow", () => ({
  condition: vi.fn(),
  defineSignal: vi.fn(() => "resolveIntervention"),
  setHandler: vi.fn(),
  proxyActivities: vi.fn((options: unknown) => {
    activityOptions.push(options);
    return activityOptions.length === 1 ? activities : irreversibleActivities;
  }),
}));

const { applicationWorkflow } = await import("../src/workflows.js");

const input = {
  accountId: "acct-test",
  applicationId: "app-test",
  jobId: "job-test",
  canonicalJobKey: "canonical-job-test",
  packetId: "resume-test",
  applicationIdentityId: "identity-test",
  browserProfileId: "profile-test",
  packet: {
    applicationId: "app-test",
    jobId: "job-test",
    resumeVersionId: "resume-test",
    applicationIdentityId: "identity-test",
    browserProfileId: "profile-test",
    answers: {},
  },
  job: {
    id: "job-test",
    canonicalKey: "canonical-job-test",
    canonicalUrl: "https://boards.greenhouse.io/acme/jobs/123",
    source: "greenhouse",
    company: "Acme",
    title: "Engineer",
    location: "Remote",
    workplace: "remote",
    employmentType: "full_time",
    description: "Build things",
    fetchedAt: new Date(0).toISOString(),
  },
  runner: "cloud" as const,
  url: "https://boards.greenhouse.io/acme/jobs/123",
  idempotencyKey: "run-test",
};

describe("application workflow irreversible boundary", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    activities.allocateBrowser.mockResolvedValue({ browserSessionId: "browser-test" });
  });

  it("configures employer-facing activities for one attempt only", () => {
    expect(activityOptions).toHaveLength(2);
    expect(activityOptions[1]).toMatchObject({ retry: { maximumAttempts: 1 } });
  });

  it("holds an uncertain employer-side failure without retrying or releasing the browser", async () => {
    irreversibleActivities.runApplication.mockRejectedValueOnce(new Error("connection lost after submit"));

    const result = await applicationWorkflow(input);

    expect(irreversibleActivities.runApplication).toHaveBeenCalledTimes(1);
    expect(irreversibleActivities.resumeApplication).not.toHaveBeenCalled();
    expect(activities.recordState).toHaveBeenNthCalledWith(1, input, "running");
    expect(activities.recordState).toHaveBeenNthCalledWith(2, input, "side_effect_unknown");
    expect(activities.releaseBrowser).not.toHaveBeenCalled();
    expect(result.state).toBe("side_effect_unknown");
    expect(result.receipt?.issues?.[0]?.message).toContain("will not submit again automatically");
  });
});
