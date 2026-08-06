import { describe, expect, it } from "vitest";
import {
  assertRunnablePacket,
  assertSubmissionReceiptComplete,
  type ApplicationPacket,
  type ApplicationReceiptBundle,
  type AtsCertifiedReceiptAuthority,
  type CertifiedAutoSubmitAdmission,
} from "../src/index.js";

const SUCCESSFUL_SUBMIT_HTTP_STATUSES = [200, 204, 299, 301, 302, 303, 307, 308] as const;
const UNSUCCESSFUL_SUBMIT_HTTP_STATUSES = [199, 300, 304, 305, 306, 309, 399, 400] as const;

describe("application runner guards", () => {
  it("requires the frozen application identity and browser profile before a browser run", () => {
    expect(() => assertRunnablePacket(packet({ applicationIdentityId: undefined })))
      .toThrow("applicationIdentityId");
    expect(() => assertRunnablePacket(packet({ browserProfileId: "" })))
      .toThrow("browserProfileId");
    expect(() => assertRunnablePacket(packet())).not.toThrow();
  });

  it("requires submitted receipts to prove the exact resume, identity, and confirmation", () => {
    expect(() => assertSubmissionReceiptComplete(receipt())).not.toThrow();
    expect(() => assertSubmissionReceiptComplete(receipt({ documents: [] }))).toThrow("resume document");
    expect(() => assertSubmissionReceiptComplete(receipt({
      documents: [{ kind: "resume", versionId: "other-resume", storageKey: "resume.pdf", sha256: "a".repeat(64) }],
    }))).toThrow("resume document does not match");
    expect(() => assertSubmissionReceiptComplete(receipt({
      result: { status: "submitted", issues: [] },
      screenshotKeys: [],
    }))).toThrow("submission confirmation");
    expect(() => assertSubmissionReceiptComplete(receipt({
      result: { ...receipt().result, submitHttpStatus: 304 },
    }))).toThrow("successful submit HTTP status");
  });

  it.each(SUCCESSFUL_SUBMIT_HTTP_STATUSES)(
    "accepts submitted receipt HTTP status %i",
    (submitHttpStatus) => {
      expect(() => assertSubmissionReceiptComplete(receipt({
        result: { ...receipt().result, submitHttpStatus },
      }))).not.toThrow();
    },
  );

  it.each(UNSUCCESSFUL_SUBMIT_HTTP_STATUSES)(
    "rejects submitted receipt HTTP status %i",
    (submitHttpStatus) => {
      expect(() => assertSubmissionReceiptComplete(receipt({
        result: { ...receipt().result, submitHttpStatus },
      }))).toThrow("successful submit HTTP status");
    },
  );

  it("preserves schema-v1 review receipts and forbids certified authority on them", () => {
    const review = receipt();
    expect(review.schemaVersion).toBe(1);
    expect(() => assertSubmissionReceiptComplete(review)).not.toThrow();

    const injected = structuredClone(review);
    injected.atsCertifiedReceiptAuthority = certifiedAuthority();
    expect(() => assertSubmissionReceiptComplete(injected)).toThrow(
      "Review submission receipt cannot carry certified authority",
    );
    expect(() => assertSubmissionReceiptComplete({
      ...review,
      schemaVersion: 2,
    })).toThrow("Review submission receipt must use schema version 1");
  });

  it("requires schema-v2 authority for a frozen certified Auto admission", () => {
    const complete = certifiedReceipt();
    expect(() => assertSubmissionReceiptComplete(complete)).not.toThrow();

    const missing = structuredClone(complete);
    delete missing.atsCertifiedReceiptAuthority;
    expect(() => assertSubmissionReceiptComplete(missing)).toThrow(
      "Certified submission receipt authority is missing",
    );
    expect(() => assertSubmissionReceiptComplete({
      ...complete,
      schemaVersion: 1,
    })).toThrow("Certified submission receipt must use schema version 2");
  });

  it("rejects unknown certified receipt and frozen-packet fields", () => {
    const receiptField = certifiedReceipt() as unknown as Record<string, unknown>;
    receiptField.debug = true;
    expect(() => assertSubmissionReceiptComplete(
      receiptField as unknown as ApplicationReceiptBundle,
    )).toThrow("Certified submission receipt schema is invalid");

    const packetField = certifiedReceipt();
    (packetField.packet as unknown as Record<string, unknown>).credential = "forbidden";
    expect(() => assertSubmissionReceiptComplete(packetField)).toThrow(
      "Certified submission receipt packet schema is invalid",
    );
  });

  it.each([
    "schemaVersion",
    "accountId",
    "applicationId",
    "runId",
    "provider",
    "adapter",
    "adapterVersion",
    "manifestSha256",
    "activationSha256",
    "activationGeneration",
    "targetKeySha256",
    "layoutSetSha256",
    "layoutObservationSha256",
    "observedSurfaceSha256",
    "adapterBundleSha256",
    "runnerKind",
    "runnerTargetSha256",
    "bindingSha256",
    "bindingFence",
    "bindingConsumedAtMs",
    "applicationAttemptId",
    "phaseBRequestId",
    "rolloutChannel",
    "canaryReservationSha256",
    "meteringReservationSha256",
  ])("rejects certified authority missing %s", (field) => {
    const value = certifiedReceipt();
    delete (value.atsCertifiedReceiptAuthority as unknown as Record<string, unknown>)[field];
    expect(() => assertSubmissionReceiptComplete(value)).toThrow(
      "Certified submission receipt authority schema is invalid",
    );
  });

  it.each([
    ["uppercase digest", (authority: Record<string, unknown>) => {
      authority.layoutObservationSha256 = "F".repeat(64);
    }],
    ["zero activation generation", (authority: Record<string, unknown>) => {
      authority.activationGeneration = 0;
    }],
    ["unsafe binding fence", (authority: Record<string, unknown>) => {
      authority.bindingFence = Number.MAX_SAFE_INTEGER + 1;
    }],
    ["nonpositive consumed time", (authority: Record<string, unknown>) => {
      authority.bindingConsumedAtMs = 0;
    }],
    ["control-bearing attempt ID", (authority: Record<string, unknown>) => {
      authority.applicationAttemptId = "attempt\nother";
    }],
    ["unbounded Phase-B request ID", (authority: Record<string, unknown>) => {
      authority.phaseBRequestId = "x".repeat(241);
    }],
    ["shadow rollout", (authority: Record<string, unknown>) => {
      authority.rolloutChannel = "shadow";
    }],
    ["unknown field", (authority: Record<string, unknown>) => {
      authority.reusableCredential = "forbidden";
    }],
  ] as const)("rejects certified authority with %s", (_label, mutate) => {
    const value = certifiedReceipt();
    mutate(value.atsCertifiedReceiptAuthority as unknown as Record<string, unknown>);
    expect(() => assertSubmissionReceiptComplete(value)).toThrow(
      "Certified submission receipt authority schema is invalid",
    );
  });

  it.each([
    ["packet checksum", (value: ApplicationReceiptBundle) => {
      value.packet.approvedPacketChecksum = "C".repeat(64);
    }],
    ["document checksum", (value: ApplicationReceiptBundle) => {
      value.documents[0]!.sha256 = "A".repeat(64);
    }],
  ] as const)("rejects an uppercase certified %s", (_label, mutate) => {
    const value = certifiedReceipt();
    mutate(value);
    expect(() => assertSubmissionReceiptComplete(value)).toThrow(
      "Certified submission receipt digest is invalid",
    );
  });

  it.each([
    ["account", (authority: AtsCertifiedReceiptAuthority) => {
      authority.accountId = "account-other";
    }],
    ["application", (authority: AtsCertifiedReceiptAuthority) => {
      authority.applicationId = "application-other";
    }],
    ["run", (authority: AtsCertifiedReceiptAuthority) => {
      authority.runId = "run-other";
    }],
    ["selected runner", (authority: AtsCertifiedReceiptAuthority) => {
      authority.runnerTargetSha256 = "d".repeat(64);
    }],
  ] as const)("rejects cross-%s certified authority", (_label, mutate) => {
    const value = certifiedReceipt();
    mutate(value.atsCertifiedReceiptAuthority!);
    expect(() => assertSubmissionReceiptComplete(value)).toThrow(
      "does not match the frozen execution",
    );
  });

  it.each([
    ["provider", (value: ApplicationReceiptBundle) => {
      value.atsCertifiedReceiptAuthority!.provider = "lever";
    }],
    ["adapter", (value: ApplicationReceiptBundle) => {
      value.atsCertifiedReceiptAuthority!.adapter = "lever";
    }],
    ["receipt adapter", (value: ApplicationReceiptBundle) => {
      value.adapter = "lever";
    }],
    ["adapter version", (value: ApplicationReceiptBundle) => {
      value.atsCertifiedReceiptAuthority!.adapterVersion = "2026.07.0-beta.1";
    }],
    ["manifest", (value: ApplicationReceiptBundle) => {
      value.atsCertifiedReceiptAuthority!.manifestSha256 = "d".repeat(64);
    }],
    ["activation", (value: ApplicationReceiptBundle) => {
      value.atsCertifiedReceiptAuthority!.activationSha256 = "d".repeat(64);
    }],
    ["activation generation", (value: ApplicationReceiptBundle) => {
      value.atsCertifiedReceiptAuthority!.activationGeneration += 1;
    }],
    ["target", (value: ApplicationReceiptBundle) => {
      value.atsCertifiedReceiptAuthority!.targetKeySha256 = "d".repeat(64);
    }],
    ["layout set", (value: ApplicationReceiptBundle) => {
      value.atsCertifiedReceiptAuthority!.layoutSetSha256 = "d".repeat(64);
    }],
    ["observed surface", (value: ApplicationReceiptBundle) => {
      value.atsCertifiedReceiptAuthority!.observedSurfaceSha256 = "e".repeat(64);
    }],
    ["adapter bundle", (value: ApplicationReceiptBundle) => {
      value.atsCertifiedReceiptAuthority!.adapterBundleSha256 = "d".repeat(64);
    }],
    ["runner kind", (value: ApplicationReceiptBundle) => {
      value.atsCertifiedReceiptAuthority!.runnerKind = "cloud";
    }],
  ] as const)("rejects a cross-provider or mutated %s binding", (_label, mutate) => {
    const value = certifiedReceipt();
    mutate(value);
    expect(() => assertSubmissionReceiptComplete(value)).toThrow(
      "does not match the frozen execution",
    );
  });
});

