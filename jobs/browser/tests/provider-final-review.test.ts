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
  localProviderConfirmationDisposition,
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
      canonicalJobUrl(adapter),
      "approve_submission=true Submit Application",
      confirmationUrl(adapter),
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

  it("quarantines markerless manual confirmation without synthesizing submitted", async () => {
    const runDirectory = await temporaryRunDirectory();
    const review = localProviderFinalReview(finalReviewExecution(adapter))!;
    const result = reconcileLocalProviderConfirmation(
      review,
      canonicalJobUrl(adapter),
      "Thank you for applying.\nperson@example.test\nYour application has been received.",
      confirmationUrl(adapter),
    );

    expect(result).toEqual({
      kind: "manual_submission_observed",
      binding: "exact_job",
    });
    expect(localProviderConfirmationDisposition(result, false)).toBe("manual_submission_observed");
    expect(JSON.stringify(result)).not.toContain("person@example.test");
    expect(JSON.stringify(result)).not.toContain(confirmationUrl(adapter));
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("never promotes observed confirmation to submitted from marker presence alone", async () => {
    const runDirectory = await temporaryRunDirectory();
    const hooks = durableFinalSubmitHooks(runDirectory);
    await hooks.beforeFinalSubmit(adapter === "greenhouse" ? {
      adapter,
      adapterVersion: "test-greenhouse",
      control: "greenhouse_submit_application",
      target: providerSubmitTarget("greenhouse"),
      files: providerSubmitFiles(),
      fields: providerSubmitFields(),
    } : {
      adapter,
      adapterVersion: "test-lever",
      control: "lever_application_submit",
      target: providerSubmitTarget("lever"),
      files: providerSubmitFiles(),
      fields: providerSubmitFields(),
    });
    const review = localProviderFinalReview(finalReviewExecution(adapter))!;
    const observation = reconcileLocalProviderConfirmation(
      review,
      canonicalJobUrl(adapter),
      "Thank you for applying.\nYour application has been received.",
      confirmationUrl(adapter),
    );

    const markerExists = await finalSubmitMarkerExists(runDirectory);
    expect(markerExists).toBe(true);
    expect(localProviderConfirmationDisposition(observation, markerExists))
      .toBe("manual_submission_observed");
    expect(observation).not.toHaveProperty("execution");
    expect(localProviderConfirmationDisposition(undefined, true)).toBe("submit_outcome_unknown");
  });

  it("continues only when the page has no credible submission-state evidence", () => {
    const review = localProviderFinalReview(finalReviewExecution(adapter))!;
    const expectedUrl = canonicalJobUrl(adapter);
    const explicitConfirmation = "Thank you for applying.\nYour application has been received.";

    expect(reconcileLocalProviderConfirmation(
      review,
      expectedUrl,
      "Thank you for applying.",
      confirmationUrl(adapter),
    )).toBeUndefined();
    expect(reconcileLocalProviderConfirmation(
      review,
      expectedUrl,
      explicitConfirmation,
      "https://jobs.example.test/acme/job-123/confirmation",
    )).toEqual({ kind: "manual_submission_observed", binding: "unbound" });
    expect(reconcileLocalProviderConfirmation(
      review,
      expectedUrl,
      explicitConfirmation,
      adapter === "greenhouse"
        ? "https://boards.greenhouse.io/acme/jobs/job-other/confirmation"
        : "https://jobs.lever.co/acme/job-other/confirmation",
    )).toEqual({ kind: "manual_submission_observed", binding: "unbound" });
    if (adapter === "greenhouse") {
      expect(reconcileLocalProviderConfirmation(
        review,
        expectedUrl,
        explicitConfirmation,
        "https://job-boards.greenhouse.io/acme/jobs/job-123/confirmation",
      )).toEqual({ kind: "manual_submission_observed", binding: "exact_job" });
    }
  });

  it.each([
    "You have already applied for this position.",
    "Your application was already submitted.",
    "Unable to submit your application.",
    "We could not submit the application.",
  ])("quarantines an unbound submission-state page: %s", (bodyText) => {
    const review = localProviderFinalReview(finalReviewExecution(adapter))!;

    const observation = reconcileLocalProviderConfirmation(
      review,
      canonicalJobUrl(adapter),
      bodyText,
      "https://jobs.example.test/submission-state",
    );

    expect(observation).toEqual({ kind: "manual_submission_observed", binding: "unbound" });
    expect(localProviderConfirmationDisposition(observation, false))
      .toBe("manual_submission_observed");
  });

  it("quarantines confirmation even when an already-applied footer is present", () => {
    const review = localProviderFinalReview(finalReviewExecution(adapter))!;
    const observation = reconcileLocalProviderConfirmation(
      review,
      canonicalJobUrl(adapter),
      "Your application has been received.\nAlready applied? Sign in to view your status.",
      confirmationUrl(adapter),
    );

    expect(observation).toEqual({
      kind: "manual_submission_observed",
      binding: "exact_job",
    });
  });
});

it("quarantines a real Lever tenant confirmation URL that omits the job id", () => {
  const review = localProviderFinalReview(finalReviewExecution("lever"))!;
  const observation = reconcileLocalProviderConfirmation(
    review,
    canonicalJobUrl("lever"),
    "Thank you for submitting your application.",
    "https://jobs.eu.lever.co/atlas/confirmation/candidate-123",
  );

  expect(observation).toEqual({ kind: "manual_submission_observed", binding: "unbound" });
  expect(localProviderConfirmationDisposition(observation, false))
    .toBe("manual_submission_observed");
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

function canonicalJobUrl(adapter: "greenhouse" | "lever"): string {
  return adapter === "greenhouse"
    ? "https://boards.greenhouse.io/acme/jobs/job-123"
    : "https://jobs.lever.co/acme/job-123";
}

function confirmationUrl(adapter: "greenhouse" | "lever"): string {
  return `${canonicalJobUrl(adapter)}/confirmation`;
}

function providerSubmitTarget(adapter: "greenhouse" | "lever") {
  const actionUrl = canonicalJobUrl(adapter);
  return {
    actionUrl,
    method: "post",
    enctype: "multipart/form-data",
    formTarget: "_self",
    providerJobKey: adapter === "greenhouse"
      ? "greenhouse:acme:job-123"
      : "lever:jobs.lever.co:acme:job-123",
    formIdentity: `${adapter}-form`,
  };
}

function providerSubmitFiles() {
  return [{
    fieldName: "resume",
    name: `resume-${"a".repeat(64)}.pdf`,
    byteLength: 1,
    sha256: "a".repeat(64),
  }];
}

function providerSubmitFields() {
  return [{
    fieldName: "job_id",
    valueByteLength: 3,
    valueSha256: "d".repeat(64),
  }];
}
