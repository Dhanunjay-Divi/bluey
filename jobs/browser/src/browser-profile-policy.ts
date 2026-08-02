import { isAbsolute, parse, relative, resolve, sep } from "node:path";
import {
  accountProfileKey,
  identityContextKey,
  identityProfileKey,
} from "./profile.js";

const BROWSER_PROFILE_ID_PATTERN = /^[a-f0-9]{24}:[a-f0-9]{24}$/;
const CONTROL_CHARACTER_PATTERN = /[\u0000-\u001f\u007f]/u;
const MAX_IDENTITY_BYTES = 1_024;

declare const browserProfileIdBrand: unique symbol;

export type BrowserProfileId = string & {
  readonly [browserProfileIdBrand]: "BrowserProfileId";
};

export type BrowserContextPolicyErrorCode =
  | "browser_context_already_bound"
  | "browser_context_binding_mismatch"
  | "browser_profile_binding_collision"
  | "browser_profile_path_escape"
  | "invalid_browser_identity"
  | "invalid_browser_profile_id"
  | "invalid_browser_profile_root";

export interface BrowserIdentityScope {
  readonly accountId: string;
  readonly applicationIdentityId: string;
}

export interface BrowserProfilePaths {
  readonly browserProfileId: BrowserProfileId;
  readonly profileDirectory: string;
  readonly chromiumUserDataDirectory: string;
}

export interface BrowserProfilePolicy {
  profileFor(identity: BrowserIdentityScope): BrowserProfilePaths;
}

export interface RegisteredBrowserContext<TContext extends object>
  extends BrowserProfilePaths {
  readonly context: TContext;
}

export class BrowserContextPolicyError extends Error {
  constructor(readonly code: BrowserContextPolicyErrorCode) {
    super(code);
    this.name = "BrowserContextPolicyError";
  }
}

export function browserIdentityScope(
  accountId: string,
  applicationIdentityId: string,
): BrowserIdentityScope {
  return checkedIdentity({ accountId, applicationIdentityId });
}

export function browserProfileIdForIdentity(
  identity: BrowserIdentityScope,
): BrowserProfileId {
  const checked = checkedIdentity(identity);
  // Byte-for-byte compatible with the server-owned execution profile ID.
  return identityContextKey(
    checked.accountId,
    checked.applicationIdentityId,
  ) as BrowserProfileId;
}

export class BrowserProfilePathPolicy implements BrowserProfilePolicy {
  readonly rootDirectory: string;

  constructor(rootDirectory: string) {
    this.rootDirectory = checkedRoot(rootDirectory);
  }

  profileFor(identity: BrowserIdentityScope): BrowserProfilePaths {
    const checked = checkedIdentity(identity);
    // Preserve the original profile layout so an upgrade cannot strand saved
    // employer sessions or create a second profile for one identity.
    const accountDirectoryId = accountProfileKey(checked.accountId);
    const identityDirectoryId = identityProfileKey(checked.applicationIdentityId);
    const profileDirectory = this.descendant(
      "profiles",
      accountDirectoryId,
      "identities",
      identityDirectoryId,
    );
    return Object.freeze({
      browserProfileId: browserProfileIdForIdentity(checked),
      profileDirectory,
      chromiumUserDataDirectory: this.descendant(
        "profiles",
        accountDirectoryId,
        "identities",
        identityDirectoryId,
        "chromium-profile",
      ),
    });
  }

  private descendant(...segments: readonly string[]): string {
    const candidate = resolve(this.rootDirectory, ...segments);
    const child = relative(this.rootDirectory, candidate);
    if (
      child.length === 0
      || child === ".."
      || child.startsWith(`..${sep}`)
      || isAbsolute(child)
    ) {
      throw new BrowserContextPolicyError("browser_profile_path_escape");
    }
    return candidate;
  }
}

interface StoredBrowserContext<TContext extends object> {
  readonly identity: BrowserIdentityScope;
  readonly registration: RegisteredBrowserContext<TContext>;
}

export class IdentityScopedBrowserContextRegistry<TContext extends object> {
  private readonly byProfileId = new Map<
    BrowserProfileId,
    StoredBrowserContext<TContext>
  >();
  private readonly profileIdByContext = new Map<TContext, BrowserProfileId>();

  constructor(private readonly policy: BrowserProfilePolicy) {}

  get size(): number {
    return this.byProfileId.size;
  }

  profileFor(identity: BrowserIdentityScope): BrowserProfilePaths {
    return this.checkedProfile(identity).profile;
  }

