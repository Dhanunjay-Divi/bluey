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

describe("approved execution snapshots", () => {
  it("matches the server canonical checksum contract", () => {
    const packet = approvedPacket();
    expect(packet.approvedPacketChecksum).toBe(
      "558834e9f04f81657522eca710b10d988535188f0cb3d6daba37b7e839bbff8a",
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
});
