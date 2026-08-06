import { describe, expect, it } from "vitest";
import { parseBlueyJobsProtocol } from "../src/protocol.js";
import { localRunCapabilityFixture } from "./fixtures/local-run-capability.js";

describe("Bluey Browser protocol", () => {
  it("parses application-scoped run tickets", () => {
    const ticket = "a".repeat(64);
    expect(parseBlueyJobsProtocol(`bluey-jobs://run/run-123?ticket=${ticket}`)).toEqual({
      action: "run",
      runId: "run-123",
      ticket,
    });
  });

  it("parses only live run-scoped resume capabilities", () => {
    const capability = resumeCapability();
    expect(parseBlueyJobsProtocol(
      `bluey-jobs://resume/run-123?capability=${encodeURIComponent(capability)}`,
      1_000,
    )).toEqual({ action: "resume", runId: "run-123", capability });
  });

  it("allows the non-sensitive open action", () => {
    expect(parseBlueyJobsProtocol("bluey-jobs://open")).toEqual({ action: "open" });
  });

  it("rejects missing or malformed capabilities", () => {
    expect(() => parseBlueyJobsProtocol("bluey-jobs://takeover?session_id=run-123")).toThrow();
    expect(() => parseBlueyJobsProtocol("bluey-jobs://run/run-123?ticket=short")).toThrow();
    expect(() => parseBlueyJobsProtocol(`https://bluey.sh/run/run-123?ticket=${"a".repeat(64)}`)).toThrow();
    expect(() => parseBlueyJobsProtocol(
      `bluey-jobs://resume/run-123?ticket=${"a".repeat(64)}`,
      1_000,
    )).toThrow();
    expect(() => parseBlueyJobsProtocol(
      `bluey-jobs://resume/run-other?capability=${encodeURIComponent(resumeCapability())}`,
      1_000,
    )).toThrow();
    expect(() => parseBlueyJobsProtocol(
      `bluey-jobs://resume/run-123?capability=${encodeURIComponent(resumeCapability(1_000))}`,
      1_000,
    )).toThrow();
  });
});

function resumeCapability(expiresAtMs = 2_000): string {
  return localRunCapabilityFixture("resume", expiresAtMs);
}
