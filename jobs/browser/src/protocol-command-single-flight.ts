import { createHash } from "node:crypto";

const MAX_PROTOCOL_URL_LENGTH = 8_192;
const MAX_IN_FLIGHT_PROTOCOL_COMMANDS = 32;
const PROTOCOL_PREFIX = "bluey-jobs://";

export class ProtocolCommandSingleFlight {
  private readonly inFlight = new Map<string, Promise<unknown>>();

  run<T>(
    rawUrl: string,
    operation: () => Promise<T>,
    reject: () => T | Promise<T>,
  ): Promise<T> {
    if (typeof rawUrl !== "string"
      || rawUrl.length === 0
      || rawUrl.length > MAX_PROTOCOL_URL_LENGTH
      || !rawUrl.startsWith(PROTOCOL_PREFIX)) {
      return Promise.resolve().then(reject);
    }
    const key = createHash("sha256").update(rawUrl, "utf8").digest("hex");
    const existing = this.inFlight.get(key);
    if (existing) return existing as Promise<T>;
    if (this.inFlight.size >= MAX_IN_FLIGHT_PROTOCOL_COMMANDS) {
      return Promise.resolve().then(reject);
    }

    let tracked!: Promise<T>;
    tracked = Promise.resolve().then(operation).then(
      (value) => {
        this.release(key, tracked);
        return value;
      },
      (error) => {
        this.release(key, tracked);
        throw error;
      },
    );
    this.inFlight.set(key, tracked);
    return tracked;
  }

  private release(key: string, promise: Promise<unknown>): void {
    if (this.inFlight.get(key) === promise) this.inFlight.delete(key);
  }
}

export const PROTOCOL_COMMAND_IN_FLIGHT_CAP = MAX_IN_FLIGHT_PROTOCOL_COMMANDS;
