import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  assertFinalSubmitProof,
  createFinalSubmitProof,
  finalSubmitSurfaceSha256,
  FinalSubmitProofError,
} from "../src/final-submit-proof.js";
import type { ApplicationPacket, FinalSubmitProof } from "../src/contracts.js";
import {
  assertCertifiedProviderNavigationJob,
  certifiedProviderJobKey,
} from "../src/provider-job-key.js";

const RESUME_SHA = "a".repeat(64);
const COVER_SHA = "b".repeat(64);
const GREENHOUSE_JOB = {
  approvedCanonicalUrl: "https://boards.greenhouse.io/acme/jobs/123",
  pageUrl: "https://job-boards.greenhouse.io/embed/job_app?for=acme&token=123",
};
const LEVER_JOB = {
  approvedCanonicalUrl: "https://jobs.lever.co/acme/posting-123",
  pageUrl: "https://jobs.lever.co/acme/posting-123/apply",
};
const GREENHOUSE_TARGET = {
  actionUrl: GREENHOUSE_JOB.pageUrl,
  method: "post",
  enctype: "multipart/form-data",
  formTarget: "_self",
  providerJobKey: "greenhouse:acme:123",
  formIdentity: '[0,"application_form","","application-form","","123"]',
};
const LEVER_TARGET = {
  actionUrl: LEVER_JOB.pageUrl,
  method: "post",
  enctype: "multipart/form-data",
  formTarget: "_self",
  providerJobKey: "lever:jobs.lever.co:acme:posting-123",
  formIdentity: '[0,"application-form","","","application-form","posting-123"]',
};
const GREENHOUSE_FILES = [{
  fieldName: "resume",
  name: `resume-${RESUME_SHA}.pdf`,
  byteLength: 1_234,
  sha256: RESUME_SHA,
}];
const LEVER_FILES = [
  {
    fieldName: "resume",
    name: `resume-${RESUME_SHA}.pdf`,
    byteLength: 1_234,
    sha256: RESUME_SHA,
  },
  {
    fieldName: "coverLetter",
    name: `cover-letter-${COVER_SHA}.pdf`,
    byteLength: 2_345,
    sha256: COVER_SHA,
  },
];
const SUBMIT_FIELDS = [{
  fieldName: "job_id",
  valueByteLength: 3,
  valueSha256: "d".repeat(64),
}];
const GREENHOUSE_PART_ORDER = [
  { kind: "field" as const, index: 0 },
  { kind: "file" as const, index: 0 },
];
const LEVER_PART_ORDER = [
  { kind: "field" as const, index: 0 },
  { kind: "file" as const, index: 0 },
  { kind: "file" as const, index: 1 },
];
const SURFACE_VECTORS = (JSON.parse(readFileSync(
  new URL("./fixtures/final-submit-surface-vectors.json", import.meta.url),
  "utf8",
)) as {
  vectors: Array<{
    name: string;
    proof: FinalSubmitProof;
    expectedSurfaceSha256: string;
  }>;
}).vectors;

function autoAdmission(): NonNullable<ApplicationPacket["approvedExecutionAdmission"]> {
  return {
    kind: "track_auto_submit",
    authorization_id: "authorization-604",
    career_track_id: "track-604",
    revision_no: 3,
    authority_fingerprint: "1".repeat(64),
    ats_certification: {
      schema_version: 1,
      provider: "greenhouse",
      adapter_version: "2026.07.1-beta.1",
      variant_key: "public",
      layout_contract_version: 1,
      surface_sha256: finalSubmitSurfaceSha256({
        adapter: "greenhouse",
        adapterVersion: "2026.07.1-beta.1",
        control: "greenhouse_submit_application",
        target: GREENHOUSE_TARGET,
        files: GREENHOUSE_FILES,
        fields: SUBMIT_FIELDS,
        partOrder: GREENHOUSE_PART_ORDER,
      }),
      manifest_sha256: "2".repeat(64),
      activation_sha256: "3".repeat(64),
      activation_generation: 4,
      target_key_sha256: "5".repeat(64),
      layout_set_sha256: "6".repeat(64),
      adapter_bundle_sha256: "7".repeat(64),
      runner_target_sha256s: ["8".repeat(64), "9".repeat(64)],
      expires_at_ms: 9_007_199_254_740_000,
    },
  };
}

