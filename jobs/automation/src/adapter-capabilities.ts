import type { AtsKind } from "./contracts.js";

export type SubmissionPolicy = "automate" | "handoff" | "blocked";

export type SubmissionCapability =
  | "certified"
  | "beta_review"
  | "handoff"
  | "unknown_review"
  | "blocked";

export type AdapterImplementation =
  | "provider_state_machine"
  | "review_fill"
  | "handoff";

export interface AtsCapabilityProfile {
  kind: AtsKind;
  implementation: AdapterImplementation;
  policy: SubmissionPolicy;
  capability: SubmissionCapability;
  certified: boolean;
  finalSubmission: "explicit_review_only" | "disabled";
  expectedAdapterVersion?: string;
  requiresOriginalSourceRevalidation: boolean;
  reason: string;
}

const PROFILES: Readonly<Record<AtsKind, AtsCapabilityProfile>> = Object.freeze(
  {
    greenhouse: Object.freeze({
      kind: "greenhouse",
      implementation: "provider_state_machine",
      policy: "automate",
      capability: "beta_review",
      certified: false,
      finalSubmission: "explicit_review_only",
      expectedAdapterVersion: "2026.07.1-beta.1",
      requiresOriginalSourceRevalidation: false,
      reason:
        "Greenhouse can use Bluey's reviewed provider runner after packet approval.",
    }),
    lever: Object.freeze({
      kind: "lever",
      implementation: "provider_state_machine",
      policy: "automate",
      capability: "beta_review",
      certified: false,
      finalSubmission: "explicit_review_only",
      expectedAdapterVersion: "2026.07.0-beta.1",
      requiresOriginalSourceRevalidation: false,
      reason:
        "Lever can use Bluey's reviewed provider runner after packet approval.",
    }),
    workday: Object.freeze({
      kind: "workday",
      implementation: "review_fill",
      policy: "handoff",
      capability: "unknown_review",
      certified: false,
      finalSubmission: "disabled",
      requiresOriginalSourceRevalidation: false,
      reason:
        "Bluey can fill this Workday application for review, but cannot submit it unattended.",
    }),
    ashby: Object.freeze({
      kind: "ashby",
      implementation: "review_fill",
      policy: "handoff",
      capability: "unknown_review",
      certified: false,
      finalSubmission: "disabled",
      requiresOriginalSourceRevalidation: false,
      reason:
        "Bluey can fill this Ashby application for review, but cannot submit it unattended.",
    }),
    smartrecruiters: Object.freeze({
      kind: "smartrecruiters",
      implementation: "review_fill",
      policy: "handoff",
      capability: "unknown_review",
      certified: false,
      finalSubmission: "disabled",
      requiresOriginalSourceRevalidation: false,
      reason:
        "Bluey can fill this SmartRecruiters application for review, but cannot submit it unattended.",
    }),
    semantic: Object.freeze({
      kind: "semantic",
      implementation: "handoff",
      policy: "handoff",
      capability: "unknown_review",
      certified: false,
      finalSubmission: "disabled",
      requiresOriginalSourceRevalidation: true,
      reason:
        "Bluey can prepare this application for review; the site is not certified for submission.",
    }),
  },
);

export function atsCapabilityProfile(kind: AtsKind): AtsCapabilityProfile {
  return PROFILES[kind];
}

export function adapterCanFinalize(kind: AtsKind, version: string): boolean {
  const profile = atsCapabilityProfile(kind);
  return (
    profile.finalSubmission === "explicit_review_only" &&
    profile.implementation === "provider_state_machine" &&
    profile.expectedAdapterVersion === version
  );
}
