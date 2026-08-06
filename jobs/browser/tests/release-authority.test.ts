import {
  createPrivateKey,
  createPublicKey,
  type KeyObject,
} from "node:crypto";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import * as authority from "../src/release-authority.js";

const POLICY_ISSUED_AT_MS = 1_785_969_900_000;
const MANIFEST_PUBLISHED_AT_MS = 1_785_970_001_000;
const ACTIVATION_ISSUED_AT_MS = 1_785_970_100_000;
const ROLLBACK_ISSUED_AT_MS = 1_785_970_200_000;
const REVOCATION_ISSUED_AT_MS = 1_785_970_300_000;
const SUCCESSOR_ISSUED_AT_MS = 1_785_970_400_000;
const POLICY_EXPIRES_AT_MS = 1_817_506_000_000;

const keyIndexes = Object.freeze({
  "incident-key-1": 1,
  "promotion-key-1": 2,
  "promotion-key-2": 3,
  "promotion-key-3": 4,
  "promotion-key-4": 5,
  "release-key-1": 6,
  "release-key-2": 7,
  "release-key-3": 8,
  "release-key-4": 9,
  "root-key-1": 10,
  "root-key-2": 11,
  "root-key-3": 12,
  "root-key-4": 13,
});

describe("Bluey Browser release authority", () => {
  it("matches every checked-in exact cross-language authority vector", () => {
    const checkedIn = JSON.parse(
      readFileSync(
        new URL("../fixtures/release-authority-v1.json", import.meta.url),
        "utf8",
      ),
    ) as unknown;
    expect(checkedIn).toEqual(sharedVector());
  });

  it("preserves the packaged canonical build proof API", () => {
    const descriptor = descriptorsFixture()[0];
    const proof = authority.browserBuildProof(descriptor);
    const verified = authority.verifyBrowserBuildProof(proof, {
      "release-key-1": publicKey("release-key-1"),
    });

    expect(verified.descriptor).toEqual(descriptor);
    expect(verified.descriptorBytes).toEqual(
      authority.canonicalBrowserBuildDescriptorBytes(descriptor),
    );
    expect(verified.descriptorSha256).toBe(
      authority.browserBuildDescriptorSha256(descriptor),
    );
  });

  it("rejects a descriptor for any foreign application identity", () => {
    expect(() => authority.parseBrowserBuildDescriptor({
      ...descriptorsFixture()[0],
      appId: "com.example.other-browser",
    })).toThrowError(/descriptor/i);
  });

  it("verifies exact threshold sets for all release authority roles", () => {
    const fixture = authorityFixture();
    expect(
      authority.verifyBrowserReleaseTrustPolicy(
        fixture.policyBytes,
        fixture.policySignatureSetBytes,
        null,
        bootstrapAnchor(),
        ACTIVATION_ISSUED_AT_MS,
      ),
    ).toEqual(fixture.policy);
    expect(
      authority.verifyBrowserReleaseManifest(
        fixture.manifestBytes,
        fixture.manifestSignatureSetBytes,
        fixture.policyBytes,
        ACTIVATION_ISSUED_AT_MS,
      ),
    ).toEqual(fixture.manifest);
    expect(
      authority.verifyBrowserReleaseActivation(
        fixture.activationBytes,
        fixture.activationSignatureSetBytes,
        fixture.policyBytes,
        ACTIVATION_ISSUED_AT_MS,
      ),
    ).toEqual(fixture.activation);
    expect(
      authority.verifyBrowserReleaseRollback(
        fixture.rollbackBytes,
        fixture.rollbackSignatureSetBytes,
        fixture.policyBytes,
        ROLLBACK_ISSUED_AT_MS,
      ),
    ).toEqual(fixture.rollback);
    expect(
      authority.verifyBrowserReleaseRevocation(
        fixture.revocationBytes,
        fixture.revocationSignatureSetBytes,
        fixture.policyBytes,
        REVOCATION_ISSUED_AT_MS,
      ),
    ).toEqual(fixture.revocation);
    expect(fixture.activation.signatureSetSha256).toBe(
      authority.browserReleaseSignatureSetSha256(
        fixture.manifestSignatureSet,
      ),
    );
    expect(fixture.revocation.subjectSha256).toBe(
      authority.browserReleaseAuthorityTargetSha256(
        Buffer.from(fixture.revocation.subjectId, "utf8"),
      ),
    );
  });

  it("allows delegated-key revocation but reserves root changes for root policy", () => {
    const fixture = authorityFixture();
    const verifySigningKeyRevocation = (
      keyId: string,
      publicKeyId = keyId,
    ) => {
      const revocation = authority.createBrowserReleaseRevocation({
        ...fixture.revocation,
        revocationId: `browser-revocation-${keyId}`,
        subjectKind: "signing-key",
        subjectId: keyId,
        subjectSha256: authority.browserReleaseAuthorityTargetSha256(
          Buffer.from(publicKey(publicKeyId), "base64url"),
        ),
      });
      const revocationBytes = authority.canonicalBrowserReleaseRevocationBytes(
        revocation,
      );
      const revocationSignatures = signatureSet(
        `browser-revocation-${keyId}-incident-set`,
        revocationBytes,
        authority.BLUEY_BROWSER_REVOCATION_AUDIENCE,
        "incident",
        1,
        revocation.issuedAtMs,
        ["incident-key-1"],
      );
      return () => authority.verifyBrowserReleaseRevocation(
        revocationBytes,
        authority.canonicalBrowserReleaseSignatureSetBytes(revocationSignatures),
        fixture.policyBytes,
        revocation.issuedAtMs,
      );
    };

    expect(verifySigningKeyRevocation("promotion-key-1")()).toMatchObject({
      subjectKind: "signing-key",
      subjectId: "promotion-key-1",
    });
    expect(verifySigningKeyRevocation("root-key-1")).toThrowError(/revocation/i);
    expect(
      verifySigningKeyRevocation("promotion-key-1", "promotion-key-2"),
    ).toThrowError(/revocation/i);
    expect(
      verifySigningKeyRevocation("unknown-key-1", "promotion-key-1"),
    ).toThrowError(/revocation/i);
  });

  it("requires exact artifacts with one descriptor and app image per target", () => {
    const fixture = authorityFixture();
    const artifacts = fixture.manifest.artifacts;
    expect(artifacts.map(artifactTarget)).toEqual([
      "darwin:arm64:darwin-dmg",
      "darwin:arm64:darwin-zip",
      "darwin:x64:darwin-dmg",
      "darwin:x64:darwin-zip",
      "windows:x64:windows-nsis",
    ]);
    expect(artifacts[0]?.buildDescriptorSha256).toBe(
      artifacts[1]?.buildDescriptorSha256,
    );
    expect(artifacts[2]?.buildDescriptorSha256).toBe(
      artifacts[3]?.buildDescriptorSha256,
    );
    expect(artifacts[0]?.appContentSha256).toBe(
      artifacts[1]?.appContentSha256,
    );
    expect(artifacts[2]?.appContentSha256).toBe(
      artifacts[3]?.appContentSha256,
    );

    expect(() =>
      authority.parseBrowserReleaseManifest({
        ...fixture.manifest,
        artifacts: artifacts.slice(0, 4),
      }),
    ).toThrowError(/manifest/i);
    expect(() =>
      authority.parseBrowserReleaseManifest({
        ...fixture.manifest,
        artifacts: artifacts.map((artifact, index) =>
          index === 1 ? { ...artifact, url: artifacts[0]!.url } : artifact,
        ),
      }),
    ).toThrowError(/manifest/i);
    expect(() =>
      authority.parseBrowserReleaseManifest({
        ...fixture.manifest,
        artifacts: artifacts.map((artifact, index) =>
          index === 1
            ? { ...artifact, appContentSha256: "f".repeat(64) }
            : artifact,
        ),
      }),
    ).toThrowError(/manifest/i);
    for (const [index, extension] of [
      [0, "zip"],
      [1, "dmg"],
      [4, "dmg"],
    ] as const) {
      expect(() =>
        authority.parseBrowserReleaseManifest({
          ...fixture.manifest,
          artifacts: artifacts.map((artifact, artifactIndex) =>
            artifactIndex === index
              ? { ...artifact, url: artifact.url.replace(/[^.]+$/, extension) }
              : artifact,
          ),
        }),
      ).toThrowError(/manifest/i);
    }
    expect(() =>
      authority.parseBrowserReleaseManifest({
        ...fixture.manifest,
        artifacts: [...artifacts, artifacts[0]],
      }),
    ).toThrowError(/manifest/i);
    expect(() =>
      authority.parseBrowserReleaseManifest({
        ...fixture.manifest,
        artifacts: [artifacts[1], artifacts[0], ...artifacts.slice(2)],
      }),
    ).toThrowError(/manifest/i);
    expect(() =>
      authority.parseBrowserReleaseManifest({
        ...fixture.manifest,
        artifacts: artifacts.map((artifact, index) =>
          index === 4 ? { ...artifact, architecture: "arm64" } : artifact,
        ),
      }),
    ).toThrowError(/manifest/i);
    expect(() =>
      authority.parseBrowserReleaseManifest({
        ...fixture.manifest,
        artifacts: artifacts.map((artifact, index) =>
          index === 1
            ? { ...artifact, buildDescriptorSha256: "f".repeat(64) }
            : artifact,
        ),
      }),
    ).toThrowError(/manifest/i);
    expect(() =>
      authority.parseBrowserReleaseManifest({
        ...fixture.manifest,
        artifacts: artifacts.map((artifact, index) =>
          index === 1 ? { ...artifact, sha256: artifacts[0]!.sha256 } : artifact,
        ),
      }),
    ).toThrowError(/manifest/i);
  });

  it("pins a canonical artifact origin and rejects ambiguous release paths", () => {
    const fixture = authorityFixture();
    expect(() =>
      authority.assertBrowserReleaseManifestArtifactOrigin(
        fixture.manifest,
        fixture.policy,
      )
    ).not.toThrow();

    for (const artifactOrigin of [
      "http://bluey.sh",
      "https://bluey.sh:443",
      "https://user@bluey.sh",
      "https://bluey.sh/",
      "https://bluey.sh/path",
      "https://bluey.sh?mirror=1",
      "https://bluey.sh#mirror",
    ]) {
      expect(() =>
        authority.parseBrowserReleaseTrustPolicy({
          ...fixture.policy,
          artifactOrigin,
        }),
      ).toThrowError(/trust policy/i);
    }

    for (const host of ["artifacts.example", "bluey.sh.evil.example"]) {
      const foreignManifest = manifestWithFirstArtifactUrl(
        fixture.manifest,
        fixture.manifest.artifacts[0]!.url.replace("bluey.sh", host),
      );
      expect(() =>
        authority.assertBrowserReleaseManifestArtifactOrigin(
          foreignManifest,
          fixture.policy,
        ),
      ).toThrowError(/binding/i);
    }

    const foreignManifest = manifestWithFirstArtifactUrl(
      fixture.manifest,
      fixture.manifest.artifacts[0]!.url.replace(
        "bluey.sh",
        "artifacts.example",
      ),
    );
    const foreignBytes = authority.canonicalBrowserReleaseManifestBytes(
      foreignManifest,
    );
    const foreignSignatureSet = signatureSet(
      "manifest-foreign-origin-release-set",
      foreignBytes,
      authority.BLUEY_BROWSER_RELEASE_AUDIENCE,
      "release",
      1,
      foreignManifest.publishedAtMs,
      ["release-key-1", "release-key-2"],
    );
    expect(() =>
      authority.verifyBrowserReleaseManifest(
        foreignBytes,
        authority.canonicalBrowserReleaseSignatureSetBytes(
          foreignSignatureSet,
        ),
        fixture.policyBytes,
        ACTIVATION_ISSUED_AT_MS,
      ),
    ).toThrowError(/binding/i);

    const validUrl = fixture.manifest.artifacts[0]!.url;
    for (const confusingUrl of [
      validUrl.replace(
        "/jobs/",
        "/mirror/jobs/",
      ),
      validUrl.replace(
        "/browser-release-603-1/",
        "/%62rowser-release-603-1/",
      ),
      validUrl.replace(
        "/browser-release-603-1/",
        "/browser-release-603-1//",
      ),
      validUrl.replace("darwin-arm64-dmg.dmg", "darwin%2Farm64-dmg.dmg"),
      validUrl.replace(
        "darwin-arm64-dmg.dmg",
        "nested/darwin-arm64-dmg.dmg",
      ),
    ]) {
      expect(() =>
        authority.parseBrowserReleaseManifest(
          manifestWithFirstArtifactUrl(fixture.manifest, confusingUrl),
        ),
      ).toThrowError(/manifest/i);
    }

    expect(
      authority.parseBrowserReleaseManifest({
        ...fixture.manifest,
        releaseNotesUrl: fixture.manifest.releaseNotesUrl.replace(
          "RELEASE.md",
          "notes/RELEASE.md",
        ),
      }).releaseNotesUrl,
    ).toContain("/notes/RELEASE.md");
  });

  it("rejects case-insensitive mutable release path identities", () => {
    const fixture = authorityFixture();
    for (const releaseId of [
      "beta",
      "current",
      "download",
      "internal",
      "latest",
      "stable",
      "LATEST",
      "Stable",
    ]) {
      expect(() =>
        descriptorFixture("darwin", "arm64", { releaseId }),
      ).toThrowError(/descriptor/i);
      expect(() =>
        authority.parseBrowserReleaseManifest(
          manifestWithReleaseId(fixture.manifest, releaseId),
        ),
      ).toThrowError(/manifest/i);
    }

  });

  it("rejects noncanonical bytes and unknown fields for every authority layer", () => {
    const fixture = authorityFixture();
    const exactCases = [
      [fixture.manifest, authority.parseCanonicalBrowserReleaseManifestBytes],
      [fixture.activation, authority.parseCanonicalBrowserReleaseActivationBytes],
      [fixture.rollback, authority.parseCanonicalBrowserReleaseRollbackBytes],
      [fixture.revocation, authority.parseCanonicalBrowserReleaseRevocationBytes],
      [fixture.policy, authority.parseCanonicalBrowserReleaseTrustPolicyBytes],
      [fixture.manifestSignatureSet, authority.parseCanonicalBrowserReleaseSignatureSetBytes],
    ] as const;
    for (const [value, parseCanonical] of exactCases) {
      expect(() =>
        parseCanonical(Buffer.from(JSON.stringify(value, null, 2))),
      ).toThrow();
    }

    const strictCases = [
      [fixture.manifest, authority.parseBrowserReleaseManifest],
      [fixture.activation, authority.parseBrowserReleaseActivation],
      [fixture.rollback, authority.parseBrowserReleaseRollback],
      [fixture.revocation, authority.parseBrowserReleaseRevocation],
      [fixture.policy, authority.parseBrowserReleaseTrustPolicy],
      [fixture.manifestSignatureSet, authority.parseBrowserReleaseSignatureSet],
    ] as const;
    for (const [value, parse] of strictCases) {
      expect(() => parse({ ...value, unexpected: true })).toThrow();
    }
  });

  it("rejects cross-audience and wrong-role signature replay", () => {
    const fixture = authorityFixture();
    expect(() =>
      authority.verifyBrowserReleaseActivation(
        fixture.activationBytes,
        fixture.manifestSignatureSetBytes,
        fixture.policyBytes,
        ACTIVATION_ISSUED_AT_MS,
      ),
    ).toThrowError(/signature set/i);

    const wrongRoleSet = signatureSet(
      "manifest-wrong-role-set",
      fixture.manifestBytes,
      authority.BLUEY_BROWSER_RELEASE_AUDIENCE,
      "release",
      1,
      MANIFEST_PUBLISHED_AT_MS,
      ["promotion-key-1", "promotion-key-2"],
    );
    expect(() =>
      authority.verifyBrowserReleaseManifest(
        fixture.manifestBytes,
        authority.canonicalBrowserReleaseSignatureSetBytes(wrongRoleSet),
        fixture.policyBytes,
        ACTIVATION_ISSUED_AT_MS,
      ),
    ).toThrowError(/role/i);

    const replayedMetadata: authority.BrowserReleaseSignatureSet = {
      ...fixture.manifestSignatureSet,
      signatureSetId: "replayed-as-activation",
      role: "promotion",
      targetAudience: authority.BLUEY_BROWSER_ACTIVATION_AUDIENCE,
      targetSha256: authority.browserReleaseAuthorityTargetSha256(
        fixture.activationBytes,
      ),
      signedAtMs: ACTIVATION_ISSUED_AT_MS,
    };
    expect(() =>
      authority.verifyBrowserReleaseActivation(
        fixture.activationBytes,
        authority.canonicalBrowserReleaseSignatureSetBytes(replayedMetadata),
        fixture.policyBytes,
        ACTIVATION_ISSUED_AT_MS,
      ),
    ).toThrow();
  });

  it("enforces role thresholds exactly", () => {
    const fixture = authorityFixture();
    const insufficient: authority.BrowserReleaseSignatureSet = {
      ...fixture.manifestSignatureSet,
      signatures: fixture.manifestSignatureSet.signatures.slice(0, 1),
    };
    expect(() =>
      authority.verifyBrowserReleaseManifest(
        fixture.manifestBytes,
        authority.canonicalBrowserReleaseSignatureSetBytes(insufficient),
        fixture.policyBytes,
        ACTIVATION_ISSUED_AT_MS,
      ),
    ).toThrowError(/not authorized|threshold/i);
  });

  it("rejects retired release keys even for backdated new authority", () => {
    const fixture = authorityFixture();
    const successor = successorPolicyFixture(fixture.policy);
    const successorBytes = authority.canonicalBrowserReleaseTrustPolicyBytes(
      successor,
    );

    expect(() =>
      authority.verifyBrowserReleaseManifest(
        fixture.manifestBytes,
        fixture.manifestSignatureSetBytes,
        successorBytes,
        SUCCESSOR_ISSUED_AT_MS,
      ),
    ).toThrowError(/not authorized/i);

    const newerManifest = authority.createBrowserReleaseManifest({
      ...fixture.manifest,
      manifestId: "browser-manifest-603-2",
      manifestGeneration: 2,
      releaseSequence: 603_002,
      publishedAtMs: SUCCESSOR_ISSUED_AT_MS + 1,
    });
    const newerManifestBytes = authority.canonicalBrowserReleaseManifestBytes(
      newerManifest,
    );
    const retiredReleaseSet = signatureSet(
      "manifest-603-2-retired-release-set",
      newerManifestBytes,
      authority.BLUEY_BROWSER_RELEASE_AUDIENCE,
      "release",
      2,
      newerManifest.publishedAtMs,
      ["release-key-1", "release-key-2"],
    );
    expect(() =>
      authority.verifyBrowserReleaseManifest(
        newerManifestBytes,
        authority.canonicalBrowserReleaseSignatureSetBytes(retiredReleaseSet),
        successorBytes,
        newerManifest.publishedAtMs,
      ),
    ).toThrowError(/not authorized/i);

    const activation = authority.createBrowserReleaseActivation({
      ...fixture.activation,
      activationId: "browser-activation-beta-2",
      activationGeneration: 2,
      trustGeneration: 2,
      channelSequence: 2,
      issuedAtMs: SUCCESSOR_ISSUED_AT_MS + 1_000,
      expiresAtMs: SUCCESSOR_ISSUED_AT_MS + 10_000,
    });
    const activationBytes = authority.canonicalBrowserReleaseActivationBytes(
      activation,
    );
    const retiredSet = signatureSet(
      "activation-retired-promotion-set",
      activationBytes,
      authority.BLUEY_BROWSER_ACTIVATION_AUDIENCE,
      "promotion",
      2,
      activation.issuedAtMs,
      ["promotion-key-1", "promotion-key-2"],
    );
    expect(() =>
      authority.verifyBrowserReleaseActivation(
        activationBytes,
        authority.canonicalBrowserReleaseSignatureSetBytes(retiredSet),
        successorBytes,
        activation.issuedAtMs,
      ),
    ).toThrowError(/not authorized/i);

    const revoked = authority.createBrowserReleaseTrustPolicy({
      ...successor,
      policyId: "browser-trust-policy-2-revoked-history",
      keys: successor.keys.map((key) =>
        key.keyId === "release-key-1" ? { ...key, state: "revoked" } : key,
      ),
    });
    expect(() =>
      authority.verifyBrowserReleaseManifest(
        fixture.manifestBytes,
        fixture.manifestSignatureSetBytes,
        authority.canonicalBrowserReleaseTrustPolicyBytes(revoked),
        SUCCESSOR_ISSUED_AT_MS,
      ),
    ).toThrowError(/not authorized/i);
  });

  it("requires predecessor and successor root thresholds for rotation", () => {
    const fixture = authorityFixture();
    const successor = successorPolicyFixture(fixture.policy);
    const successorBytes = authority.canonicalBrowserReleaseTrustPolicyBytes(
      successor,
    );
    const dualSet = signatureSet(
      "trust-policy-2-dual-root-set",
      successorBytes,
      authority.BLUEY_BROWSER_TRUST_POLICY_AUDIENCE,
      "root",
      2,
      successor.issuedAtMs,
      ["root-key-1", "root-key-2", "root-key-3", "root-key-4"],
    );
    expect(
      authority.verifyBrowserReleaseTrustPolicy(
        successorBytes,
        authority.canonicalBrowserReleaseSignatureSetBytes(dualSet),
        fixture.policyBytes,
        null,
        successor.issuedAtMs,
      ),
    ).toEqual(successor);

    for (const keyIds of [
      ["root-key-1", "root-key-2"],
      ["root-key-3", "root-key-4"],
    ]) {
      const oneSide = signatureSet(
        `trust-policy-2-${keyIds[0]}-set`,
        successorBytes,
        authority.BLUEY_BROWSER_TRUST_POLICY_AUDIENCE,
        "root",
        2,
        successor.issuedAtMs,
        keyIds,
      );
      expect(() =>
        authority.verifyBrowserReleaseTrustPolicy(
          successorBytes,
          authority.canonicalBrowserReleaseSignatureSetBytes(oneSide),
          fixture.policyBytes,
          null,
          successor.issuedAtMs,
        ),
      ).toThrowError(/threshold/i);
    }
  });

  it("requires predecessor roots to authorize the successor generation", () => {
    const fixture = authorityFixture();
    const boundedPredecessor = authority.createBrowserReleaseTrustPolicy({
      ...fixture.policy,
      policyId: "browser-trust-policy-1-bounded-root",
      keys: fixture.policy.keys.map((key) =>
        key.role === "root" ? { ...key, maximumTrustGeneration: 1 } : key,
      ),
    });
    const successor = successorPolicyFixture(boundedPredecessor);
    const successorBytes = authority.canonicalBrowserReleaseTrustPolicyBytes(
      successor,
    );
    const dualSet = signatureSet(
      "bounded-root-rotation-set",
      successorBytes,
      authority.BLUEY_BROWSER_TRUST_POLICY_AUDIENCE,
      "root",
      2,
      successor.issuedAtMs,
      ["root-key-1", "root-key-2", "root-key-3", "root-key-4"],
    );
    expect(() =>
      authority.verifyBrowserReleaseTrustPolicy(
        successorBytes,
        authority.canonicalBrowserReleaseSignatureSetBytes(dualSet),
        authority.canonicalBrowserReleaseTrustPolicyBytes(boundedPredecessor),
        null,
        successor.issuedAtMs,
      ),
    ).toThrowError(/not authorized|threshold/i);
  });

  it("cannot reuse bootstrap root IDs with different successor keys", () => {
    const fixture = authorityFixture();
    const replacedRoots = authority.createBrowserReleaseTrustPolicy({
      ...fixture.policy,
      policyId: "browser-trust-policy-1-reused-root-ids",
      keys: fixture.policy.keys.map((key) => {
        if (key.keyId === "root-key-1") {
          return { ...key, publicKey: publicKey("root-key-3") };
        }
        if (key.keyId === "root-key-2") {
          return { ...key, publicKey: publicKey("root-key-4") };
        }
        return key;
      }),
    });
    const policyBytes = authority.canonicalBrowserReleaseTrustPolicyBytes(
      replacedRoots,
    );
    const forged = authority.createBrowserReleaseSignatureSet(
      {
        version: 1,
        audience: authority.BLUEY_BROWSER_SIGNATURE_SET_AUDIENCE,
        signatureSetId: "reused-bootstrap-root-ids-set",
        trustGeneration: 1,
        role: "root",
        targetAudience: authority.BLUEY_BROWSER_TRUST_POLICY_AUDIENCE,
        targetSha256: authority.browserReleaseAuthorityTargetSha256(policyBytes),
        signedAtMs: replacedRoots.issuedAtMs,
      },
      [
        { keyId: "root-key-1", privateKey: privateKey("root-key-3") },
        { keyId: "root-key-2", privateKey: privateKey("root-key-4") },
      ],
    );

    expect(() =>
      authority.verifyBrowserReleaseTrustPolicy(
        policyBytes,
        authority.canonicalBrowserReleaseSignatureSetBytes(forged),
        null,
        bootstrapAnchor(),
        replacedRoots.issuedAtMs,
      ),
    ).toThrowError(/rotation/i);
  });

  it("fails closed outside policy and key validity bounds", () => {
    const fixture = authorityFixture();
    expect(() =>
      authority.verifyBrowserReleaseManifest(
        fixture.manifestBytes,
        fixture.manifestSignatureSetBytes,
        fixture.policyBytes,
        POLICY_EXPIRES_AT_MS,
      ),
    ).toThrowError(/validity window/i);
  });

  it("rejects future-dated policy and delegated authority", () => {
    const fixture = authorityFixture();
    expect(() =>
      authority.verifyBrowserReleaseTrustPolicy(
        fixture.policyBytes,
        fixture.policySignatureSetBytes,
        null,
        bootstrapAnchor(),
        POLICY_ISSUED_AT_MS - 1,
      ),
    ).toThrowError(/validity window/i);

    const delegated = [
      () => authority.verifyBrowserReleaseManifest(
        fixture.manifestBytes,
        fixture.manifestSignatureSetBytes,
        fixture.policyBytes,
        MANIFEST_PUBLISHED_AT_MS - 1,
      ),
      () => authority.verifyBrowserReleaseActivation(
        fixture.activationBytes,
        fixture.activationSignatureSetBytes,
        fixture.policyBytes,
        ACTIVATION_ISSUED_AT_MS - 1,
      ),
      () => authority.verifyBrowserReleaseRollback(
        fixture.rollbackBytes,
        fixture.rollbackSignatureSetBytes,
        fixture.policyBytes,
        ROLLBACK_ISSUED_AT_MS - 1,
      ),
      () => authority.verifyBrowserReleaseRevocation(
        fixture.revocationBytes,
        fixture.revocationSignatureSetBytes,
        fixture.policyBytes,
        REVOCATION_ISSUED_AT_MS - 1,
      ),
    ];
    for (const verifyFutureAuthority of delegated) {
      expect(verifyFutureAuthority).toThrowError(/validity window/i);
    }

    const futureSignatureSet = {
      ...fixture.manifestSignatureSet,
      signedAtMs: MANIFEST_PUBLISHED_AT_MS + 1,
    };
    expect(() =>
      authority.verifyBrowserReleaseManifest(
        fixture.manifestBytes,
        authority.canonicalBrowserReleaseSignatureSetBytes(
          futureSignatureSet,
        ),
        fixture.policyBytes,
        MANIFEST_PUBLISHED_AT_MS,
      ),
    ).toThrowError(/validity window/i);
  });

  it("binds every descriptor to the exact matching manifest target", () => {
    const fixture = authorityFixture();
    for (const descriptor of fixture.descriptors) {
      const artifact = authority.requireBrowserDescriptorInManifest(
        descriptor,
        fixture.manifest,
      );
      expect(artifact.platform).toBe(descriptor.platform);
      expect(artifact.architecture).toBe(descriptor.architecture);
    }
    expect(() =>
      authority.requireBrowserDescriptorInManifest(
        descriptorFixture("darwin", "arm64", { buildId: "browser-603.2" }),
        fixture.manifest,
      ),
    ).toThrowError(/bindings/i);
  });
});

