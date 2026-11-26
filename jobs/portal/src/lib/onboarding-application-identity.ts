import type { ApplicationIdentity } from "../types";

export type ApplicationIdentityIssue =
  | "identity_data_invalid"
  | "no_verified_identity"
  | "multiple_verified_identities"
  | "stale_track_identity"
  | "selected_identity_unavailable";

export type ApplicationIdentityResolution =
  | {
    kind: "selected";
    identity: ApplicationIdentity;
    verified: readonly ApplicationIdentity[];
  }
  | {
    kind: "unavailable";
    issue: ApplicationIdentityIssue;
    verified: readonly ApplicationIdentity[];
  };

function isIdentity(value: unknown): value is ApplicationIdentity {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const identity = value as Partial<ApplicationIdentity>;
  return typeof identity.id === "string"
    && identity.id.trim().length > 0
    && typeof identity.email === "string"
    && identity.email.trim().length > 0
    && typeof identity.label === "string"
    && (identity.verification_status === "pending" || identity.verification_status === "verified")
    && typeof identity.is_default === "boolean";
}

function verifiedIdentities(identities: readonly ApplicationIdentity[]): ApplicationIdentityResolution | ApplicationIdentity[] {
  if (!Array.isArray(identities) || identities.some((identity) => !isIdentity(identity))) {
    return { kind: "unavailable", issue: "identity_data_invalid", verified: [] };
  }
  const ids = new Set<string>();
  for (const identity of identities) {
    if (ids.has(identity.id)) {
      return { kind: "unavailable", issue: "identity_data_invalid", verified: [] };
    }
    ids.add(identity.id);
  }
  return identities.filter((identity) => identity.verification_status === "verified");
}

function unavailable(
  issue: ApplicationIdentityIssue,
  verified: readonly ApplicationIdentity[],
): ApplicationIdentityResolution {
  return { kind: "unavailable", issue, verified: Object.freeze([...verified]) };
}

function selected(
  identity: ApplicationIdentity,
  verified: readonly ApplicationIdentity[],
): ApplicationIdentityResolution {
  return { kind: "selected", identity, verified: Object.freeze([...verified]) };
}

/**
 * Chooses an initial application identity without deriving one from profile
 * contact data. A present Track binding is authoritative only when that exact
 * ID remains in the returned verified-identity set.
 */
export function resolveOnboardingApplicationIdentity(
  identities: readonly ApplicationIdentity[],
  existingTrackIdentityId?: string,
): ApplicationIdentityResolution {
  const candidates = verifiedIdentities(identities);
  if (!Array.isArray(candidates)) return candidates;
  if (typeof existingTrackIdentityId === "string" && existingTrackIdentityId.trim()) {
    const bound = candidates.find((identity) => identity.id === existingTrackIdentityId);
    return bound ? selected(bound, candidates) : unavailable("stale_track_identity", candidates);
  }
  if (candidates.length === 0) return unavailable("no_verified_identity", candidates);
  const defaults = candidates.filter((identity) => identity.is_default);
  if (defaults.length === 1) return selected(defaults[0], candidates);
  if (defaults.length > 1 || candidates.length > 1) {
    return unavailable("multiple_verified_identities", candidates);
  }
  return selected(candidates[0], candidates);
}

/** Ensures a UI-selected ID still names a returned verified identity at submit. */
export function validateSelectedOnboardingApplicationIdentity(
  identities: readonly ApplicationIdentity[],
  selectedIdentityId: string,
): ApplicationIdentityResolution {
  const candidates = verifiedIdentities(identities);
  if (!Array.isArray(candidates)) return candidates;
  if (typeof selectedIdentityId !== "string" || !selectedIdentityId.trim()) {
    return unavailable("selected_identity_unavailable", candidates);
  }
  const selectedIdentity = candidates.find((identity) => identity.id === selectedIdentityId);
  return selectedIdentity
    ? selected(selectedIdentity, candidates)
    : unavailable("selected_identity_unavailable", candidates);
}

export function applicationIdentityIssueCopy(issue: ApplicationIdentityIssue): string {
  switch (issue) {
    case "no_verified_identity":
      return "Add and verify an application email in Settings before creating this Career Track.";
    case "multiple_verified_identities":
      return "Choose a verified application email before creating this Career Track.";
    case "stale_track_identity":
    case "selected_identity_unavailable":
      return "The selected application email is no longer verified. Choose a currently verified email.";
    case "identity_data_invalid":
      return "Application email details are unavailable. Refresh before creating this Career Track.";
  }
}
