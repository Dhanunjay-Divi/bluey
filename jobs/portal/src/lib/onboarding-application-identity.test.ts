import { describe, expect, it } from "vitest";
import type { ApplicationIdentity } from "../types";
import { previewWorkspace, previewWorkspaceForScenario } from "../data/preview";
import {
  resolveOnboardingApplicationIdentity,
  validateSelectedOnboardingApplicationIdentity,
} from "./onboarding-application-identity";

const identity = (overrides: Partial<ApplicationIdentity> = {}): ApplicationIdentity => ({
  id: "identity-default",
  email: "verified@example.test",
  label: "Verified application email",
  verification_status: "verified",
  is_default: true,
  created_at_ms: 1,
  updated_at_ms: 1,
  ...overrides,
});

describe("onboarding application identity selection", () => {
  it("uses the returned verified default identity", () => {
    expect(resolveOnboardingApplicationIdentity([identity()])).toMatchObject({
      kind: "selected",
      identity: { id: "identity-default" },
    });
  });

  it("uses the sole verified identity when no default exists", () => {
    expect(resolveOnboardingApplicationIdentity([identity({ is_default: false })])).toMatchObject({
      kind: "selected",
      identity: { id: "identity-default" },
    });
  });

  it("retains an existing binding only when it is currently verified", () => {
    const identities = [identity(), identity({ id: "identity-career", is_default: false })];
    expect(resolveOnboardingApplicationIdentity(identities, "identity-career")).toMatchObject({
      kind: "selected",
      identity: { id: "identity-career" },
    });
  });

  it("fails closed for no, unverified, injected, and stale identities", () => {
    expect(resolveOnboardingApplicationIdentity([])).toMatchObject({
      kind: "unavailable",
      issue: "no_verified_identity",
    });
    expect(resolveOnboardingApplicationIdentity([identity({ verification_status: "pending" })])).toMatchObject({
      kind: "unavailable",
      issue: "no_verified_identity",
    });
    expect(resolveOnboardingApplicationIdentity(
      [identity(), { ...identity({ id: "identity-injected" }), id: "" } as ApplicationIdentity],
    )).toMatchObject({ kind: "unavailable", issue: "identity_data_invalid" });
    expect(resolveOnboardingApplicationIdentity([identity()], "identity-removed")).toMatchObject({
      kind: "unavailable",
      issue: "stale_track_identity",
    });
    expect(resolveOnboardingApplicationIdentity([
      identity(),
      identity({ id: "identity-pending", verification_status: "pending", is_default: false }),
    ], "identity-pending")).toMatchObject({ kind: "unavailable", issue: "stale_track_identity" });
    expect(resolveOnboardingApplicationIdentity([
      identity(),
      identity({ label: "Duplicate ID" }),
    ], "identity-default")).toMatchObject({ kind: "unavailable", issue: "identity_data_invalid" });
  });

  it("does not choose among multiple verified identities without a default", () => {
    expect(resolveOnboardingApplicationIdentity([
      identity({ id: "identity-one", is_default: false }),
      identity({ id: "identity-two", is_default: false }),
    ])).toMatchObject({ kind: "unavailable", issue: "multiple_verified_identities" });
  });

  it("fails closed when multiple verified identities are marked default", () => {
    expect(resolveOnboardingApplicationIdentity([
      identity({ id: "identity-one" }),
      identity({ id: "identity-two" }),
    ])).toMatchObject({ kind: "unavailable", issue: "multiple_verified_identities" });
  });

  it("fails closed for duplicate identity IDs instead of following a matching binding", () => {
    expect(resolveOnboardingApplicationIdentity([
      identity(),
      identity({ label: "Injected duplicate" }),
    ], "identity-default")).toMatchObject({ kind: "unavailable", issue: "identity_data_invalid" });
  });

  it("uses the existing verified default from the onboarding preview scenario", () => {
    const workspace = previewWorkspaceForScenario(previewWorkspace, "onboarding");
    expect(resolveOnboardingApplicationIdentity(workspace.application_identities)).toMatchObject({
      kind: "selected",
      identity: { id: "identity-primary" },
    });
  });

  it("accepts only an explicitly selected returned verified identity", () => {
    const identities = [identity(), identity({ id: "identity-career", is_default: false })];
    expect(validateSelectedOnboardingApplicationIdentity(identities, "identity-career")).toMatchObject({
      kind: "selected",
      identity: { id: "identity-career" },
    });
    expect(validateSelectedOnboardingApplicationIdentity(identities, "identity-missing")).toMatchObject({
      kind: "unavailable",
      issue: "selected_identity_unavailable",
    });
  });

  it("does not derive an identity from the profile resume-contact email", () => {
    const profileResumeContact = "resume-contact@example.test";
    const resolution = resolveOnboardingApplicationIdentity([
      identity({ email: "verified-login@example.test" }),
    ]);
    expect(profileResumeContact).not.toBe("verified-login@example.test");
    expect(resolution).toMatchObject({
      kind: "selected",
      identity: { email: "verified-login@example.test" },
    });
  });
});