function authorityFixture() {
  const descriptors = descriptorsFixture();
  const policy = trustPolicyFixture();
  const policyBytes = authority.canonicalBrowserReleaseTrustPolicyBytes(policy);
  const policySignatureSet = signatureSet(
    "trust-policy-1-root-set",
    policyBytes,
    authority.BLUEY_BROWSER_TRUST_POLICY_AUDIENCE,
    "root",
    1,
    policy.issuedAtMs,
    ["root-key-1", "root-key-2"],
  );
  const manifest = manifestFixture(descriptors);
  const manifestBytes = authority.canonicalBrowserReleaseManifestBytes(manifest);
  const manifestSignatureSet = signatureSet(
    "manifest-603-1-release-set",
    manifestBytes,
    authority.BLUEY_BROWSER_RELEASE_AUDIENCE,
    "release",
    1,
    manifest.publishedAtMs,
    ["release-key-1", "release-key-2"],
  );
  const activation = activationFixture(
    manifest,
    authority.browserReleaseSignatureSetSha256(manifestSignatureSet),
  );
  const activationBytes = authority.canonicalBrowserReleaseActivationBytes(
    activation,
  );
  const activationSignatureSet = signatureSet(
    "activation-beta-1-promotion-set",
    activationBytes,
    authority.BLUEY_BROWSER_ACTIVATION_AUDIENCE,
    "promotion",
    1,
    activation.issuedAtMs,
    ["promotion-key-1", "promotion-key-2"],
  );
  const rollback = rollbackFixture();
  const rollbackBytes = authority.canonicalBrowserReleaseRollbackBytes(rollback);
  const rollbackSignatureSet = signatureSet(
    "rollback-1-promotion-set",
    rollbackBytes,
    authority.BLUEY_BROWSER_ROLLBACK_AUDIENCE,
    "promotion",
    1,
    rollback.issuedAtMs,
    ["promotion-key-1", "promotion-key-2"],
  );
  const revocation = revocationFixture();
  const revocationBytes = authority.canonicalBrowserReleaseRevocationBytes(
    revocation,
  );
  const revocationSignatureSet = signatureSet(
    "revocation-1-incident-set",
    revocationBytes,
    authority.BLUEY_BROWSER_REVOCATION_AUDIENCE,
    "incident",
    1,
    revocation.issuedAtMs,
    ["incident-key-1"],
  );
  return {
    descriptors,
    policy,
    policyBytes,
    policySignatureSet,
    policySignatureSetBytes:
      authority.canonicalBrowserReleaseSignatureSetBytes(policySignatureSet),
    manifest,
    manifestBytes,
    manifestSignatureSet,
    manifestSignatureSetBytes:
      authority.canonicalBrowserReleaseSignatureSetBytes(manifestSignatureSet),
    activation,
    activationBytes,
    activationSignatureSet,
    activationSignatureSetBytes:
      authority.canonicalBrowserReleaseSignatureSetBytes(activationSignatureSet),
    rollback,
    rollbackBytes,
    rollbackSignatureSet,
    rollbackSignatureSetBytes:
      authority.canonicalBrowserReleaseSignatureSetBytes(rollbackSignatureSet),
    revocation,
    revocationBytes,
    revocationSignatureSet,
    revocationSignatureSetBytes:
      authority.canonicalBrowserReleaseSignatureSetBytes(revocationSignatureSet),
  };
}