describe("final submit proof", () => {
  it.each(SURFACE_VECTORS)(
    "matches the shared Rust/TypeScript surface vector: $name",
    (vector) => {
      expect(finalSubmitSurfaceSha256(vector.proof)).toBe(vector.expectedSurfaceSha256);
    },
  );

  it("builds the exact Greenhouse wire proof without local document material", () => {
    const proof = createFinalSubmitProof({
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application",
      target: GREENHOUSE_TARGET,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
    }, {
      resume: { versionId: "resume-version-123", sha256: RESUME_SHA },
    }, GREENHOUSE_JOB);

    expect(proof).toEqual({
      schemaVersion: 3,
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application",
      target: GREENHOUSE_TARGET,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
      job: GREENHOUSE_JOB,
      documents: [{
        kind: "resume",
        versionId: "resume-version-123",
        sha256: RESUME_SHA,
      }],
    });
    expect(JSON.stringify(proof)).not.toMatch(/path|bytes|base64/i);
  });

  it("includes a materialized cover letter and sorts documents stably", () => {
    const proof = createFinalSubmitProof({
      adapter: "lever",
      adapterVersion: "2026.07.0-beta.1",
      control: "lever_application_submit",
      target: LEVER_TARGET,
      files: LEVER_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: LEVER_PART_ORDER,
    }, {
      resume: { versionId: "resume-version-456", sha256: RESUME_SHA },
      coverLetter: { sha256: COVER_SHA },
    }, LEVER_JOB);

    expect(proof.documents).toEqual([
      { kind: "cover_letter", sha256: COVER_SHA },
      { kind: "resume", versionId: "resume-version-456", sha256: RESUME_SHA },
    ]);
    expect(() => assertFinalSubmitProof(structuredClone(proof))).not.toThrow();
  });

  it("builds a strict schema-v4 proof only from a frozen certified Auto admission", () => {
    const providerProof = {
      adapter: "greenhouse" as const,
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application" as const,
      target: GREENHOUSE_TARGET,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
    };
    const proof = createFinalSubmitProof(providerProof, {
      resume: { versionId: "resume-version-604", sha256: RESUME_SHA },
    }, GREENHOUSE_JOB, autoAdmission());

    expect(proof.schemaVersion).toBe(4);
    if (proof.schemaVersion !== 4) throw new Error("expected certified proof");
    expect(proof.certification).toEqual({
      schemaVersion: 1,
      provider: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      manifestSha256: "2".repeat(64),
      activationSha256: "3".repeat(64),
      activationGeneration: 4,
      targetKeySha256: "5".repeat(64),
      layoutSetSha256: "6".repeat(64),
      adapterBundleSha256: "7".repeat(64),
      runnerTargetSha256s: ["8".repeat(64), "9".repeat(64)],
      expiresAtMs: 9_007_199_254_740_000,
    });
    expect(proof.observedSurface).toEqual({
      schemaVersion: 1,
      variantKey: "public",
      layoutContractVersion: 1,
      surfaceSha256: finalSubmitSurfaceSha256(providerProof),
    });
    expect(() => assertFinalSubmitProof(structuredClone(proof))).not.toThrow();
    expect(JSON.stringify(proof)).not.toMatch(/authorization-604|track-604|candidate value/i);
  });

  it("rejects missing or mismatched certification on an Auto proof", () => {
    const providerProof = {
      adapter: "greenhouse" as const,
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application" as const,
      target: GREENHOUSE_TARGET,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
    };
    const admission = autoAdmission();
    if (admission.kind !== "track_auto_submit") throw new Error("expected Auto admission");
    delete admission.ats_certification;
    expect(() => createFinalSubmitProof(providerProof, {
      resume: { versionId: "resume-version-604", sha256: RESUME_SHA },
    }, GREENHOUSE_JOB, admission)).toThrow(FinalSubmitProofError);

    const wrongProvider = autoAdmission();
    if (wrongProvider.kind !== "track_auto_submit" || !wrongProvider.ats_certification) {
      throw new Error("expected certified Auto admission");
    }
    wrongProvider.ats_certification.provider = "lever";
    expect(() => createFinalSubmitProof(providerProof, {
      resume: { versionId: "resume-version-604", sha256: RESUME_SHA },
    }, GREENHOUSE_JOB, wrongProvider)).toThrow(FinalSubmitProofError);
  });

  it.each([
    ["certification digest", (proof: Record<string, unknown>) => {
      (proof.certification as Record<string, unknown>).manifestSha256 = "F".repeat(64);
    }],
    ["observed surface", (proof: Record<string, unknown>) => {
      (proof.observedSurface as Record<string, unknown>).surfaceSha256 = "f".repeat(64);
    }],
    ["unknown certification field", (proof: Record<string, unknown>) => {
      (proof.certification as Record<string, unknown>).credential = "forbidden";
    }],
  ])("rejects a received schema-v4 proof with a mutated %s", (_label, mutate) => {
    const proof = createFinalSubmitProof({
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application",
      target: GREENHOUSE_TARGET,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
    }, {
      resume: { versionId: "resume-version-604", sha256: RESUME_SHA },
    }, GREENHOUSE_JOB, autoAdmission()) as unknown as Record<string, unknown>;
    mutate(proof);

    expect(() => assertFinalSubmitProof(proof)).toThrow(FinalSubmitProofError);
  });

  it.each([
    ["missing provider proof", undefined],
    ["generic adapter", {
      adapter: "semantic",
      adapterVersion: "2026.07.1-handoff",
      control: "semantic_submit",
    }],
    ["wrong provider version", {
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.2",
      control: "greenhouse_submit_application",
      target: GREENHOUSE_TARGET,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
    }],
    ["wrong provider control", {
      adapter: "lever",
      adapterVersion: "2026.07.0-beta.1",
      control: "greenhouse_submit_application",
      target: LEVER_TARGET,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
    }],
  ])("rejects %s", (_label, providerProof) => {
    expect(() => createFinalSubmitProof(providerProof as never, {
      resume: { versionId: "resume-version-123", sha256: RESUME_SHA },
    }, GREENHOUSE_JOB)).toThrow(FinalSubmitProofError);
  });

  it.each([
    ["uppercase hash", { resume: { versionId: "resume-version-123", sha256: "A".repeat(64) } }],
    ["missing resume", {}],
    ["blank version", { resume: { versionId: " ", sha256: RESUME_SHA } }],
    ["invalid cover hash", {
      resume: { versionId: "resume-version-123", sha256: RESUME_SHA },
      coverLetter: { sha256: "not-a-hash" },
    }],
  ])("rejects %s before a server request", (_label, documents) => {
    expect(() => createFinalSubmitProof({
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application",
      target: GREENHOUSE_TARGET,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
    }, documents as never, GREENHOUSE_JOB)).toThrow(FinalSubmitProofError);
  });

  it.each([
    ["another Greenhouse job", {
      approvedCanonicalUrl: GREENHOUSE_JOB.approvedCanonicalUrl,
      pageUrl: "https://boards.greenhouse.io/acme/jobs/456",
    }],
    ["another Greenhouse tenant", {
      approvedCanonicalUrl: GREENHOUSE_JOB.approvedCanonicalUrl,
      pageUrl: "https://boards.greenhouse.io/other/jobs/123",
    }],
    ["ambiguous Greenhouse token", {
      approvedCanonicalUrl: GREENHOUSE_JOB.approvedCanonicalUrl,
      pageUrl: "https://boards.greenhouse.io/acme/jobs/123?gh_jid=456",
    }],
  ])("rejects %s before final-submit authority", (_label, job) => {
    expect(() => createFinalSubmitProof({
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application",
      target: GREENHOUSE_TARGET,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
    }, {
      resume: { versionId: "resume-version-123", sha256: RESUME_SHA },
    }, job)).toThrow(FinalSubmitProofError);
  });

  it("rejects another Lever posting on the same shared provider host", () => {
    expect(() => createFinalSubmitProof({
      adapter: "lever",
      adapterVersion: "2026.07.0-beta.1",
      control: "lever_application_submit",
      target: LEVER_TARGET,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
    }, {
      resume: { versionId: "resume-version-123", sha256: RESUME_SHA },
    }, {
      approvedCanonicalUrl: LEVER_JOB.approvedCanonicalUrl,
      pageUrl: "https://jobs.lever.co/acme/posting-456/apply",
    })).toThrow(FinalSubmitProofError);
  });

  it.each([
    ["another provider job", {
      ...GREENHOUSE_TARGET,
      actionUrl: "https://job-boards.greenhouse.io/acme/jobs/456",
      providerJobKey: "greenhouse:acme:456",
    }],
    ["a non-POST method", { ...GREENHOUSE_TARGET, method: "get" }],
    ["a non-multipart encoding", {
      ...GREENHOUSE_TARGET,
      enctype: "application/x-www-form-urlencoded",
    }],
    ["a mismatched provider key", {
      ...GREENHOUSE_TARGET,
      providerJobKey: "greenhouse:acme:456",
    }],
    ["a different origin", {
      ...GREENHOUSE_TARGET,
      actionUrl: "https://boards.greenhouse.io/acme/jobs/123",
    }],
    ["an empty form identity", { ...GREENHOUSE_TARGET, formIdentity: "" }],
    ["an extra target field", { ...GREENHOUSE_TARGET, extra: true }],
  ])("rejects a final-submit target with %s", (_label, target) => {
    expect(() => createFinalSubmitProof({
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application",
      target: target as never,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
    }, {
      resume: { versionId: "resume-version-123", sha256: RESUME_SHA },
    }, GREENHOUSE_JOB)).toThrow(FinalSubmitProofError);
  });

  it.each([
    ["no outgoing file evidence", []],
    ["a different resume than the materialized proof", [{
      ...GREENHOUSE_FILES[0],
      name: `resume-${"c".repeat(64)}.pdf`,
      sha256: "c".repeat(64),
    }]],
    ["an unproved extra cover letter", [
      ...GREENHOUSE_FILES,
      {
        fieldName: "coverLetter",
        name: `cover-letter-${COVER_SHA}.pdf`,
        byteLength: 2_345,
        sha256: COVER_SHA,
      },
    ]],
    ["a filename/hash mismatch", [{
      ...GREENHOUSE_FILES[0],
      sha256: "c".repeat(64),
    }]],
    ["an invalid byte length", [{ ...GREENHOUSE_FILES[0], byteLength: 0 }]],
    ["an unknown file field", [{ ...GREENHOUSE_FILES[0], extra: true }]],
  ])("rejects %s", (_label, files) => {
    expect(() => createFinalSubmitProof({
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application",
      target: GREENHOUSE_TARGET,
      files: files as never,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
    }, {
      resume: { versionId: "resume-version-123", sha256: RESUME_SHA },
    }, GREENHOUSE_JOB)).toThrow(FinalSubmitProofError);
  });

  it.each([
    ["missing hashed field evidence", []],
    ["an invalid field name", [{ ...SUBMIT_FIELDS[0], fieldName: "job\u0000id" }]],
    ["an oversized field value", [{ ...SUBMIT_FIELDS[0], valueByteLength: 65_537 }]],
    ["an uppercase field hash", [{ ...SUBMIT_FIELDS[0], valueSha256: "D".repeat(64) }]],
    ["an unknown field-evidence key", [{ ...SUBMIT_FIELDS[0], value: "123" }]],
    ["oversized aggregate field evidence", Array.from({ length: 9 }, (_, index) => ({
      fieldName: `field_${index}`,
      valueByteLength: 64 * 1_024,
      valueSha256: `${index}`.repeat(64),
    }))],
  ])("rejects %s", (_label, fields) => {
    expect(() => createFinalSubmitProof({
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application",
      target: GREENHOUSE_TARGET,
      files: GREENHOUSE_FILES,
      fields: fields as never,
      partOrder: GREENHOUSE_PART_ORDER,
    }, {
      resume: { versionId: "resume-version-123", sha256: RESUME_SHA },
    }, GREENHOUSE_JOB)).toThrow(FinalSubmitProofError);
  });

  it("rejects unknown fields in a received schema-v3 proof", () => {
    const proof = createFinalSubmitProof({
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application",
      target: GREENHOUSE_TARGET,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: GREENHOUSE_PART_ORDER,
    }, {
      resume: { versionId: "resume-version-123", sha256: RESUME_SHA },
    }, GREENHOUSE_JOB) as unknown as Record<string, unknown>;
    proof.extra = true;

    expect(() => assertFinalSubmitProof(proof)).toThrow(FinalSubmitProofError);
  });

  it.each([
    [[], "missing order"],
    [[{ kind: "field", index: 0 }, { kind: "field", index: 0 }], "duplicate index"],
    [[{ kind: "field", index: 0 }, { kind: "file", index: 1 }], "out-of-range index"],
  ])("rejects a non-permutation multipart order: %s (%s)", (partOrder) => {
    expect(() => createFinalSubmitProof({
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application",
      target: GREENHOUSE_TARGET,
      files: GREENHOUSE_FILES,
      fields: SUBMIT_FIELDS,
      partOrder: partOrder as never,
    }, {
      resume: { versionId: "resume-version-123", sha256: RESUME_SHA },
    }, GREENHOUSE_JOB)).toThrow(FinalSubmitProofError);
  });

  it("rejects field/file overlap and field names Rust cannot accept", () => {
    for (const fieldName of ["resume", "candidate name"]) {
      expect(() => createFinalSubmitProof({
        adapter: "greenhouse",
        adapterVersion: "2026.07.1-beta.1",
        control: "greenhouse_submit_application",
        target: GREENHOUSE_TARGET,
        files: GREENHOUSE_FILES,
        fields: [{ ...SUBMIT_FIELDS[0]!, fieldName }],
        partOrder: GREENHOUSE_PART_ORDER,
      }, {
        resume: { versionId: "resume-version-123", sha256: RESUME_SHA },
      }, GREENHOUSE_JOB)).toThrow(FinalSubmitProofError);
    }
  });
});

