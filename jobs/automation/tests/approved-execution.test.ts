import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  ApprovedExecutionIntegrityError,
  approvedExecutionChecksum,
  assertApprovedExecutionChecksum,
  cloneApprovedPacketForRuntime,
  createApprovedExecutionSnapshot,
  type ApplicationPacket,
  type NormalizedJob,
} from "../src/index.js";

interface ApprovedExecutionVector {
  name: string;
  schemaVersion: 1 | 2;
  admission?: ApplicationPacket["approvedExecutionAdmission"];
  packet: Omit<
    ApplicationPacket,
    "approvedPacketChecksum" | "approvedExecutionSchemaVersion" | "approvedExecutionAdmission"
  >;
  job: NormalizedJob;
  checksum: string;
}

const vectors = (JSON.parse(readFileSync(
  new URL("./fixtures/approved-execution-vectors.json", import.meta.url),
  "utf8",
)) as { vectors: ApprovedExecutionVector[] }).vectors;

const job: NormalizedJob = {
  externalId: "job-1",
  canonicalUrl: "https://jobs.acme.test/job-1",
  company: "Acme",
  title: "Engineer",
  location: "Remote",
  workplace: "remote",
  description: "Build useful things.",
  source: "greenhouse",
};

function approvedPacket(): ApplicationPacket {
  const packet: ApplicationPacket = {
    applicationId: "application-1",
    jobId: "job-1",
    resumeVersionId: "resume-1",
    approvedPacketChecksum: "",
    answers: { email: "ada@example.com", sponsorship: "No" },
    verifiedClaimIds: ["claim-2", "claim-1"],
  };
  packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
  return packet;
}

function approvedPacketV2(
  admission: ApplicationPacket["approvedExecutionAdmission"] = { kind: "review_approval" },
): ApplicationPacket {
  const packet: ApplicationPacket = {
    applicationId: "application-1",
    jobId: "job-1",
    resumeVersionId: "resume-1",
    approvedPacketChecksum: "",
    approvedExecutionSchemaVersion: 2,
    approvedExecutionAdmission: admission,
    answers: { email: "ada@example.com", sponsorship: "No" },
    verifiedClaimIds: ["claim-2", "claim-1"],
  };
  packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
  return packet;
}

