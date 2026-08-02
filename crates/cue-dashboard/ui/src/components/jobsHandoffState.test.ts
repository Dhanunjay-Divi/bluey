import { describe, expect, it } from "vitest";
import {
  INITIAL_JOBS_HANDOFF_STATE,
  jobsHandoffReducer,
  jobsHandoffTargetLabel,
  normalizeJobsHandoffPayload,
  safeJobsHandoffRetryError,
  type JobsHandoffImportPayload,
} from "./jobsHandoffState";

const success: JobsHandoffImportPayload = {
  result_id: "a".repeat(32),
  completed_at_ms: 1_752_345_678_000,
  success: true,
  application_id: "application-1",
  role: "Staff Engineer",
  company: "Acme",
  recovered: false,
  error: null,
};

describe("jobs handoff event state", () => {
  it("publishes successful imports and increments the reload revision", () => {
    const next = jobsHandoffReducer(INITIAL_JOBS_HANDOFF_STATE, {
      type: "received",
      id: 1,
      payload: success,
    });
    expect(next.notice).toEqual({ id: 1, payload: success });
    expect(next.successful_import_revision).toBe(1);
  });

  it("does not force a profile reload for failed imports", () => {
    const failed = jobsHandoffReducer(INITIAL_JOBS_HANDOFF_STATE, {
      type: "received",
      id: 1,
      payload: { ...success, success: false, error: "Nonce expired." },
    });
    expect(failed.notice?.payload.error).toBe("Nonce expired.");
    expect(failed.successful_import_revision).toBe(0);
  });

  it("dismisses the banner without losing the successful-import revision", () => {
    const received = jobsHandoffReducer(INITIAL_JOBS_HANDOFF_STATE, {
      type: "received",
      id: 1,
      payload: success,
    });
    const dismissed = jobsHandoffReducer(received, { type: "dismissed" });
    expect(dismissed.notice).toBeNull();
    expect(dismissed.successful_import_revision).toBe(1);
  });

  it("increments once for each later successful event, including recovery", () => {
    const first = jobsHandoffReducer(INITIAL_JOBS_HANDOFF_STATE, {
      type: "received",
      id: 1,
      payload: success,
    });
    const second = jobsHandoffReducer(first, {
      type: "received",
      id: 2,
      payload: { ...success, result_id: "b".repeat(32), recovered: true },
    });
    expect(second.successful_import_revision).toBe(2);
    expect(second.notice?.payload.recovered).toBe(true);
  });
});

describe("jobs handoff event normalization", () => {
  it("keeps renderer-safe backend errors verbatim", () => {
    const payload = normalizeJobsHandoffPayload({
      result_id: "c".repeat(32),
      completed_at_ms: 1_752_345_678_001,
      success: false,
      application_id: null,
      role: null,
      company: null,
      recovered: false,
      error: "This handoff has already been used. Try again.",
    });
    expect(payload.error).toBe("This handoff has already been used. Try again.");
  });

  it("normalizes optional target labels and rejects unreadable events", () => {
    const payload = normalizeJobsHandoffPayload({ ...success, role: " Staff Engineer ", company: " Acme " });
    expect(jobsHandoffTargetLabel(payload)).toBe("Staff Engineer at Acme");

    const invalid = normalizeJobsHandoffPayload({ recovered: true });
    expect(invalid).toMatchObject({ success: false, recovered: false });
    expect(invalid.error).toMatch(/unreadable/);
  });

  it("renders retry failures without controls, unbounded text, or object dumps", () => {
    expect(safeJobsHandoffRetryError(" Finish\n the current session\u0000, then retry. ")).toBe(
      "Finish the current session, then retry.",
    );
    expect(Array.from(safeJobsHandoffRetryError("x".repeat(700))).length).toBe(500);
    expect(safeJobsHandoffRetryError({ token: "do-not-render" })).not.toContain("token");
  });
});