function descriptorsFixture(): readonly authority.BrowserBuildDescriptor[] {
  return [
    descriptorFixture("darwin", "arm64"),
    descriptorFixture("darwin", "x64"),
    descriptorFixture("windows", "x64"),
  ];
}

function descriptorFixture(
  platform: authority.BrowserReleasePlatform,
  architecture: authority.BrowserReleaseArchitecture,
  overrides: Partial<authority.UnsignedBrowserBuildDescriptor> = {},
): authority.BrowserBuildDescriptor {
  return authority.createBrowserBuildDescriptor(
    {
      version: 1,
      audience: authority.BLUEY_BROWSER_BUILD_AUDIENCE,
      releaseId: "browser-release-603-1",
      buildId: "browser-603.1",
      appVersion: "0.1.0",
      appId: "sh.bluey.jobs.browser",
      protocolVersion: 1,
      sourceCommit: "1".repeat(40),
      platform,
      architecture,
      electronVersion: "43.1.0",
      playwrightVersion: "1.61.1",
      chromiumRevision: "1228",
      issuedAtMs: 1_785_970_000_000,
      signingKeyId: "release-key-1",
      ...overrides,
    },
    privateKey("release-key-1"),
  );
}

function manifestFixture(
  descriptors: readonly authority.BrowserBuildDescriptor[],
): authority.BrowserReleaseManifest {
  const descriptorHashes = descriptors.map((descriptor) =>
    authority.browserBuildDescriptorSha256(descriptor),
  );
  return authority.createBrowserReleaseManifest({
    version: 1,
    audience: authority.BLUEY_BROWSER_RELEASE_AUDIENCE,
    manifestId: "browser-manifest-603-1",
    manifestGeneration: 1,
    releaseId: "browser-release-603-1",
    releaseSequence: 603_001,
    buildId: "browser-603.1",
    appVersion: "0.1.0",
    protocolVersion: 1,
    sourceCommit: "1".repeat(40),
    electronVersion: "43.1.0",
    playwrightVersion: "1.61.1",
    chromiumRevision: "1228",
    publishedAtMs: MANIFEST_PUBLISHED_AT_MS,
    releaseNotesUrl:
      "https://bluey.sh/jobs/browser/releases/browser-release-603-1/RELEASE.md",
    artifacts: [
      artifact("darwin-arm64-dmg", "darwin", "arm64", "darwin-dmg", descriptorHashes[0]!, "a"),
      artifact("darwin-arm64-zip", "darwin", "arm64", "darwin-zip", descriptorHashes[0]!, "b"),
      artifact("darwin-x64-dmg", "darwin", "x64", "darwin-dmg", descriptorHashes[1]!, "c"),
      artifact("darwin-x64-zip", "darwin", "x64", "darwin-zip", descriptorHashes[1]!, "d"),
      artifact("windows-x64-nsis", "windows", "x64", "windows-nsis", descriptorHashes[2]!, "e"),
    ],
  });
}

