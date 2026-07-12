import { createHash } from "node:crypto";
import { join } from "node:path";

export function accountProfileKey(accountId: string): string {
  return digest(accountId);
}

export function identityProfileKey(applicationIdentityId: string): string {
  return digest(applicationIdentityId);
}

export function accountProfileDirectory(baseDirectory: string, accountId: string): string {
  return join(baseDirectory, "profiles", accountProfileKey(accountId));
}

export function identityProfileDirectory(
  baseDirectory: string,
  accountId: string,
  applicationIdentityId: string,
): string {
  return join(
    accountProfileDirectory(baseDirectory, accountId),
    "identities",
    identityProfileKey(applicationIdentityId),
  );
}

export function identityContextKey(accountId: string, applicationIdentityId: string): string {
  return `${accountProfileKey(accountId)}:${identityProfileKey(applicationIdentityId)}`;
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 24);
}
