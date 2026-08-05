import { describe, expect, it } from "vitest";
import {
  approvedExecutionChecksum,
  type ApplicationPacket,
  type NormalizedJob,
} from "@bluey/jobs-automation";
import {
  validateStartRunRequest,
  type StartRunRequest,
} from "../src/local-run-contracts.js";

describe("local run approved job navigation", () => {
  it("accepts only the exact certified provider job before navigation", () => {
    const request = fixture();
    expect(() => validateStartRunRequest(request)).not.toThrow();

    expect(() => validateStartRunRequest({
      ...request,
      url: "https://boards.greenhouse.io/acme/jobs/456",
    })).toThrow(expect.objectContaining({ code: "launch_mismatch" }));
  });

  it("rejects a certified URL whose approved source is inconsistent", () => {
    const request = fixture();
    const job = { ...request.job!, source: "semantic" as const };
    const packet = packetFor(job);

    expect(() => validateStartRunRequest({ ...request, packet, job }))
      .toThrow(expect.objectContaining({ code: "launch_mismatch" }));
  });
});

function fixture(): StartRunRequest {
  const job: NormalizedJob = {
    externalId: "123",
    canonicalUrl: "https://boards.greenhouse.io/acme/jobs/123",
    company: "Acme",
    title: "Engineer",
    location: "Remote",
    workplace: "remote",
    description: "Build reliable systems.",
    source: "greenhouse",
  };
  return {
    accountId: "account-123",
    applicationIdentityId: "identity-123",
    runId: "run-123",
    applicationId: "application-123",
    browserProfileId: "profile-123",
    url: "https://job-boards.greenhouse.io/embed/job_app?for=acme&token=123",
    packet: packetFor(job),
    job,
  };
}

function packetFor(job: NormalizedJob): ApplicationPacket {
  const packet: ApplicationPacket = {
    applicationId: "application-123",
    jobId: "job-123",
    resumeVersionId: "resume-123",
    approvedPacketChecksum: "",
    answers: {},
    verifiedClaimIds: [],
    applicationIdentityId: "identity-123",
  };
  packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
  return packet;
}