function artifact(
  suffix: string,
  platform: authority.BrowserReleasePlatform,
  architecture: authority.BrowserReleaseArchitecture,
  packageKind: authority.BrowserReleasePackageKind,
  buildDescriptorSha256: string,
  hashCharacter: string,
): authority.BrowserReleaseArtifact {
  const hashIndex = "abcde".indexOf(hashCharacter);
  if (hashIndex < 0) throw new Error("Unsupported fixture hash character.");
  const appContentHashCharacter = platform === "darwin"
    ? architecture === "arm64" ? "1" : "2"
    : "3";
  const extension = packageKind === "darwin-dmg"
    ? "dmg"
    : packageKind === "darwin-zip"
      ? "zip"
      : "exe";
  return {
    artifactId: `browser-release-603-1-${suffix}`,
    platform,
    architecture,
    packageKind,
    buildDescriptorSha256,
    url: `https://bluey.sh/jobs/browser/releases/browser-release-603-1/${suffix}.${extension}`,
    sizeBytes: 128_000_000 + hashCharacter.charCodeAt(0),
    sha256: hashCharacter.repeat(64),
    appContentSha256: appContentHashCharacter.repeat(64),
    verificationEvidenceSha256: "6789f"[hashIndex]!.repeat(64),
    nativeSignatureKind:
      platform === "darwin" ? "apple-developer-id" : "microsoft-authenticode",
    nativeSignerIdentity: platform === "darwin" ? "TEAMID1234" : "BLUEY LLC",
  };
}

