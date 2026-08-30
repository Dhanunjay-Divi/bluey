import {
  createHash,
  createPublicKey,
  verify,
  type KeyObject,
} from "node:crypto";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

type JsonObject = Record<string, unknown>;

type CanonicalMutation =
  | "remove_terminal_newline"
  | "append_terminal_newline"
  | "prepend_ascii_space"
  | "duplicate_top_level_version"
  | "append_unknown_top_level_field"
  | "swap_version_and_audience";

interface CanonicalRejectionVector {
  name: string;
  mutation: CanonicalMutation;
  expectedError: "invalid_authority";
}

interface KeyRejectionVector {
  name: "malformed_key_length" | "weak_identity_point";
  publicKeyBase64url: string;
  expectedError: "invalid_trust_anchor";
}

interface SignatureRejectionVector {
  name: "small_order_r" | "noncanonical_s" | "padded_base64url";
  signature: string;
  expectedError: "invalid_signature" | "invalid_envelope";
}

interface JobIntegrityAuthorityFixture {
  schemaVersion: number;
  attestation: JsonObject;
  attestationSha256: string;
  authorization: {
    publicKeyBase64url: string;
    payload: JsonObject;
    payloadSha256: string;
    envelope: JsonObject & {
      signatures: Array<{ keyId: string; signature: string }>;
    };
    envelopeSha256: string;
  };
  canonicalRejections: CanonicalRejectionVector[];
  keyRejections: KeyRejectionVector[];
  signatureRejections: SignatureRejectionVector[];
}

const fixture = JSON.parse(
  readFileSync(
    new URL("./fixtures/job-integrity-authority-vectors.json", import.meta.url),
    "utf8",
  ),
) as JobIntegrityAuthorityFixture;

const ATTESTATION_KEYS = [
  "version",
  "audience",
  "attestationId",
  "policySha256",
  "subjectSha256",
  "sourceMaterialSha256",
  "attestationGeneration",
  "predecessorAttestationSha256",
  "canonicalJobId",
  "source",
  "employer",
  "risk",
  "assessedAtMs",
  "issuedAtMs",
  "notBeforeMs",
  "expiresAtMs",
] as const;

const AUTHORIZATION_PAYLOAD_KEYS = [
  "version",
  "audience",
  "authorizationId",
  "role",
  "policySha256",
  "targetAudience",
  "targetSha256",
  "signedAtMs",
] as const;

const GROUP_ORDER = Buffer.from([
  0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58,
  0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
]);

function canonicalJson(value: unknown): Buffer {
  return Buffer.from(`${JSON.stringify(value)}\n`, "utf8");
}

function sha256(value: Uint8Array): string {
  return createHash("sha256").update(value).digest("hex");
}

function isJsonObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: unknown, keys: readonly string[]): value is JsonObject {
  return isJsonObject(value) &&
    JSON.stringify(Object.keys(value)) === JSON.stringify(keys);
}

function hasExactEvidenceSchema(value: unknown): boolean {
  return hasExactKeys(value, [
    "class",
    "kind",
    "sha256",
    "observedAtMs",
    "expiresAtMs",
  ]);
}

function hasExactAttestationSchema(value: unknown): value is JsonObject {
  if (!hasExactKeys(value, ATTESTATION_KEYS) ||
      !hasExactKeys(value.source, [
        "providerFamily",
        "providerRecordId",
        "target",
        "canonicalApplicationUrl",
        "applicationDomain",
        "atsTenantBindingSha256",
      ]) ||
      !hasExactKeys(value.source.target, ["host", "tenant", "job", "variant"]) ||
      !hasExactKeys(value.employer, [
        "status",
        "canonicalEmployerId",
        "canonicalEmployerDomain",
        "verificationMethods",
        "evidence",
      ]) ||
      !hasExactKeys(value.risk, [
        "status",
        "signalCodes",
        "policySha256",
        "inputSha256",
        "engineReleaseSha256",
        "evidence",
      ])) {
    return false;
  }
  return Array.isArray(value.employer.evidence) &&
    value.employer.evidence.every(hasExactEvidenceSchema) &&
    Array.isArray(value.risk.evidence) &&
    value.risk.evidence.every(hasExactEvidenceSchema);
}

function hasExactAuthorizationSchema(value: unknown, includeSignatures: boolean): boolean {
  const keys = includeSignatures
    ? [...AUTHORIZATION_PAYLOAD_KEYS, "signatures"]
    : AUTHORIZATION_PAYLOAD_KEYS;
  if (!hasExactKeys(value, keys)) return false;
  if (!includeSignatures) return true;
  return Array.isArray(value.signatures) &&
    value.signatures.every((signature) =>
      hasExactKeys(signature, ["keyId", "signature"])
    );
}

function decodeStrictBase64url(value: string, expectedLength?: number): Buffer | undefined {
  if (!/^[A-Za-z0-9_-]+$/.test(value)) return undefined;
  const decoded = Buffer.from(value, "base64url");
  if (decoded.length === 0 || decoded.toString("base64url") !== value) return undefined;
  return expectedLength === undefined || decoded.length === expectedLength
    ? decoded
    : undefined;
}

