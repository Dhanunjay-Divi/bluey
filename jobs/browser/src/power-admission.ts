export type PowerAdmissionReason = "device_unavailable";

/**
 * Tracks only OS states Electron can state explicitly. Electron does not expose
 * a trustworthy remaining-battery percentage, so this deliberately makes no
 * "critical battery" claim and never blocks merely because AC power is absent.
 */
export class PowerAdmissionGuard {
  private suspended = false;
  private locked = false;

  setSuspended(suspended: boolean): void {
    this.suspended = suspended;
  }

  setLocked(locked: boolean): void {
    this.locked = locked;
  }

  decision(): { allowed: true } | { allowed: false; reason: PowerAdmissionReason } {
    return this.suspended || this.locked
      ? { allowed: false, reason: "device_unavailable" }
      : { allowed: true };
  }
}