function manifestWithFirstArtifactUrl(
  manifest: authority.BrowserReleaseManifest,
  url: string,
): authority.BrowserReleaseManifest {
  return {
    ...manifest,
    artifacts: manifest.artifacts.map((artifact, index) =>
      index === 0 ? { ...artifact, url } : artifact
    ),
  };
}

function manifestWithReleaseId(
  manifest: authority.BrowserReleaseManifest,
  releaseId: string,
): authority.BrowserReleaseManifest {
  const replaceReleaseId = (url: string): string =>
    url.replace(manifest.releaseId, releaseId);
  return {
    ...manifest,
    releaseId,
    releaseNotesUrl: replaceReleaseId(manifest.releaseNotesUrl),
    artifacts: manifest.artifacts.map((artifact) => ({
      ...artifact,
      url: replaceReleaseId(artifact.url),
    })),
  };
}

function activationFixture(
  manifest: authority.BrowserReleaseManifest,
  signatureSetSha256: string,
): authority.BrowserReleaseActivation {
  return authority.createBrowserReleaseActivation({
    version: 1,
    audience: authority.BLUEY_BROWSER_ACTIVATION_AUDIENCE,
    activationId: "browser-activation-beta-1",
    activationGeneration: 1,
    trustGeneration: 1,
    channel: "beta",
    channelSequence: 1,
    manifestSha256: authority.browserReleaseManifestSha256(manifest),
    signatureSetSha256,
    acceptedServerReleaseIds: ["server-603.1", "server-603.2"],
    canaryEvidenceSha256: "d".repeat(64),
    issuedAtMs: ACTIVATION_ISSUED_AT_MS,
    expiresAtMs: POLICY_EXPIRES_AT_MS - 1,
  });
}