function ed25519PublicKey(publicKeyBase64url: string): KeyObject {
  const raw = decodeStrictBase64url(publicKeyBase64url, 32);
  if (!raw) throw new Error("invalid Ed25519 public key vector");
  return createPublicKey({
    key: Buffer.concat([
      Buffer.from("302a300506032b6570032100", "hex"),
      raw,
    ]),
    format: "der",
    type: "spki",
  });
}

function isCanonicalAttestation(bytes: Buffer): boolean {
  try {
    const parsed = JSON.parse(bytes.toString("utf8")) as unknown;
    return hasExactAttestationSchema(parsed) && canonicalJson(parsed).equals(bytes);
  } catch {
    return false;
  }
}

function mutateCanonicalAttestation(
  canonical: Buffer,
  mutation: CanonicalMutation,
): Buffer {
  const text = canonical.toString("utf8");
  switch (mutation) {
    case "remove_terminal_newline":
      return canonical.subarray(0, canonical.length - 1);
    case "append_terminal_newline":
      return Buffer.concat([canonical, Buffer.from("\n", "utf8")]);
    case "prepend_ascii_space":
      return Buffer.concat([Buffer.from(" ", "utf8"), canonical]);
    case "duplicate_top_level_version":
      return Buffer.from(text.replace("{\"version\":1,", "{\"version\":1,\"version\":1,"));
    case "append_unknown_top_level_field":
      return Buffer.from(`${text.slice(0, -2)},\"unexpected\":true}\n`, "utf8");
    case "swap_version_and_audience": {
      const { version, audience, ...rest } = fixture.attestation;
      return canonicalJson({ audience, version, ...rest });
    }
  }
}

function littleEndianAtLeast(value: Buffer, minimum: Buffer): boolean {
  for (let index = value.length - 1; index >= 0; index -= 1) {
    if (value[index] !== minimum[index]) return value[index] > minimum[index];
  }
  return true;
}

describe("shared job-integrity canonical and Ed25519 vectors", () => {
  it("matches the Rust attestation, authorization payload, and envelope bytes", () => {
    expect(fixture.schemaVersion).toBe(1);
    expect(hasExactAttestationSchema(fixture.attestation)).toBe(true);
    expect(hasExactAuthorizationSchema(fixture.authorization.payload, false)).toBe(true);
    expect(hasExactAuthorizationSchema(fixture.authorization.envelope, true)).toBe(true);

    const attestationBytes = canonicalJson(fixture.attestation);
    const payloadBytes = canonicalJson(fixture.authorization.payload);
    const envelopeBytes = canonicalJson(fixture.authorization.envelope);
    expect(attestationBytes.at(-1)).toBe(0x0a);
    expect(attestationBytes.at(-2)).not.toBe(0x0a);
    expect(sha256(attestationBytes)).toBe(fixture.attestationSha256);
    expect(sha256(payloadBytes)).toBe(fixture.authorization.payloadSha256);
    expect(sha256(envelopeBytes)).toBe(fixture.authorization.envelopeSha256);
    expect(fixture.authorization.payload.targetSha256).toBe(fixture.attestationSha256);

    const signature = decodeStrictBase64url(
      fixture.authorization.envelope.signatures[0].signature,
      64,
    );
    expect(signature).toBeDefined();
    expect(
      verify(
        null,
        payloadBytes,
        ed25519PublicKey(fixture.authorization.publicKeyBase64url),
        signature!,
      ),
    ).toBe(true);
  });

  it.each(fixture.canonicalRejections)(
    "rejects the noncanonical Rust input: $name",
    (vector) => {
      expect(vector.expectedError).toBe("invalid_authority");
      const canonical = canonicalJson(fixture.attestation);
      expect(isCanonicalAttestation(canonical)).toBe(true);
      expect(
        isCanonicalAttestation(mutateCanonicalAttestation(canonical, vector.mutation)),
      ).toBe(false);
    },
  );

  it.each(fixture.keyRejections)("preserves the strict key rejection: $name", (vector) => {
    expect(vector.expectedError).toBe("invalid_trust_anchor");
    const decoded = decodeStrictBase64url(vector.publicKeyBase64url);
    expect(decoded).toBeDefined();
    if (vector.name === "malformed_key_length") {
      expect(decoded).toHaveLength(3);
      expect(decodeStrictBase64url(vector.publicKeyBase64url, 32)).toBeUndefined();
    } else {
      expect(decoded).toEqual(Buffer.concat([Buffer.from([1]), Buffer.alloc(31)]));
    }
  });

  it.each(fixture.signatureRejections)(
    "rejects the strict Ed25519 input: $name",
    (vector) => {
      const signature = decodeStrictBase64url(vector.signature, 64);
      if (vector.name === "padded_base64url") {
        expect(vector.expectedError).toBe("invalid_envelope");
        expect(signature).toBeUndefined();
        return;
      }
      expect(vector.expectedError).toBe("invalid_signature");
      expect(signature).toBeDefined();
      if (vector.name === "small_order_r") {
        expect(signature!.subarray(0, 32)).toEqual(
          Buffer.concat([Buffer.from([1]), Buffer.alloc(31)]),
        );
      } else {
        expect(littleEndianAtLeast(signature!.subarray(32), GROUP_ORDER)).toBe(true);
      }
      expect(
        verify(
          null,
          canonicalJson(fixture.authorization.payload),
          ed25519PublicKey(fixture.authorization.publicKeyBase64url),
          signature!,
        ),
      ).toBe(false);
    },
  );
});
