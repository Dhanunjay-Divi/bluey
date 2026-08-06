import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type {
  ApplicationEvidence,
  AtsCertificationSummary,
  JobApplication,
  JobEligibilityDecision,
  RunnerAvailability,
} from "../types";
import {
  answerInterventionActionLabel,
  applicationCountFor,
  applicationEligibility,
  applicationNeedsReview,
  hasVerifiedSubmissionEvidence,
  hasAvailableRunner,
  ReceiptView,
  runnerUnavailableReason,
} from "./ApplicationsView";

function certificationSummary(
  capability: JobEligibilityDecision["capability"],
): AtsCertificationSummary {
  const certified = capability === "certified";
  return {
    provider_label: certified ? "Lever" : "Application system",
    adapter_version: certified ? "2026.07.0-beta.1" : null,
    certified_runner_kinds: certified ? ["local"] : [],
    status: certified ? "active" : "review_only",
    last_verified_at_ms: certified ? Date.now() - 60_000 : null,
    expires_at_ms: certified ? Date.now() + 60 * 60_000 : null,
    reason: certified
      ? "The current job and Local Browser runner passed server verification."
      : "Review first is required for this application system.",
    next_action: certified
      ? "Review the application kit and choose an available runner."
      : "Review the application kit before continuing.",
    canary_available: false,
  };
}

const eligibility = (
  capability: JobEligibilityDecision["capability"],
  canQueue = false,
): JobEligibilityDecision => ({
  capability,
  can_prepare: true,
  can_auto_submit: canQueue,
  can_queue_local: canQueue,
  can_queue_cloud: canQueue,
  hard_failures: [],
  review_reasons: [],
  passed_checks: [],
  evaluated_at_ms: 1,
  ats_certification: certificationSummary(capability),
});

const runners = (available: boolean): RunnerAvailability => ({
  local: {
    status: available ? "available" : "invited_beta",
    available,
    plan_included: true,
    distribution_enabled: available,
    reason: available ? "Available." : "Bluey Browser is still in invited beta.",
    next_action: available ? "Run locally." : "Use Review first.",
  },
  cloud: {
    status: "upgrade_required",
    available: false,
    plan_included: false,
    distribution_enabled: false,
    reason: "Cloud plan required.",
    next_action: "View plans.",
  },
  auto_submit_available: available,
  auto_submit_reason: available
    ? "Auto-submit is available."
    : "Auto-submit is not available in this release because your included runner is still in invited beta.",
});

describe("reviewed application runner availability", () => {
  it("keeps beta, handoff, and unknown application systems in review or handoff", () => {
    expect(runnerUnavailableReason(eligibility("beta_review"), runners(true))).toContain("still in beta");
    expect(runnerUnavailableReason(eligibility("handoff"), runners(true))).toContain("user-controlled handoff");
    expect(runnerUnavailableReason(eligibility("unknown_review"), runners(true))).toContain("not certified");
  });

  it("requires an actually distributed runner even for a certified application", () => {
    const certified = eligibility("certified", true);

    expect(hasAvailableRunner(certified, runners(false))).toBe(false);
    expect(runnerUnavailableReason(certified, runners(false))).toContain("invited beta");
    expect(hasAvailableRunner(certified, runners(true))).toBe(true);
  });

  it("surfaces a hard Career Track failure before runner messaging", () => {
    const blocked = eligibility("certified", true);
    blocked.hard_failures = [{ code: "location", message: "This job is outside your selected locations." }];

    expect(runnerUnavailableReason(blocked, runners(false))).toBe(
      "This job is outside your selected locations.",
    );
  });

  it("does not queue a certified receipt from an older server without the bounded summary", () => {
    const application = {
      id: "application-one",
      job_id: "job-one",
      state: "awaiting_review",
      submission_mode: "auto_submit",
      match_score: 90,
      answers: [],
      cover_letter: "",
      receipt: { eligibility: { ...eligibility("certified", true), ats_certification: undefined } },
      created_at_ms: 1,
      updated_at_ms: 1,
    } as JobApplication;
    const decoded = applicationEligibility(application);

    expect(decoded.capability).toBe("unknown_review");
    expect(decoded.can_auto_submit).toBe(false);
    expect(hasAvailableRunner(decoded, runners(true))).toBe(false);
    expect(runnerUnavailableReason(decoded, runners(true))).toContain(
      "certification details are unavailable",
    );
  });
});

