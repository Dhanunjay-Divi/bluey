import { isAbsolute, parse, relative, resolve, sep } from "node:path";
import { describe, expect, it } from "vitest";
import {
  BrowserContextPolicyError,
  BrowserProfilePathPolicy,
  IdentityScopedBrowserContextRegistry,
  browserIdentityScope,
  browserProfileIdForIdentity,
  type BrowserProfilePolicy,
} from "../src/browser-profile-policy.js";
import { identityProfileDirectory } from "../src/profile.js";

const profileRoot = resolve("/tmp", "bluey-jobs-browser-registry-tests");

describe("browser profile identity policy", () => {
  it("derives a stable opaque receipt identifier from account and application identity", () => {
    const identity = browserIdentityScope("account-123", "identity-456");
    const profileId = browserProfileIdForIdentity(identity);

    expect(profileId).toBe(browserProfileIdForIdentity(identity));
    expect(profileId).toBe("725a2fd11a2b525124e641d2:7e2c54f91b4005dffe48f133");
    expect(profileId).toMatch(/^[a-f0-9]{24}:[a-f0-9]{24}$/);
    expect(profileId).not.toContain(identity.accountId);
    expect(profileId).not.toContain(identity.applicationIdentityId);
  });

  it("separates every account and application identity tuple", () => {
    const original = browserProfileIdForIdentity(browserIdentityScope("account-a", "identity-a"));
    const otherAccount = browserProfileIdForIdentity(
      browserIdentityScope("account-b", "identity-a"),
    );
    const otherIdentity = browserProfileIdForIdentity(
      browserIdentityScope("account-a", "identity-b"),
    );
    const ambiguousLeft = browserProfileIdForIdentity(browserIdentityScope("ab", "c"));
    const ambiguousRight = browserProfileIdForIdentity(browserIdentityScope("a", "bc"));

    expect(new Set([
      original,
      otherAccount,
      otherIdentity,
      ambiguousLeft,
      ambiguousRight,
    ]).size).toBe(5);
  });

  it("never places raw emails, names, or traversal-shaped identity values in paths", () => {
    const accountId = "owner@example.com/../../other-account";
    const applicationIdentityId = "Jane Applicant <jobs+primary@example.com>/../secondary";
    const paths = new BrowserProfilePathPolicy(profileRoot).profileFor(
      browserIdentityScope(accountId, applicationIdentityId),
    );

    expect(isAbsolute(paths.profileDirectory)).toBe(true);
    expect(isDescendant(profileRoot, paths.profileDirectory)).toBe(true);
    expect(isDescendant(profileRoot, paths.chromiumUserDataDirectory)).toBe(true);
    expect(paths.chromiumUserDataDirectory).not.toContain(accountId);
    expect(paths.chromiumUserDataDirectory).not.toContain(applicationIdentityId);
    expect(paths.chromiumUserDataDirectory).not.toContain("owner@example.com");
    expect(paths.chromiumUserDataDirectory).not.toContain("Jane Applicant");
    expect(relative(profileRoot, paths.chromiumUserDataDirectory).split(sep)).not.toContain("..");
  });

  it("keeps path scopes deterministic and account-sensitive", () => {
    const policy = new BrowserProfilePathPolicy(`${profileRoot}${sep}`);
    const firstIdentity = browserIdentityScope("account-a", "shared-identity");
    const secondIdentity = browserIdentityScope("account-b", "shared-identity");

    expect(policy.profileFor(firstIdentity)).toEqual(policy.profileFor(firstIdentity));
    expect(policy.profileFor(firstIdentity).profileDirectory).not.toBe(
      policy.profileFor(secondIdentity).profileDirectory,
    );
  });

  it("preserves the existing identity profile directory across upgrades", () => {
    const identity = browserIdentityScope("account-a", "identity-a");
    const paths = new BrowserProfilePathPolicy(profileRoot).profileFor(identity);
    const existingDirectory = identityProfileDirectory(
      profileRoot,
      identity.accountId,
      identity.applicationIdentityId,
    );

    expect(paths.profileDirectory).toBe(existingDirectory);
    expect(paths.chromiumUserDataDirectory).toBe(
      resolve(existingDirectory, "chromium-profile"),
    );
  });

  it("rejects unsafe roots without echoing them", () => {
    const secretRoot = `relative${sep}person@example.com${sep}..${sep}profiles`;

    expect(() => new BrowserProfilePathPolicy(secretRoot)).toThrow(
      "invalid_browser_profile_root",
    );
    expect(() => new BrowserProfilePathPolicy(parse(profileRoot).root)).toThrow(
      "invalid_browser_profile_root",
    );
    expect(() => new BrowserProfilePathPolicy(`${profileRoot}${sep}..${sep}profiles`)).toThrow(
      "invalid_browser_profile_root",
    );
    try {
      new BrowserProfilePathPolicy(secretRoot);
    } catch (error) {
      expect(String(error)).not.toContain("person@example.com");
    }
  });

  it("rejects empty and control-bearing identities without leaking values", () => {
    expect(() => browserIdentityScope("account", " ")).toThrow("invalid_browser_identity");
    const secret = "private@example.com\nsecond-line";
    try {
      browserIdentityScope("account", secret);
    } catch (error) {
      expect(error).toBeInstanceOf(BrowserContextPolicyError);
      expect(String(error)).not.toContain("private@example.com");
    }
  });
});