function rollbackFixture(): authority.BrowserReleaseRollback {
  return authority.createBrowserReleaseRollback({
    version: 1,
    audience: authority.BLUEY_BROWSER_ROLLBACK_AUDIENCE,
    rollbackId: "browser-rollback-1",
    rollbackGeneration: 1,
    trustGeneration: 1,
    channel: "beta",
    fromActivationSha256: "e".repeat(64),
    fromManifestSha256: "f".repeat(64),
    toManifestSha256: "0".repeat(64),
    toActivationSha256: "1".repeat(64),
    canaryEvidenceSha256: "2".repeat(64),
    reasonRef: "incident-BR-1",
    issuedAtMs: ROLLBACK_ISSUED_AT_MS,
  });
}

function revocationFixture(): authority.BrowserReleaseRevocation {
  return authority.createBrowserReleaseRevocation({
    version: 1,
    audience: authority.BLUEY_BROWSER_REVOCATION_AUDIENCE,
    revocationId: "browser-revocation-1",
    revocationGeneration: 1,
    trustGeneration: 1,
    subjectKind: "release",
    subjectId: "browser-release-603-1",
    subjectSha256: authority.browserReleaseAuthorityTargetSha256(
      Buffer.from("browser-release-603-1", "utf8"),
    ),
    reasonRef: "incident-BR-2",
    issuedAtMs: REVOCATION_ISSUED_AT_MS,
  });
}

