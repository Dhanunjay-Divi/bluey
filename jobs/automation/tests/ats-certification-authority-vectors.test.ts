import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

type AuthorityKind =
  | "trustPolicy"
  | "layoutObservation"
  | "manifest"
  | "activation"
  | "revocation";

interface AuthorityVector {
  name: string;
  kind: AuthorityKind;
  authority: Record<string, unknown>;
  sha256: string;
}

const vectors = (JSON.parse(
  readFileSync(
    new URL("./fixtures/ats-certification-authority-vectors.json", import.meta.url),
    "utf8",
  ),
) as { schemaVersion: number; vectors: AuthorityVector[] }).vectors;

const authorityKeys: Record<AuthorityKind, readonly string[]> = {
  trustPolicy: [
    "version",
    "audience",
    "policyId",
    "trustGeneration",
    "predecessorPolicySha256",
    "delegatedTrust",
    "certificationRequirements",
    "issuedAtMs",
    "validFromMs",
    "expiresAtMs",
  ],
  layoutObservation: [
    "version",
    "audience",
    "observationId",
    "policySha256",
    "provider",
    "targetFingerprintSha256",
    "pageVariant",
    "surfaceSha256",
    "adapterVersion",
    "runnerTargetSha256",
    "evidenceClass",
    "controls",
    "form",
    "challengeCategories",
    "stepCount",
    "confirmationStateCategories",
    "predecessorObservationSha256",
    "observedAtMs",
    "issuedAtMs",
    "expiresAtMs",
  ],
  manifest: [
    "version",
    "audience",
    "certificationId",
    "policySha256",
    "manifestGeneration",
    "predecessorManifestSha256",
    "provider",
    "targetKey",
    "allowedProviderHosts",
    "variantKey",
    "surfaceSha256",
    "scopeSha256",
    "adapterVersion",
    "finalSubmitControlId",
    "adapterBundleSha256",
    "sourceCommit",
    "layoutContractVersion",
    "layoutContractSha256",
    "maximumCapability",
    "certificationProfile",
    "evidenceSha256s",
    "runtimeTargets",
    "testedAtMs",
    "issuedAtMs",
    "notBeforeMs",
    "expiresAtMs",
  ],
  activation: [
    "version",
    "audience",
    "activationId",
    "policySha256",
    "activationGeneration",
    "predecessorActivationSha256",
    "manifestSha256",
    "scopeSha256",
    "channel",
    "channelSequence",
    "capability",
    "accountAllowlistSha256",
    "canaryMaxSubmissions",
    "canaryAccountCap",
    "canaryConcurrencyCap",
    "canaryDailySideEffectCap",
    "canaryEvidenceManifestSha256",
    "approvalRef",
    "issuedAtMs",
    "notBeforeMs",
    "expiresAtMs",
  ],
  revocation: [
    "version",
    "audience",
    "revocationId",
    "policySha256",
    "revocationGeneration",
    "predecessorRevocationSha256",
    "subjectKind",
    "subjectId",
    "subjectSha256",
    "reasonRef",
    "issuedAtMs",
    "effectiveAtMs",
  ],
};

function hasExactTopLevelSchema(vector: AuthorityVector): boolean {
  return JSON.stringify(Object.keys(vector.authority)) ===
    JSON.stringify(authorityKeys[vector.kind]);
}

function isCanonicalAuthorityText(vector: AuthorityVector, text: string): boolean {
  try {
    const authority = JSON.parse(text) as Record<string, unknown>;
    return text === JSON.stringify(authority) &&
      hasExactTopLevelSchema({ ...vector, authority });
  } catch {
    return false;
  }
}

function decodeStrictBase64url(value: string): Buffer | undefined {
  if (!/^[A-Za-z0-9_-]+$/.test(value)) return undefined;
  const decoded = Buffer.from(value, "base64url");
  return decoded.length > 0 && decoded.toString("base64url") === value
    ? decoded
    : undefined;
}

describe("shared ATS certification authority vectors", () => {
  it.each(vectors)("matches canonical UTF-8 bytes and SHA-256 for $name", (vector) => {
    const canonical = JSON.stringify(vector.authority);
    const bytes = Buffer.from(canonical, "utf8");

    expect(JSON.stringify(JSON.parse(canonical))).toBe(canonical);
    expect(createHash("sha256").update(bytes).digest("hex")).toBe(vector.sha256);

    const canonicalBase64url = bytes.toString("base64url");
    expect(canonicalBase64url).not.toContain("=");
    expect(Buffer.from(canonicalBase64url, "base64url")).toEqual(bytes);
  });

  it.each(vectors)("keeps a closed top-level schema for $name", (vector) => {
    expect(hasExactTopLevelSchema(vector)).toBe(true);
    expect(
      hasExactTopLevelSchema({
        ...vector,
        authority: { ...vector.authority, unexpected: true },
      }),
    ).toBe(false);
  });

  it.each(vectors)(
    "rejects noncanonical, duplicate-key, unknown-field, and invalid encodings for $name",
    (vector) => {
      const canonical = JSON.stringify(vector.authority);
      expect(isCanonicalAuthorityText(vector, canonical)).toBe(true);
      expect(isCanonicalAuthorityText(vector, ` ${canonical}`)).toBe(false);
      expect(isCanonicalAuthorityText(vector, `${canonical}\n`)).toBe(false);
      expect(
        isCanonicalAuthorityText(
          vector,
          canonical.replace("{", "{\"version\":1,"),
        ),
      ).toBe(false);
      expect(
        isCanonicalAuthorityText(
          vector,
          JSON.stringify({ ...vector.authority, unexpected: true }),
        ),
      ).toBe(false);

      const encoded = Buffer.from(canonical, "utf8").toString("base64url");
      expect(decodeStrictBase64url(encoded)).toEqual(Buffer.from(canonical, "utf8"));
      expect(decodeStrictBase64url(`${encoded}=`)).toBeUndefined();
      expect(decodeStrictBase64url("%not-base64url")).toBeUndefined();
    },
  );
});