function packet(overrides: Partial<ApplicationPacket> = {}): ApplicationPacket {
  return {
    applicationId: "application-1",
    jobId: "job-1",
    resumeVersionId: "resume-1",
    approvedPacketChecksum: "c".repeat(64),
    answers: { email: "ada@example.com" },
    verifiedClaimIds: ["claim-1"],
    applicationIdentityId: "identity-1",
    applicationEmail: "ada@example.com",
    browserProfileId: "profile-1",
    ...overrides,
  };
}

function receipt(overrides: Partial<ApplicationReceiptBundle> = {}): ApplicationReceiptBundle {
  return {
    schemaVersion: 1,
    receiptId: "receipt-1",
    accountId: "account-1",
    applicationId: "application-1",
    runId: "run-1",
    generatedAt: "2026-07-11T12:00:00.000Z",
    runner: "local",
    applicationIdentityId: "identity-1",
    browserProfileId: "profile-1",
    adapter: "greenhouse",
    adapterVersion: "1.0.0",
    job: {
      externalId: "job-1",
      canonicalUrl: "https://boards.greenhouse.io/acme/jobs/1",
      company: "Acme",
      title: "Software Engineer",
      location: "New York, NY",
      workplace: "hybrid",
      description: "Build.",
      source: "greenhouse",
    },
    packet: {
      jobId: "job-1",
      resumeVersionId: "resume-1",
      approvedPacketChecksum: "c".repeat(64),
      answers: { email: "ada@example.com" },
      verifiedClaimIds: ["claim-1"],
      applicationEmail: "ada@example.com",
    },
    documents: [{ kind: "resume", versionId: "resume-1", storageKey: "resume.pdf", sha256: "a".repeat(64) }],
    events: [],
    result: {
      status: "submitted",
      submitHttpStatus: 200,
      confirmationText: "Application received",
      confirmationUrl: "https://boards.greenhouse.io/acme/jobs/1/confirmation",
      submittedAt: "2026-07-11T12:00:00.000Z",
      issues: [],
    },
    finalUrl: "https://boards.greenhouse.io/acme/jobs/1/confirmation",
    screenshotKeys: ["receipt.png"],
    ...overrides,
  };
}