function trustPolicyFixture(): authority.BrowserReleaseTrustPolicy {
  return authority.createBrowserReleaseTrustPolicy({
    version: 1,
    audience: authority.BLUEY_BROWSER_TRUST_POLICY_AUDIENCE,
    policyId: "browser-trust-policy-1",
    trustGeneration: 1,
    predecessorPolicySha256: null,
    artifactOrigin: "https://bluey.sh",
    issuedAtMs: POLICY_ISSUED_AT_MS,
    validFromMs: POLICY_ISSUED_AT_MS - 1_000,
    expiresAtMs: POLICY_EXPIRES_AT_MS,
    roles: roleThresholds(),
    keys: [
      trustKey("incident-key-1", "incident", 1),
      trustKey("promotion-key-1", "promotion", 1),
      trustKey("promotion-key-2", "promotion", 1),
      trustKey("release-key-1", "release", 1),
      trustKey("release-key-2", "release", 1),
      trustKey("root-key-1", "root", 1),
      trustKey("root-key-2", "root", 1),
    ],
  });
}

function successorPolicyFixture(
  predecessor: authority.BrowserReleaseTrustPolicy,
): authority.BrowserReleaseTrustPolicy {
  const retired = new Set([
    "promotion-key-1",
    "promotion-key-2",
    "release-key-1",
    "release-key-2",
    "root-key-1",
    "root-key-2",
  ]);
  const previousKeys = predecessor.keys.map((key) => ({
    ...key,
    state: retired.has(key.keyId) ? "retired" as const : key.state,
  }));
  return authority.createBrowserReleaseTrustPolicy({
    version: 1,
    audience: authority.BLUEY_BROWSER_TRUST_POLICY_AUDIENCE,
    policyId: "browser-trust-policy-2",
    trustGeneration: 2,
    predecessorPolicySha256:
      authority.browserReleaseTrustPolicySha256(predecessor),
    artifactOrigin: predecessor.artifactOrigin,
    issuedAtMs: SUCCESSOR_ISSUED_AT_MS,
    validFromMs: SUCCESSOR_ISSUED_AT_MS,
    expiresAtMs: POLICY_EXPIRES_AT_MS,
    roles: roleThresholds(),
    keys: [
      ...previousKeys,
      trustKey("promotion-key-3", "promotion", 2),
      trustKey("promotion-key-4", "promotion", 2),
      trustKey("release-key-3", "release", 2),
      trustKey("release-key-4", "release", 2),
      trustKey("root-key-3", "root", 2),
      trustKey("root-key-4", "root", 2),
    ].sort((left, right) => left.keyId.localeCompare(right.keyId)),
  });
}

