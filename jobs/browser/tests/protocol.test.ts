import { describe, expect, it } from "vitest";
import { parseBlueyJobsProtocol } from "../src/protocol.js";

describe("Bluey Browser protocol", () => {
  it("parses application-scoped run tickets", () => {
    const ticket = "a".repeat(64);
    expect(parseBlueyJobsProtocol(`bluey-jobs://run/run-123?ticket=${ticket}`)).toEqual({
      action: "run",
      runId: "run-123",
      ticket,
    });
  });

  it("allows the non-sensitive open action", () => {
    expect(parseBlueyJobsProtocol("bluey-jobs://open")).toEqual({ action: "open" });
  });

  it("rejects missing or malformed capabilities", () => {
    expect(() => parseBlueyJobsProtocol("bluey-jobs://run/run-123?ticket=short")).toThrow();
    expect(() => parseBlueyJobsProtocol(`https://bluey.sh/run/run-123?ticket=${"a".repeat(64)}`)).toThrow();
  });
});
