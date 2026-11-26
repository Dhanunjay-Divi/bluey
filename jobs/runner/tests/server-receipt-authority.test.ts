import { describe, expect, it } from "vitest";
import { submissionReceiptAuthority } from "../src/server.js";

const LEASE_TOKEN = "a".repeat(43);

describe("submitted receipt authority", () => {
  it("returns only the exact lease token and fence after an authorized submit", () => {
    const authority = submissionReceiptAuthority("submitted", lease());

    expect(authority).toEqual({ leaseToken: LEASE_TOKEN, fence: 7 });
    expect(authority).not.toHaveProperty("ownerId");
  });

  it.each(["failed", "needs_input"] as const)(
    "does not expose the lease capability for a %s result",
    (status) => {
      expect(submissionReceiptAuthority(status, lease())).toBeUndefined();
    },
  );

  it("accepts confirmed evidence after an activation-uncertain browser response", () => {
    expect(
      submissionReceiptAuthority(
        "submitted",
        lease({
          finalSubmitActivationOutcome: "activation_uncertain",
        }),
      ),
    ).toEqual({ leaseToken: LEASE_TOKEN, fence: 7 });
  });

  it.each([
    { finalSubmitAttempted: false },
    { finalSubmitAuthorized: false },
    { finalSubmitActivationOutcome: undefined },
    { leaseToken: "short" },
    { fence: 0 },
    { fence: Number.MAX_SAFE_INTEGER + 1 },
  ])(
    "rejects submitted evidence without complete fenced authority: %j",
    (override) => {
      expect(() =>
        submissionReceiptAuthority("submitted", lease(override)),
      ).toThrowError(/Execution lease irreversible failed \(invalid_state\)/);
    },
  );
});

function lease(
  override: {
    finalSubmitActivationOutcome?: "activated" | "activation_uncertain";
    finalSubmitAttempted?: boolean;
    finalSubmitAuthorized?: boolean;
    leaseToken?: string;
    fence?: number;
  } = {},
) {
  const finalSubmitActivationOutcome = Object.prototype.hasOwnProperty.call(
    override,
    "finalSubmitActivationOutcome",
  )
    ? override.finalSubmitActivationOutcome
    : "activated";
  const finalSubmitAttempted = override.finalSubmitAttempted ?? true;
  const finalSubmitAuthorized = override.finalSubmitAuthorized ?? true;
  const leaseToken = override.leaseToken ?? LEASE_TOKEN;
  const fence = override.fence ?? 7;
  return {
    finalSubmitActivationOutcome,
    finalSubmitAttempted,
    finalSubmitAuthorized,
    checkpointMetadata: () => ({
      leaseToken,
      fence,
      expiresAtMs: Date.now() + 60_000,
      ownerId: "runner-one",
    }),
  };
}