describe("certified provider initial navigation", () => {
  it("accepts exact Greenhouse and Lever application variants", () => {
    expect(() => assertCertifiedProviderNavigationJob(
      GREENHOUSE_JOB.pageUrl,
      { source: "greenhouse", canonicalUrl: GREENHOUSE_JOB.approvedCanonicalUrl },
    )).not.toThrow();
    expect(() => assertCertifiedProviderNavigationJob(
      LEVER_JOB.pageUrl,
      { source: "lever", canonicalUrl: LEVER_JOB.approvedCanonicalUrl },
    )).not.toThrow();
  });

  it("binds every routing-looking query alias to the provider job", () => {
    expect(certifiedProviderJobKey(
      "greenhouse",
      "https://boards.greenhouse.io/acme/jobs/123?Job_ID=123&POSTINGID=123",
    )).toBe("greenhouse:acme:123");
    expect(certifiedProviderJobKey(
      "lever",
      "https://jobs.lever.co/acme/posting-123/apply?jobId=posting-123&LEVER_JOB_ID=posting-123",
    )).toBe("lever:jobs.lever.co:acme:posting-123");
    expect(() => certifiedProviderJobKey(
      "greenhouse",
      "https://boards.greenhouse.io/acme/jobs/123?job_id=456",
    )).toThrow();
    expect(() => certifiedProviderJobKey(
      "lever",
      "https://jobs.lever.co/acme/posting-123/apply?posting_id=posting-456",
    )).toThrow();
  });

  it("requires explicit provider confirmation paths", () => {
    expect(() => certifiedProviderJobKey(
      "greenhouse",
      GREENHOUSE_JOB.approvedCanonicalUrl,
      "confirmation",
    )).toThrow();
    expect(() => certifiedProviderJobKey(
      "lever",
      LEVER_JOB.pageUrl,
      "confirmation",
    )).toThrow();
    expect(certifiedProviderJobKey(
      "greenhouse",
      `${GREENHOUSE_JOB.approvedCanonicalUrl}/confirmation`,
      "confirmation",
    )).toBe("greenhouse:acme:123");
    expect(certifiedProviderJobKey(
      "lever",
      `${LEVER_JOB.approvedCanonicalUrl}/confirmation`,
      "confirmation",
    )).toBe("lever:jobs.lever.co:acme:posting-123");
  });

  it.each([
    ["another job", "https://boards.greenhouse.io/acme/jobs/456", "greenhouse"],
    ["another provider", LEVER_JOB.pageUrl, "greenhouse"],
    ["a non-provider redirect", "https://careers.acme.test/apply", "greenhouse"],
    ["a source mismatch", GREENHOUSE_JOB.pageUrl, "semantic"],
    ["an ambiguous official path", "https://boards.greenhouse.io/acme/jobs/123/extra", "greenhouse"],
  ] as const)("rejects %s before provider interaction", (_label, url, source) => {
    expect(() => assertCertifiedProviderNavigationJob(url, {
      source,
      canonicalUrl: GREENHOUSE_JOB.approvedCanonicalUrl,
    })).toThrow();
  });
});
