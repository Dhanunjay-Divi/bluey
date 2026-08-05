const PROFILE_SCOPE = /^[a-f0-9]{40}$/;

export class ProfileRecoveryBlockedError extends Error {
  readonly code = "profile_recovery_blocked";

  constructor() {
    super("Runner recovery is pending for this browser profile.");
    this.name = "ProfileRecoveryBlockedError";
  }
}

/** Keeps checkpoint recovery failures local to their encrypted profile scope. */
export class ProfileRecoveryIsolation {
  readonly #blockedScopes = new Set<string>();

  block(profileScope: string): void {
    assertProfileScope(profileScope);
    this.#blockedScopes.add(profileScope);
  }

  isBlocked(profileScope: string): boolean {
    assertProfileScope(profileScope);
    return this.#blockedScopes.has(profileScope);
  }

  async attemptStartup(
    profileScope: string,
    recover: () => Promise<void>,
  ): Promise<boolean> {
    assertProfileScope(profileScope);
    try {
      await recover();
      this.#blockedScopes.delete(profileScope);
      return true;
    } catch {
      this.#blockedScopes.add(profileScope);
      return false;
    }
  }

  async requireMutation(
    profileScope: string,
    retry: () => Promise<void>,
  ): Promise<void> {
    assertProfileScope(profileScope);
    if (!this.#blockedScopes.has(profileScope)) return;
    try {
      await retry();
      this.#blockedScopes.delete(profileScope);
    } catch {
      this.#blockedScopes.add(profileScope);
      throw new ProfileRecoveryBlockedError();
    }
  }

  rejectMutation(profileScope: string): never {
    this.block(profileScope);
    throw new ProfileRecoveryBlockedError();
  }
}

function assertProfileScope(profileScope: string): void {
  if (!PROFILE_SCOPE.test(profileScope)) throw new ProfileRecoveryBlockedError();
}