describe("identity-scoped browser context registry", () => {
  it("reuses only the context bound to the exact account and identity", () => {
    const registry = registryForTests();
    const identity = browserIdentityScope("account-a", "identity-a");
    const context = { id: "context-a" };

    const first = registry.bind(identity, context);
    expect(registry.bind(identity, context)).toBe(first);
    expect(registry.get(identity)).toBe(context);
    expect(registry.size).toBe(1);
  });

  it("keeps the same identity identifier isolated across accounts", () => {
    const registry = registryForTests();
    const firstIdentity = browserIdentityScope("account-a", "identity-shared");
    const secondIdentity = browserIdentityScope("account-b", "identity-shared");
    const firstContext = { id: "context-a" };
    const secondContext = { id: "context-b" };

    registry.bind(firstIdentity, firstContext);
    registry.bind(secondIdentity, secondContext);

    expect(registry.get(firstIdentity)).toBe(firstContext);
    expect(registry.get(secondIdentity)).toBe(secondContext);
    expect(registry.size).toBe(2);
  });

  it("fails closed when a profile or context is rebound", () => {
    const registry = registryForTests();
    const firstIdentity = browserIdentityScope("account-a", "identity-a");
    const secondIdentity = browserIdentityScope("account-a", "identity-b");
    const firstContext = { id: "context-a" };

    registry.bind(firstIdentity, firstContext);
    expect(() => registry.bind(firstIdentity, { id: "replacement" })).toThrow(
      "browser_context_already_bound",
    );
    expect(() => registry.bind(secondIdentity, firstContext)).toThrow(
      "browser_context_binding_mismatch",
    );
  });

  it("detects a policy-level profile collision before contexts can cross identities", () => {
    const safePolicy = new BrowserProfilePathPolicy(profileRoot);
    const firstIdentity = browserIdentityScope("account-a", "identity-a");
    const secondIdentity = browserIdentityScope("account-b", "identity-b");
    const collidingProfile = safePolicy.profileFor(firstIdentity);
    const collidingPolicy: BrowserProfilePolicy = {
      profileFor: () => collidingProfile,
    };
    const registry = new IdentityScopedBrowserContextRegistry<object>(collidingPolicy);

    registry.bind(firstIdentity, { id: "context-a" });
    expect(() => registry.bind(secondIdentity, { id: "context-b" })).toThrow(
      "browser_profile_binding_collision",
    );
    expect(registry.size).toBe(1);
  });

  it("uses expected-context release fencing and exposes no raw identity in snapshots", () => {
    const registry = registryForTests();
    const identity = browserIdentityScope("owner@example.com", "Jane Applicant");
    const context = { id: "context-a" };
    registry.bind(identity, context);

    expect(registry.release(identity, { id: "stale-context" })).toBe(false);
    expect(registry.get(identity)).toBe(context);
    const [registration] = registry.registrations();
    expect(JSON.stringify(registration)).not.toContain("owner@example.com");
    expect(JSON.stringify(registration)).not.toContain("Jane Applicant");
    expect(registry.release(identity, context)).toBe(true);
    expect(registry.get(identity)).toBeUndefined();
  });
});

function registryForTests(): IdentityScopedBrowserContextRegistry<{ id: string }> {
  return new IdentityScopedBrowserContextRegistry(new BrowserProfilePathPolicy(profileRoot));
}

function isDescendant(root: string, candidate: string): boolean {
  const child = relative(root, candidate);
  return child.length > 0
    && child !== ".."
    && !child.startsWith(`..${sep}`)
    && !isAbsolute(child);
}