describe("uncertain submission review", () => {
  const application = (state: JobApplication["state"]): JobApplication => ({
    id: `application-${state}`,
    job_id: "job-one",
    state,
    submission_mode: "review_first",
    match_score: 91,
    resume_version_id: "resume-one",
    cover_letter: "",
    answers: [],
    receipt: {},
    run_id: "run-one",
    created_at_ms: 1,
    updated_at_ms: 1,
  });

  it("keeps a side-effect-unknown application in Needs review", () => {
    const uncertain = application("side_effect_unknown");

    expect(applicationNeedsReview(uncertain)).toBe(true);
    expect(applicationCountFor("review", [uncertain, application("submitted")])).toBe(1);
  });
});

describe("answer intervention review", () => {
  it("labels the answer action as a save-for-review step", () => {
    expect(answerInterventionActionLabel(false)).toBe("Save answer for review");
    expect(answerInterventionActionLabel(true)).toBe("Saving...");
  });
});

describe("submission receipt verification", () => {
  const receiptId = "receipt-one";
  const scope = "bluey-cloud/accounts/account-one";
  const resumeKey = `${scope}/context/jobs/application-one/resume.pdf`;
  const receiptKey = `${scope}/jobs/applications/application-one/receipt.json`;
  const confirmationKey = `${scope}/context/jobs/application-one/confirmation.png`;
  const resumeSha256 = "a".repeat(64);
  const receiptSha256 = "b".repeat(64);
  const confirmationSha256 = "c".repeat(64);

  const completeApplication = (): JobApplication => ({
    id: "application-one",
    job_id: "job-one",
    state: "submitted",
    submission_mode: "review_first",
    match_score: 91,
    resume_version_id: "resume-one",
    cover_letter: "",
    answers: [],
    receipt: {
      receiptId,
      accountId: "account-one",
      applicationId: "application-one",
      _bluey_server_submission_fingerprint_v1: "d".repeat(64),
      packet: { resumeVersionId: "resume-one" },
      documents: [{
        kind: "resume",
        versionId: "resume-one",
        storageKey: resumeKey,
        sha256: resumeSha256,
        mediaType: "application/pdf",
      }],
      screenshotKeys: [confirmationKey],
      evidenceObjects: [
        {
          kind: "resume",
          storageKey: resumeKey,
          sha256: resumeSha256,
          mediaType: "application/pdf",
          sizeBytes: 1_024,
        },
        {
          kind: "screenshot",
          storageKey: confirmationKey,
          sha256: confirmationSha256,
          mediaType: "image/png",
          sizeBytes: 4_096,
        },
      ],
      receiptObject: {
        storageKey: receiptKey,
        sha256: receiptSha256,
        mediaType: "application/json",
        sizeBytes: 2_048,
        schemaVersion: 1,
      },
    },
    created_at_ms: 1,
    updated_at_ms: 1,
    submitted_at_ms: 1,
  });

  const completeEvidence = (): ApplicationEvidence[] => [
    {
      id: "resume-evidence",
      application_id: "application-one",
      kind: "resume",
      label: "Resume submitted",
      provider: "greenhouse",
      file_name: "resume.pdf",
      media_type: "application/pdf",
      storage_key: resumeKey,
      sha256: resumeSha256,
      resume_version_id: "resume-one",
      occurred_at_ms: 1,
      metadata: { attached_to_submission: true, receipt_id: receiptId, size_bytes: 1_024 },
      created_at_ms: 1,
    },
    {
      id: "receipt-evidence",
      application_id: "application-one",
      kind: "application_receipt",
      label: "Application receipt bundle",
      provider: "greenhouse",
      file_name: "receipt.json",
      media_type: "application/json",
      storage_key: receiptKey,
      sha256: receiptSha256,
      resume_version_id: "resume-one",
      occurred_at_ms: 1,
      metadata: {
        immutable: true,
        receipt_id: receiptId,
        schema_version: 1,
        size_bytes: 2_048,
      },
      created_at_ms: 1,
    },
    {
      id: "confirmation-evidence",
      application_id: "application-one",
      kind: "submission_confirmation",
      label: "Application received",
      provider: "greenhouse",
      file_name: "confirmation.png",
      media_type: "image/png",
      storage_key: confirmationKey,
      sha256: confirmationSha256,
      resume_version_id: "resume-one",
      occurred_at_ms: 1,
      metadata: {
        evidence_strength: "browser_confirmed",
        confirmation: "Application received",
        receipt_id: receiptId,
        screenshot_keys: [confirmationKey],
        size_bytes: 4_096,
      },
      created_at_ms: 1,
    },
  ];

  const twoScreenshotEvidence = (): {
    application: JobApplication;
    evidence: ApplicationEvidence[];
  } => {
    const application = completeApplication();
    const evidence = completeEvidence();
    const secondKey = `${scope}/context/jobs/application-one/confirmation-2.png`;
    const secondSha256 = "e".repeat(64);
    const screenshotKeys = [confirmationKey, secondKey];
    application.receipt.screenshotKeys = screenshotKeys;
    const manifest = application.receipt.evidenceObjects as Array<Record<string, unknown>>;
    manifest.push({
      kind: "screenshot",
      storageKey: secondKey,
      sha256: secondSha256,
      mediaType: "image/png",
      sizeBytes: 4_097,
    });
    const first = evidence.find((item) => item.kind === "submission_confirmation")!;
    first.file_name = "submission-confirmation-1-of-2.png";
    first.metadata = {
      ...first.metadata,
      immutable: true,
      screenshot_keys: screenshotKeys,
      screenshot_index: 1,
      screenshot_count: 2,
    };
    evidence.push({
      ...first,
      id: "confirmation-evidence-2",
      file_name: "submission-confirmation-2-of-2.png",
      storage_key: secondKey,
      sha256: secondSha256,
      metadata: {
        ...first.metadata,
        screenshot_index: 2,
        size_bytes: 4_097,
      },
    });
    return { application, evidence };
  };

  it("verifies complete evidence for the application's exact resume", () => {
    expect(hasVerifiedSubmissionEvidence(completeApplication(), completeEvidence())).toBe(true);
  });

  it("requires canonical submitted state", () => {
    const application = completeApplication();
    application.state = "side_effect_unknown";

    expect(hasVerifiedSubmissionEvidence(application, completeEvidence())).toBe(false);
  });

  it("requires a positive canonical submission timestamp", () => {
    const missing = completeApplication();
    delete missing.submitted_at_ms;
    const zero = completeApplication();
    zero.submitted_at_ms = 0;

    expect(hasVerifiedSubmissionEvidence(missing, completeEvidence())).toBe(false);
    expect(hasVerifiedSubmissionEvidence(zero, completeEvidence())).toBe(false);
  });

  it.each(["resume", "application_receipt"] as const)(
    "requires exactly one %s record",
    (kind) => {
      const application = completeApplication();
      const missing = completeEvidence().filter((item) => item.kind !== kind);
      const duplicate = [...completeEvidence(), {
        ...completeEvidence().find((item) => item.kind === kind)!,
        id: `duplicate-${kind}`,
      }];

      expect(hasVerifiedSubmissionEvidence(application, missing)).toBe(false);
      expect(hasVerifiedSubmissionEvidence(application, duplicate)).toBe(false);
    },
  );

  it("verifies the exact complete set of two confirmation screenshots", () => {
    const { application, evidence } = twoScreenshotEvidence();

    expect(hasVerifiedSubmissionEvidence(application, evidence)).toBe(true);

    const missing = evidence.filter((item) => item.id !== "confirmation-evidence-2");
    expect(hasVerifiedSubmissionEvidence(application, missing)).toBe(false);

    const duplicate = twoScreenshotEvidence();
    duplicate.evidence.at(-1)!.metadata.screenshot_index = 1;
    expect(hasVerifiedSubmissionEvidence(duplicate.application, duplicate.evidence)).toBe(false);

    const mismatched = twoScreenshotEvidence();
    mismatched.evidence.at(-1)!.metadata.screenshot_keys = [
      confirmationKey,
      `${scope}/context/jobs/application-one/unbound.png`,
    ];
    expect(hasVerifiedSubmissionEvidence(mismatched.application, mismatched.evidence)).toBe(false);
  });

  it("retains verification for legacy single-screenshot evidence", () => {
    expect(hasVerifiedSubmissionEvidence(completeApplication(), completeEvidence())).toBe(true);

    const malformed = completeEvidence();
    malformed.at(-1)!.metadata.screenshot_index = "1";
    expect(hasVerifiedSubmissionEvidence(completeApplication(), malformed)).toBe(false);
  });

  it("rejects mismatched or empty immutable receipt identifiers", () => {
    const application = completeApplication();
    const mismatched = completeEvidence();
    mismatched[2].metadata.receipt_id = "another-receipt";
    const empty = completeEvidence();
    empty[1].metadata.receipt_id = " ";

    expect(hasVerifiedSubmissionEvidence(application, mismatched)).toBe(false);
    expect(hasVerifiedSubmissionEvidence(application, empty)).toBe(false);
  });

  it("requires the server fingerprint and application/account bindings", () => {
    const missingFingerprint = completeApplication();
    delete missingFingerprint.receipt._bluey_server_submission_fingerprint_v1;
    const wrongApplication = completeApplication();
    wrongApplication.receipt.applicationId = "application-two";
    const wrongAccount = completeApplication();
    wrongAccount.receipt.accountId = "account-two";

    expect(hasVerifiedSubmissionEvidence(missingFingerprint, completeEvidence())).toBe(false);
    expect(hasVerifiedSubmissionEvidence(wrongApplication, completeEvidence())).toBe(false);
    expect(hasVerifiedSubmissionEvidence(wrongAccount, completeEvidence())).toBe(false);
  });

  it("rejects a record or receipt document bound to another resume revision", () => {
    const wrongRecord = completeEvidence();
    wrongRecord[1].resume_version_id = "resume-two";
    const wrongDocument = completeApplication();
    const documents = wrongDocument.receipt.documents as Array<Record<string, unknown>>;
    documents[0].versionId = "resume-two";

    expect(hasVerifiedSubmissionEvidence(completeApplication(), wrongRecord)).toBe(false);
    expect(hasVerifiedSubmissionEvidence(wrongDocument, completeEvidence())).toBe(false);
  });

  it("requires immutable receipt and browser-confirmed metadata", () => {
    const mutableReceipt = completeEvidence();
    mutableReceipt[1].metadata.immutable = false;
    const weakConfirmation = completeEvidence();
    weakConfirmation[2].metadata.evidence_strength = "reported";

    expect(hasVerifiedSubmissionEvidence(completeApplication(), mutableReceipt)).toBe(false);
    expect(hasVerifiedSubmissionEvidence(completeApplication(), weakConfirmation)).toBe(false);
  });

  it("requires nonblank browser confirmation metadata", () => {
    const missing = completeEvidence();
    delete missing[2].metadata.confirmation;
    const blank = completeEvidence();
    blank[2].metadata.confirmation = " ";

    expect(hasVerifiedSubmissionEvidence(completeApplication(), missing)).toBe(false);
    expect(hasVerifiedSubmissionEvidence(completeApplication(), blank)).toBe(false);
  });

  it("requires valid hashes, expected media types, and positive object sizes", () => {
    const badHash = completeEvidence();
    badHash[0].sha256 = "not-a-sha256";
    const badMedia = completeEvidence();
    badMedia[2].media_type = "image/jpeg";
    const badSize = completeEvidence();
    badSize[1].metadata.size_bytes = 0;

    expect(hasVerifiedSubmissionEvidence(completeApplication(), badHash)).toBe(false);
    expect(hasVerifiedSubmissionEvidence(completeApplication(), badMedia)).toBe(false);
    expect(hasVerifiedSubmissionEvidence(completeApplication(), badSize)).toBe(false);
  });

  it("requires one common account-scoped object namespace", () => {
    const unscoped = completeEvidence();
    unscoped[0].storage_key = "jobs/application-one/resume.pdf";
    const anotherAccount = completeEvidence();
    anotherAccount[2].storage_key = confirmationKey.replace("account-one", "account-two");

    expect(hasVerifiedSubmissionEvidence(completeApplication(), unscoped)).toBe(false);
    expect(hasVerifiedSubmissionEvidence(completeApplication(), anotherAccount)).toBe(false);
  });

  it("binds the receipt object and exact resume document to their evidence records", () => {
    const wrongReceiptObject = completeApplication();
    const receiptObject = wrongReceiptObject.receipt.receiptObject as Record<string, unknown>;
    receiptObject.sha256 = "d".repeat(64);
    const wrongResumeDocument = completeApplication();
    const documents = wrongResumeDocument.receipt.documents as Array<Record<string, unknown>>;
    documents[0].storageKey = `${scope}/context/jobs/application-one/other.pdf`;

    expect(hasVerifiedSubmissionEvidence(wrongReceiptObject, completeEvidence())).toBe(false);
    expect(hasVerifiedSubmissionEvidence(wrongResumeDocument, completeEvidence())).toBe(false);
  });

  it("binds every confirmation to its immutable screenshot and manifest object", () => {
    const wrongScreenshot = completeApplication();
    wrongScreenshot.receipt.screenshotKeys = [`${scope}/context/jobs/application-one/other.png`];
    const wrongManifest = completeApplication();
    const manifest = wrongManifest.receipt.evidenceObjects as Array<Record<string, unknown>>;
    manifest[1].sha256 = "d".repeat(64);

    expect(hasVerifiedSubmissionEvidence(wrongScreenshot, completeEvidence())).toBe(false);
    expect(hasVerifiedSubmissionEvidence(wrongManifest, completeEvidence())).toBe(false);
  });

  it.each(["cover_letter", "attachment"] as const)(
    "reconciles every submitted %s across the receipt, manifest, and evidence record",
    (kind) => {
      const application = completeApplication();
      const evidence = completeEvidence();
      const storageKey = `${scope}/context/jobs/application-one/${kind}.pdf`;
      const sha256 = kind === "cover_letter" ? "e".repeat(64) : "f".repeat(64);
      const documents = application.receipt.documents as Array<Record<string, unknown>>;
      const manifest = application.receipt.evidenceObjects as Array<Record<string, unknown>>;
      documents.push({ kind, storageKey, sha256, mediaType: "application/pdf" });
      manifest.push({ kind, storageKey, sha256, mediaType: "application/pdf", sizeBytes: 512 });
      evidence.push({
        id: `${kind}-evidence`,
        application_id: application.id,
        kind,
        label: `${kind} submitted`,
        provider: "greenhouse",
        file_name: `${kind}.pdf`,
        media_type: "application/pdf",
        storage_key: storageKey,
        sha256,
        occurred_at_ms: 1,
        metadata: { attached_to_submission: true, receipt_id: receiptId, size_bytes: 512 },
        created_at_ms: 1,
      });

      expect(hasVerifiedSubmissionEvidence(application, evidence)).toBe(true);
      expect(hasVerifiedSubmissionEvidence(application, evidence.slice(0, -1))).toBe(false);
      evidence.at(-1)!.metadata.size_bytes = 511;
      expect(hasVerifiedSubmissionEvidence(application, evidence)).toBe(false);
    },
  );

  it("rejects a submitted document evidence record that is absent from the receipt", () => {
    const evidence = completeEvidence();
    evidence.push({
      ...evidence[0],
      id: "unreceipted-cover-letter",
      kind: "cover_letter",
      storage_key: `${scope}/context/jobs/application-one/unreceipted.pdf`,
      sha256: "e".repeat(64),
    });

    expect(hasVerifiedSubmissionEvidence(completeApplication(), evidence)).toBe(false);
  });

  it("renders verification and accessible receipt downloads only for a coherent set", () => {
    const validMarkup = renderToStaticMarkup(createElement(ReceiptView, {
      application: completeApplication(),
      evidence: completeEvidence(),
    }));
    const corruptEvidence = completeEvidence();
    corruptEvidence[2].sha256 = "invalid";
    const corruptMarkup = renderToStaticMarkup(createElement(ReceiptView, {
      application: completeApplication(),
      evidence: corruptEvidence,
    }));
    const malformedMetadata = completeEvidence();
    malformedMetadata[1].metadata = null as unknown as Record<string, unknown>;
    const malformedMarkup = renderToStaticMarkup(createElement(ReceiptView, {
      application: completeApplication(),
      evidence: malformedMetadata,
    }));
    const missingMarkup = renderToStaticMarkup(createElement(ReceiptView, {
      application: completeApplication(),
      evidence: [],
    }));

    expect(validMarkup).toContain("Submission verified");
    expect(validMarkup).toContain("Download submitted resume for application application-one");
    expect(validMarkup).toContain("Download receipt JSON for application application-one");
    expect(validMarkup).toContain("Download confirmation screenshot for application application-one");
    expect(corruptMarkup).toContain("Evidence not verified");
    expect(corruptMarkup).not.toContain("Submission verified");
    expect(malformedMarkup).toContain("Evidence not verified");
    expect(missingMarkup).toContain("Evidence not verified");
  });
});