describe("approved execution snapshots", () => {
  it.each(vectors)("matches the shared Rust/TypeScript vector: $name", (vector) => {
    const packet: ApplicationPacket = {
      ...structuredClone(vector.packet),
      approvedPacketChecksum: vector.checksum,
      approvedExecutionSchemaVersion: vector.schemaVersion,
      ...(vector.admission ? {
        approvedExecutionAdmission: structuredClone(vector.admission),
      } : {}),
    };
    expect(approvedExecutionChecksum(packet, vector.job)).toBe(vector.checksum);
    expect(assertApprovedExecutionChecksum(packet, vector.job)).toBe(vector.checksum);
  });

  it("matches the server canonical checksum contract", () => {
    const packet = approvedPacket();
    expect(packet.approvedPacketChecksum).toBe(
      "558834e9f04f81657522eca710b10d988535188f0cb3d6daba37b7e839bbff8a",
    );
    expect(assertApprovedExecutionChecksum(packet, job)).toBe(packet.approvedPacketChecksum);
  });

  it("matches the server v2 review-approval checksum contract", () => {
    const packet = approvedPacketV2();
    expect(packet.approvedPacketChecksum).toBe(
      "a27ae7ce7e97f13a1190374f88a556afd8812dc1bf2b1c318f0de2894d5f7e59",
    );
    expect(assertApprovedExecutionChecksum(packet, job)).toBe(packet.approvedPacketChecksum);
  });

  it("matches the server v2 Track Auto-submit checksum contract", () => {
    const packet = approvedPacketV2({
      kind: "track_auto_submit",
      authorization_id: "authorization-1",
      career_track_id: "track-1",
      revision_no: 3,
      authority_fingerprint: "a".repeat(64),
    });
    expect(packet.approvedPacketChecksum).toBe(
      "e1b2e1dc90a86ebee87701c7279baae571ed423485acb85f6fe9b4ec4e816c4e",
    );
    expect(assertApprovedExecutionChecksum(packet, job)).toBe(packet.approvedPacketChecksum);
  });

  it("freezes a detached approval while runtime materialization stays separate", () => {
    const packet = approvedPacket();
    const snapshot = createApprovedExecutionSnapshot(packet, job);
    const runtimePacket = cloneApprovedPacketForRuntime(snapshot.approvedPacket);

    runtimePacket.resumePath = "/runtime/resume.pdf";
    runtimePacket.answers.email = "runtime@example.com";

    expect(snapshot.approvedPacket.resumePath).toBeUndefined();
    expect(snapshot.approvedPacket.answers.email).toBe("ada@example.com");
    expect(Object.isFrozen(snapshot.approvedPacket.answers)).toBe(true);
    expect(() => {
      snapshot.approvedPacket.answers.email = "changed@example.com";
    }).toThrow(TypeError);
  });

  it("rejects answer and job changes after approval", () => {
    const packet = approvedPacket();
    const changedAnswer = structuredClone(packet);
    changedAnswer.answers.sponsorship = "Yes";
    expect(() => assertApprovedExecutionChecksum(changedAnswer, job))
      .toThrow(ApprovedExecutionIntegrityError);

    expect(() => assertApprovedExecutionChecksum(packet, { ...job, title: "Staff Engineer" }))
      .toThrow(ApprovedExecutionIntegrityError);
  });

  it("rejects missing, changed, or malformed v2 admission metadata", () => {
    const packet = approvedPacketV2();

    const missingSchema = structuredClone(packet);
    delete missingSchema.approvedExecutionSchemaVersion;
    expect(() => assertApprovedExecutionChecksum(missingSchema, job))
      .toThrow(ApprovedExecutionIntegrityError);

    const missingAdmission = structuredClone(packet);
    delete missingAdmission.approvedExecutionAdmission;
    expect(() => assertApprovedExecutionChecksum(missingAdmission, job))
      .toThrow(ApprovedExecutionIntegrityError);

    const changedAdmission = structuredClone(packet);
    changedAdmission.approvedExecutionAdmission = {
      kind: "track_auto_submit",
      authorization_id: "authorization-1",
      career_track_id: "track-1",
      revision_no: 1,
      authority_fingerprint: "a".repeat(64),
    };
    expect(() => assertApprovedExecutionChecksum(changedAdmission, job))
      .toThrow(ApprovedExecutionIntegrityError);

    const extraAdmissionField = structuredClone(packet) as ApplicationPacket & {
      approvedExecutionAdmission: Record<string, unknown>;
    };
    extraAdmissionField.approvedExecutionAdmission.extra = true;
    expect(() => approvedExecutionChecksum(extraAdmissionField, job))
      .toThrow(ApprovedExecutionIntegrityError);
  });

  it.each([
    ["NaN", Number.NaN],
    ["positive infinity", Number.POSITIVE_INFINITY],
    ["negative zero", -0],
    ["an unsafe integer", Number.MAX_SAFE_INTEGER + 1],
    ["undefined", undefined],
    ["a non-JSON object", new Date("2026-08-05T00:00:00.000Z")],
  ])("rejects %s before JSON cloning can change its meaning", (_name, value) => {
    const packet = approvedPacket();
    (packet.answers as Record<string, unknown>).unsafe = value;

    expect(() => approvedExecutionChecksum(packet, job))
      .toThrow(ApprovedExecutionIntegrityError);
  });

  it("rejects undefined array entries and unpaired Unicode surrogates", () => {
    const undefinedEntry = approvedPacket();
    (undefinedEntry.verifiedClaimIds as unknown[]).push(undefined);
    expect(() => approvedExecutionChecksum(undefinedEntry, job))
      .toThrow(ApprovedExecutionIntegrityError);

    const unpairedSurrogate = approvedPacket();
    unpairedSurrogate.answers.note = "invalid-\ud800";
    expect(() => approvedExecutionChecksum(unpairedSurrogate, job))
      .toThrow(ApprovedExecutionIntegrityError);
  });
});
