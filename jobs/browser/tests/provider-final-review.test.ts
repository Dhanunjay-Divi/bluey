import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import type { ExecutionResult } from "@bluey/jobs-automation";
import { durableFinalSubmitHooks, finalSubmitMarkerExists } from "../src/irreversible-submit.js";
import { classifyLocalFailure } from "../src/local-failure.js";
import {
  LOCAL_PROVIDER_APPROVAL_PENDING,
  isApprovedLocalResumeAction,
  localProviderFinalReview,
  pendingProviderReviewReceipt,
  providerOptionsForApprovedReview,
  reconcileLocalProviderConfirmation,
} from "../src/provider-final-review.js";
import { parseBlueyJobsProtocol } from "../src/protocol.js";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, {
    recursive: true,
    force: true,
  })));
});

describe.each(["greenhouse", "lever"] as const)("local %s final review", (adapter) => {
  it("fails closed without creating pre-click submit authority", async () => {
    const runDirectory = await temporaryRunDirectory();
    durableFinalSubmitHooks(runDirectory);
    const execution = finalReviewExecution(adapter);

    const review = localProviderFinalReview(execution);
    const pending = pendingProviderReviewReceipt();

    expect(review).toEqual({ adapter, adapterVersion: `test-${adapter}` });
    expect(execution.receipt.intervention?.resolution?.resumeAfter).toBe(true);
    expect(pending).toMatchObject({
      status: "needs_input",
      issues: [],
      intervention: {
        kind: "browser_takeover",
        title: "Submission approval required",
        detail: LOCAL_PROVIDER_APPROVAL_PENDING,
        resolution: { kind: "browser_takeover", resumeAfter: true },
      },
    });
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
    await expect(classifyLocalFailure(runDirectory, new Error("review pause"))).resolves.toMatchObject({
      status: "failed",
      preservePage: false,
    });
  });

  it("does not treat a caller-controlled approval parameter as authority", () => {
    const capability = localResumeCapability();
    const command = parseBlueyJobsProtocol(
      `bluey-jobs://resume/run-123?capability=${encodeURIComponent(capability)}&approve_submission=true`,
      1_000,
    );
    const review = localProviderFinalReview(finalReviewExecution(adapter))!;

    expect(command).toEqual({ action: "resume", runId: "run-123", capability });
    expect(isApprovedLocalResumeAction(command, "run-123")).toBe(false);
    expect(reconcileLocalProviderConfirmation(
      review,
      "approve_submission=true Submit Application",
      "https://jobs.example.test/apply",
    )).toBeUndefined();
  });

  it("accepts only a live matching server resume action", async () => {
    const review = localProviderFinalReview(finalReviewExecution(adapter))!;
    const action = {
      run_id: "run-123",
      intervention_id: "intervention-123",
      action: "approve_submission",
      expires_at_ms: 2_000,
    };

    expect(isApprovedLocalResumeAction(action, "run-123", 1_000)).toBe(true);
    expect(isApprovedLocalResumeAction({ ...action, action: "resume" }, "run-123", 1_000)).toBe(false);
    expect(isApprovedLocalResumeAction({ ...action, run_id: "run-other" }, "run-123", 1_000)).toBe(false);
    expect(isApprovedLocalResumeAction(action, "run-123", 2_000)).toBe(false);
    const options = providerOptionsForApprovedReview(review);
    const approval = adapter === "greenhouse"
      ? options.greenhouse?.finalReviewApproval
      : options.lever?.finalReviewApproval;
    await expect(approval?.({} as never)).resolves.toBe(true);
  });

  it("reconciles only explicit employer confirmation", () => {
    const review = localProviderFinalReview(finalReviewExecution(adapter))!;
    const result = reconcileLocalProviderConfirmation(
      review,
      "Thank you for submitting your application. We have received your application.",
      "https://jobs.example.test/confirmation",
      () => new Date("2026-07-12T12:00:00.000Z"),
    );

    expect(result).toMatchObject({
      adapter,
      adapterVersion: `test-${adapter}`,
      receipt: {
        status: "submitted",
        confirmationUrl: "https://jobs.example.test/confirmation",
        submittedAt: "2026-07-12T12:00:00.000Z",
        issues: [],
      },
    });
  });
});

function finalReviewExecution(adapter: "greenhouse" | "lever"): ExecutionResult {
  const review = adapter === "greenhouse"
    ? {
        title: "Review the Greenhouse application",
        detail: "Review every employer-facing field and document in the preserved form, then approve submission.",
      }
    : {
        title: "Review this Lever application",
        detail: "Review every answer and attachment in the preserved browser. Bluey will not submit until you explicitly approve final review.",
      };
  return {
    adapter,
    adapterVersion: `test-${adapter}`,
    receipt: {
      status: "needs_input",
      issues: [],
      intervention: {
        kind: "browser_takeover",
        ...review,
        resolution: { kind: "browser_takeover", resumeAfter: true },
      },
    },
  };
}

async function temporaryRunDirectory(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "bluey-provider-review-"));
  temporaryDirectories.push(root);
  const runDirectory = join(root, "identity-hash", "runs", "run-123");
  await mkdir(runDirectory, { recursive: true });
  return runDirectory;
}

function localResumeCapability(): string {
  const claims = {
    version: 1,
    audience: "bluey-jobs-local-run",
    account_id: "account-123",
    application_id: "application-123",
    run_id: "run-123",
    browser_profile_id: "profile-123",
    operation: "resume",
    expires_at_ms: 2_000,
    nonce: "n".repeat(32),
  };
  return `${Buffer.from(JSON.stringify(claims)).toString("base64url")}.${"a".repeat(64)}`;
}
