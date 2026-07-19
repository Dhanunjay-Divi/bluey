export type AdmissionBlockReason = "offline" | "paused" | "stop_requested" | "device_unavailable";

export class LocalRunAdmission {
  private paused = false;
  private stopRequested = false;
  private deviceAvailable = true;

  setPaused(paused: boolean): void {
    this.paused = paused;
    if (!paused) this.stopRequested = false;
  }

  requestStop(): void {
    this.stopRequested = true;
    this.paused = true;
  }

  setDeviceAvailable(available: boolean): void {
    this.deviceAvailable = available;
  }

  decision(online: boolean): { allowed: true } | { allowed: false; reason: AdmissionBlockReason } {
    if (!online) return { allowed: false, reason: "offline" };
    if (!this.deviceAvailable) return { allowed: false, reason: "device_unavailable" };
    if (this.stopRequested) return { allowed: false, reason: "stop_requested" };
    if (this.paused) return { allowed: false, reason: "paused" };
    return { allowed: true };
  }

  get isPaused(): boolean {
    return this.paused;
  }

  get hasStopRequest(): boolean {
    return this.stopRequested;
  }
}
