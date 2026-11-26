import { describe, expect, it } from "vitest";
import {
  providerOptionsForResumeAction,
  providerRegistryForExecution,
} from "../src/resume-policy.js";

describe("runner final-review resolution", () => {
  it("approves Greenhouse and Lever only for the exact resumed action", async () => {
    for (const action of [undefined, "approve", "submit", "approve_submission ", "APPROVE_SUBMISSION"]) {
      expect(providerOptionsForResumeAction(action)).toBeUndefined();
    }

    const options = providerOptionsForResumeAction("approve_submission");
    expect(options).toBeDefined();
    expect(await options?.greenhouse?.finalReviewApproval?.({} as never)).toBe(true);
    expect(await options?.lever?.finalReviewApproval?.({} as never)).toBe(true);
  });

  it("bypasses provider review only for a frozen certified Auto admission", () => {
    expect(providerRegistryForExecution({}, undefined)).toBeUndefined();
    expect(providerRegistryForExecution({
      approvedExecutionSchemaVersion: 2,
      approvedExecutionAdmission: {
        kind: "track_auto_submit",
        authorization_id: "authorization-1",
        career_track_id: "track-1",
        revision_no: 1,
        authority_fingerprint: "a".repeat(64),
      },
    }, undefined)).toBeUndefined();

    expect(providerRegistryForExecution({
      approvedExecutionSchemaVersion: 3,
      approvedExecutionAdmission: {
        kind: "track_auto_submit",
        authorization_id: "authorization-1",
        career_track_id: "track-1",
        revision_no: 1,
        authority_fingerprint: "a".repeat(64),
        ats_certification: {
          schema_version: 1,
          provider: "greenhouse",
          adapter_version: "2026.07.1-beta.1",
          manifest_sha256: "1".repeat(64),
          activation_sha256: "2".repeat(64),
          activation_generation: 1,
          target_key_sha256: "3".repeat(64),
          layout_set_sha256: "4".repeat(64),
          adapter_bundle_sha256: "5".repeat(64),
          runner_target_sha256s: ["6".repeat(64)],
          expires_at_ms: Date.now() + 60_000,
        },
      },
    }, undefined)).toBeDefined();
  });
});