  get(identity: BrowserIdentityScope): TContext | undefined {
    const { checked, profile } = this.checkedProfile(identity);
    const stored = this.byProfileId.get(profile.browserProfileId);
    if (!stored) return undefined;
    this.assertSameIdentity(stored.identity, checked);
    return stored.registration.context;
  }

  bind(
    identity: BrowserIdentityScope,
    context: TContext,
  ): RegisteredBrowserContext<TContext> {
    const { checked, profile } = this.checkedProfile(identity);
    const existing = this.byProfileId.get(profile.browserProfileId);
    if (existing) {
      this.assertSameIdentity(existing.identity, checked);
      if (existing.registration.context !== context) {
        throw new BrowserContextPolicyError("browser_context_already_bound");
      }
      return existing.registration;
    }

    const contextProfileId = this.profileIdByContext.get(context);
    if (contextProfileId && contextProfileId !== profile.browserProfileId) {
      throw new BrowserContextPolicyError("browser_context_binding_mismatch");
    }

    const registration = Object.freeze({ ...profile, context });
    this.byProfileId.set(profile.browserProfileId, { identity: checked, registration });
    this.profileIdByContext.set(context, profile.browserProfileId);
    return registration;
  }

  release(identity: BrowserIdentityScope, expectedContext?: TContext): boolean {
    const { checked, profile } = this.checkedProfile(identity);
    const stored = this.byProfileId.get(profile.browserProfileId);
    if (!stored) return false;
    this.assertSameIdentity(stored.identity, checked);
    if (expectedContext && stored.registration.context !== expectedContext) return false;
    this.byProfileId.delete(profile.browserProfileId);
    if (this.profileIdByContext.get(stored.registration.context) === profile.browserProfileId) {
      this.profileIdByContext.delete(stored.registration.context);
    }
    return true;
  }

  registrations(): readonly RegisteredBrowserContext<TContext>[] {
    return Object.freeze(
      [...this.byProfileId.values()].map(({ registration }) => registration),
    );
  }

  clear(): void {
    this.byProfileId.clear();
    this.profileIdByContext.clear();
  }

  private checkedProfile(identity: BrowserIdentityScope): {
    checked: BrowserIdentityScope;
    profile: BrowserProfilePaths;
  } {
    const checked = checkedIdentity(identity);
    const profile = this.policy.profileFor(checked);
    if (!BROWSER_PROFILE_ID_PATTERN.test(profile.browserProfileId)) {
      throw new BrowserContextPolicyError("invalid_browser_profile_id");
    }
    return { checked, profile };
  }

  private assertSameIdentity(
    stored: BrowserIdentityScope,
    requested: BrowserIdentityScope,
  ): void {
    if (
      stored.accountId !== requested.accountId
      || stored.applicationIdentityId !== requested.applicationIdentityId
    ) {
      throw new BrowserContextPolicyError("browser_profile_binding_collision");
    }
  }
}

function checkedIdentity(identity: BrowserIdentityScope): BrowserIdentityScope {
  if (
    !validIdentityValue(identity?.accountId)
    || !validIdentityValue(identity?.applicationIdentityId)
  ) {
    throw new BrowserContextPolicyError("invalid_browser_identity");
  }
  return Object.freeze({
    accountId: identity.accountId,
    applicationIdentityId: identity.applicationIdentityId,
  });
}

function validIdentityValue(value: unknown): value is string {
  return typeof value === "string"
    && value.length > 0
    && value.trim() === value
    && !CONTROL_CHARACTER_PATTERN.test(value)
    && Buffer.byteLength(value, "utf8") <= MAX_IDENTITY_BYTES;
}

function checkedRoot(rootDirectory: string): string {
  if (
    typeof rootDirectory !== "string"
    || rootDirectory.length === 0
    || CONTROL_CHARACTER_PATTERN.test(rootDirectory)
    || !isAbsolute(rootDirectory)
  ) {
    throw new BrowserContextPolicyError("invalid_browser_profile_root");
  }
  const rootPrefix = parse(rootDirectory).root;
  const suppliedSegments = rootDirectory.slice(rootPrefix.length).split(sep);
  if (suppliedSegments.some((segment) => segment === "." || segment === "..")) {
    throw new BrowserContextPolicyError("invalid_browser_profile_root");
  }
  const checked = resolve(rootDirectory);
  if (checked === parse(checked).root) {
    throw new BrowserContextPolicyError("invalid_browser_profile_root");
  }
  return checked;
}