function roleThresholds(): readonly authority.BrowserReleaseTrustRole[] {
  return [
    { role: "incident", threshold: 1 },
    { role: "promotion", threshold: 2 },
    { role: "release", threshold: 2 },
    { role: "root", threshold: 2 },
  ];
}

function trustKey(
  keyId: keyof typeof keyIndexes,
  role: authority.BrowserReleaseAuthorityRole,
  minimumTrustGeneration: number,
): authority.BrowserReleaseTrustKey {
  return {
    keyId,
    role,
    publicKey: publicKey(keyId),
    state: "active",
    validFromMs: POLICY_ISSUED_AT_MS - 1_000,
    validUntilMs: POLICY_EXPIRES_AT_MS - 1,
    minimumTrustGeneration,
    maximumTrustGeneration: minimumTrustGeneration + 1,
  };
}

function signatureSet(
  signatureSetId: string,
  targetBytes: Uint8Array,
  targetAudience: authority.BrowserReleaseSignedAudience,
  role: authority.BrowserReleaseAuthorityRole,
  trustGeneration: number,
  signedAtMs: number,
  keyIds: readonly string[],
): authority.BrowserReleaseSignatureSet {
  return authority.createBrowserReleaseSignatureSet(
    {
      version: 1,
      audience: authority.BLUEY_BROWSER_SIGNATURE_SET_AUDIENCE,
      signatureSetId,
      trustGeneration,
      role,
      targetAudience,
      targetSha256:
        authority.browserReleaseAuthorityTargetSha256(targetBytes),
      signedAtMs,
    },
    keyIds.map((keyId) => ({ keyId, privateKey: privateKey(keyId) })),
  );
}

function bootstrapAnchor(): authority.BrowserRootTrustAnchor {
  return {
    threshold: 2,
    keys: {
      "root-key-1": publicKey("root-key-1"),
      "root-key-2": publicKey("root-key-2"),
    },
  };
}

function privateKey(keyId: string): KeyObject {
  const index = keyIndexes[keyId as keyof typeof keyIndexes];
  if (index === undefined) throw new Error(`unknown test key: ${keyId}`);
  const prefix = Buffer.from("302e020100300506032b657004220420", "hex");
  const seed = Buffer.from(
    Array.from({ length: 32 }, (_, offset) => (index * 37 + offset) % 256),
  );
  return createPrivateKey({
    key: Buffer.concat([prefix, seed]),
    format: "der",
    type: "pkcs8",
  });
}

function publicKey(keyId: string): string {
  const exported = createPublicKey(privateKey(keyId)).export({ format: "jwk" });
  if (!exported.x) throw new Error("test key has no Ed25519 public x value");
  return exported.x;
}

function artifactTarget(artifact: authority.BrowserReleaseArtifact): string {
  return `${artifact.platform}:${artifact.architecture}:${artifact.packageKind}`;
}

function sharedVector(): Record<string, unknown> {
  const fixture = authorityFixture();
  const descriptor = fixture.descriptors[0]!;
  const proof = authority.browserBuildProof(descriptor);
  return {
    descriptor: proof.descriptor,
    signature: proof.signature,
    publicKey: publicKey("release-key-1"),
    descriptorSha256: authority.browserBuildDescriptorSha256(descriptor),
    manifest: vectorEntry(
      fixture.manifestBytes,
      authority.browserReleaseManifestSha256(fixture.manifest),
      fixture.manifestSignatureSetBytes,
      authority.browserReleaseSignatureSetSha256(
        fixture.manifestSignatureSet,
      ),
    ),
    activation: vectorEntry(
      fixture.activationBytes,
      authority.browserReleaseActivationSha256(fixture.activation),
      fixture.activationSignatureSetBytes,
      authority.browserReleaseSignatureSetSha256(
        fixture.activationSignatureSet,
      ),
    ),
    rollback: vectorEntry(
      fixture.rollbackBytes,
      authority.browserReleaseRollbackSha256(fixture.rollback),
      fixture.rollbackSignatureSetBytes,
      authority.browserReleaseSignatureSetSha256(
        fixture.rollbackSignatureSet,
      ),
    ),
    revocation: vectorEntry(
      fixture.revocationBytes,
      authority.browserReleaseRevocationSha256(fixture.revocation),
      fixture.revocationSignatureSetBytes,
      authority.browserReleaseSignatureSetSha256(
        fixture.revocationSignatureSet,
      ),
    ),
    trustPolicy: vectorEntry(
      fixture.policyBytes,
      authority.browserReleaseTrustPolicySha256(fixture.policy),
      fixture.policySignatureSetBytes,
      authority.browserReleaseSignatureSetSha256(
        fixture.policySignatureSet,
      ),
    ),
  };
}

function vectorEntry(
  canonicalBytes: Uint8Array,
  sha256: string,
  signatureSetBytes: Uint8Array,
  signatureSetSha256: string,
): Record<string, string> {
  return {
    canonical: Buffer.from(canonicalBytes).toString("base64url"),
    sha256,
    signatureSet: Buffer.from(signatureSetBytes).toString("base64url"),
    signatureSetSha256,
  };
}