function certifiedReceipt(
  overrides: Partial<ApplicationReceiptBundle> = {},
): ApplicationReceiptBundle {
  const review = receipt();
  return {
    ...review,
    schemaVersion: 2,
    adapterVersion: "2026.07.1-beta.1",
    packet: {
      ...review.packet,
      approvedExecutionSchemaVersion: 3,
      approvedExecutionAdmission: certifiedAdmission(),
    },
    atsCertifiedReceiptAuthority: certifiedAuthority(),
    ...overrides,
  };
}

function certifiedAdmission(): CertifiedAutoSubmitAdmission {
  return {
    kind: "track_auto_submit",
    authorization_id: "authorization-1",
    career_track_id: "track-1",
    revision_no: 1,
    authority_fingerprint: "b".repeat(64),
    ats_certification: {
      schema_version: 1,
      provider: "greenhouse",
      adapter_version: "2026.07.1-beta.1",
      variant_key: "public",
      layout_contract_version: 1,
      surface_sha256: "d".repeat(64),
      manifest_sha256: "1".repeat(64),
      activation_sha256: "2".repeat(64),
      activation_generation: 3,
      target_key_sha256: "3".repeat(64),
      layout_set_sha256: "4".repeat(64),
      adapter_bundle_sha256: "6".repeat(64),
      runner_target_sha256s: ["7".repeat(64)],
      expires_at_ms: 9_007_199_254_740_000,
    },
  };
}

function certifiedAuthority(): AtsCertifiedReceiptAuthority {
  return {
    schemaVersion: 1,
    accountId: "account-1",
    applicationId: "application-1",
    runId: "run-1",
    provider: "greenhouse",
    adapter: "greenhouse",
    adapterVersion: "2026.07.1-beta.1",
    manifestSha256: "1".repeat(64),
    activationSha256: "2".repeat(64),
    activationGeneration: 3,
    targetKeySha256: "3".repeat(64),
    layoutSetSha256: "4".repeat(64),
    layoutObservationSha256: "5".repeat(64),
    observedSurfaceSha256: "d".repeat(64),
    adapterBundleSha256: "6".repeat(64),
    runnerKind: "local",
    runnerTargetSha256: "7".repeat(64),
    bindingSha256: "8".repeat(64),
    bindingFence: 4,
    bindingConsumedAtMs: Date.parse("2026-07-11T11:59:59.000Z"),
    applicationAttemptId: "attempt-1",
    phaseBRequestId: "phase-b-request-1",
    rolloutChannel: "canary",
    canaryReservationSha256: "9".repeat(64),
    meteringReservationSha256: "a".repeat(64),
  };
}
