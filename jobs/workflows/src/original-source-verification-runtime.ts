import { createHash } from "node:crypto";

import {
  OriginalSourceVerifier,
  canonicalOriginalSourceJson,
  originalSourceFailureObservation,
  originalSourceSha256,
  parseOriginalSourceVerificationSubject,
  type OriginalSourceVerificationFailureCode,
  type OriginalSourceVerificationObservation,
} from "@bluey/jobs-automation/original-source-verification";
import type { ManagedCloudRuntimeInstance } from "@bluey/jobs-automation/managed-cloud-runtime-client";

import {
  OriginalSourceVerificationApiError,
  type OriginalSourceVerificationLease,
  type OriginalSourceVerificationPublishedObservation,
  type OriginalSourceVerificationWorkerApi,
  type OriginalSourceVerifierBinding,
} from "./original-source-verification-api.js";

export const DEFAULT_ORIGINAL_SOURCE_POLL_INTERVAL_MS = 5_000;
export const DEFAULT_ORIGINAL_SOURCE_LEASE_HEARTBEAT_INTERVAL_MS = 15_000;

const RUNTIME_AUTHORITY_AUDIENCE =
  "bluey-jobs-original-source-verifier-runtime-authority-v1";

export type OriginalSourceVerificationPollOutcome =
  | "completed"
  | "failed"
  | "idle"
  | "lease_lost"
  | "replayed";

export type OriginalSourceVerificationWorkerLogEvent =
  | {
      event: "original_source_verification_poll_failed";
      error_code:
        | "api_rejected"
        | "api_timeout"
        | "api_unavailable"
        | "authority_unavailable"
        | "conflict"
        | "invalid_response"
        | "not_found"
        | "worker_error";
    }
  | {
      event: "original_source_verification_reported";
      outcome: "complete" | "failed" | "lease_lost";
      assignment_fingerprint: string;
      result?: string;
      error_code?: OriginalSourceVerificationFailureCode;
      replayed?: boolean;
    };

export interface OriginalSourceVerificationWorkerLogger {
  log(event: OriginalSourceVerificationWorkerLogEvent): void;
}

export interface OriginalSourceVerifierRuntimeOptions {
  api: OriginalSourceVerificationWorkerApi;
  verifier?: OriginalSourceVerifier;
  logger?: OriginalSourceVerificationWorkerLogger;
  pollIntervalMs?: number;
  leaseHeartbeatIntervalMs?: number;
  workerRuntimeIdentitySha256: string;
  sleep?: (milliseconds: number, signal: AbortSignal) => Promise<void>;
}

const consoleLogger: OriginalSourceVerificationWorkerLogger = {
  log: (event) => console.log(JSON.stringify(event)),
};

export class OriginalSourceVerifierRuntime {
  readonly pollIntervalMs: number;
  readonly leaseHeartbeatIntervalMs: number;

  private readonly api: OriginalSourceVerificationWorkerApi;
  private readonly verifier: OriginalSourceVerifier;
  private readonly logger: OriginalSourceVerificationWorkerLogger;
  private readonly sleep: (milliseconds: number, signal: AbortSignal) => Promise<void>;
  private readonly workerRuntimeIdentitySha256: string;
  private readonly stopController = new AbortController();
  private lastPollCompletedAtMs = 0;
  private lastPollFailedAtMs = 0;
  private stopping = false;

  constructor(options: OriginalSourceVerifierRuntimeOptions) {
    this.api = options.api;
    this.verifier = options.verifier ?? new OriginalSourceVerifier();
    this.logger = options.logger ?? consoleLogger;
    this.workerRuntimeIdentitySha256 = requiredSha256(
      options.workerRuntimeIdentitySha256,
      "worker runtime identity",
    );
    this.pollIntervalMs = boundedInteger(
      options.pollIntervalMs ?? DEFAULT_ORIGINAL_SOURCE_POLL_INTERVAL_MS,
      250,
      60_000,
      "poll interval",
    );
    this.leaseHeartbeatIntervalMs = boundedInteger(
      options.leaseHeartbeatIntervalMs
        ?? DEFAULT_ORIGINAL_SOURCE_LEASE_HEARTBEAT_INTERVAL_MS,
      100,
      30_000,
      "lease heartbeat interval",
    );
    this.sleep = options.sleep ?? interruptibleSleep;
  }

  async run(signal?: AbortSignal): Promise<void> {
    const stop = (): void => this.stop();
    if (signal?.aborted) this.stop();
    else signal?.addEventListener("abort", stop, { once: true });
    try {
      while (!this.stopping) {
        try {
          await this.establishReadiness();
        } catch (error) {
          this.logger.log({
            event: "original_source_verification_poll_failed",
            error_code: pollErrorCode(error),
          });
        }
        if (!this.stopping) await this.sleep(this.pollIntervalMs, this.stopController.signal);
      }
    } finally {
      signal?.removeEventListener("abort", stop);
    }
  }

  stop(): void {
    if (this.stopping) return;
    this.stopping = true;
    this.stopController.abort();
  }

  managedCloudReady(nowMs: number = Date.now()): boolean {
    const maximumAgeMs = Math.max(15_000, this.pollIntervalMs * 3);
    return this.lastPollCompletedAtMs > this.lastPollFailedAtMs
      && nowMs - this.lastPollCompletedAtMs <= maximumAgeMs;
  }

  async establishReadiness(): Promise<OriginalSourceVerificationPollOutcome> {
    try {
      const outcome = await this.pollOnce();
      this.lastPollCompletedAtMs = Date.now();
      return outcome;
    } catch (error) {
      this.lastPollFailedAtMs = Date.now();
      throw error;
    }
  }

  async pollOnce(): Promise<OriginalSourceVerificationPollOutcome> {
    const lease = await this.api.lease();
    if (!lease) return "idle";
    const fingerprint = assignmentFingerprint(lease);
    let subject;
    let rawSubject: unknown;
    try {
      if (originalSourceSha256(lease.canonical_subject_json) !== lease.subject_sha256) {
        throw new Error("subject digest changed");
      }
      rawSubject = JSON.parse(lease.canonical_subject_json) as unknown;
      subject = parseOriginalSourceVerificationSubject(rawSubject);
    } catch {
      const observation = this.publishObservation(
        originalSourceFailureObservation(rawSubject, "invalid_assignment"),
      );
      const terminal = await this.api.fail(
        lease,
        terminalRequestId(lease),
        "invalid_assignment",
        observation,
      );
      this.logger.log({
        event: "original_source_verification_reported",
        outcome: "failed",
        assignment_fingerprint: fingerprint,
        error_code: "invalid_assignment",
        replayed: terminal.replayed,
      });
      return terminal.replayed ? "replayed" : "failed";
    }

    const heartbeatController = new AbortController();
    let heartbeatSequence = lease.heartbeat_sequence + 1;
    let heartbeatFailure: unknown;
    await this.api.heartbeat(lease, heartbeatSequence);
    heartbeatSequence += 1;
    const heartbeatRun = (async (): Promise<void> => {
      while (!heartbeatController.signal.aborted) {
        await this.sleep(
          this.leaseHeartbeatIntervalMs,
          heartbeatController.signal,
        );
        if (heartbeatController.signal.aborted) return;
        try {
          await this.api.heartbeat(lease, heartbeatSequence);
          heartbeatSequence += 1;
        } catch (error) {
          heartbeatFailure = error;
          heartbeatController.abort();
          return;
        }
      }
    })();

    let verification;
    try {
      verification = await this.verifier.verify(subject, heartbeatController.signal);
    } catch {
      const errorCode = "unreachable" as const;
      verification = {
        kind: "fail" as const,
        error_code: errorCode,
        observation: originalSourceFailureObservation(subject, errorCode),
      };
    } finally {
      heartbeatController.abort();
      await heartbeatRun;
    }
    if (heartbeatFailure) {
      this.logger.log({
        event: "original_source_verification_reported",
        outcome: "lease_lost",
        assignment_fingerprint: fingerprint,
      });
      return "lease_lost";
    }

    // The final heartbeat is the worker's immediate pre-publication fence. A
    // revocation, stale runtime, expired lease, or changed head denies the
    // terminal report without using provider bytes from the lost authority.
    await this.api.heartbeat(lease, heartbeatSequence);
    const requestId = terminalRequestId(lease);
    if (verification.kind === "fail") {
      const terminal = await this.api.fail(
        lease,
        requestId,
        verification.error_code,
        this.publishObservation(verification.observation),
      );
      this.logger.log({
        event: "original_source_verification_reported",
        outcome: "failed",
        assignment_fingerprint: fingerprint,
        error_code: verification.error_code,
        replayed: terminal.replayed,
      });
      return terminal.replayed ? "replayed" : "failed";
    }

    const terminal = await this.api.complete(
      lease,
      requestId,
      this.publishObservation(verification.observation),
    );
    this.logger.log({
      event: "original_source_verification_reported",
      outcome: "complete",
      assignment_fingerprint: fingerprint,
      result: verification.observation.result,
      replayed: terminal.replayed,
    });
    return terminal.replayed ? "replayed" : "completed";
  }

  private publishObservation(
    observation: OriginalSourceVerificationObservation,
  ): OriginalSourceVerificationPublishedObservation {
    return {
      ...observation,
      worker_runtime_identity_sha256: this.workerRuntimeIdentitySha256,
    };
  }
}

export function originalSourceVerifierBinding(
  instance: ManagedCloudRuntimeInstance,
  runtimeSessionToken: string,
): OriginalSourceVerifierBinding {
  if (instance.role !== "original_source_verifier"
    || instance.componentId !== "jobs-workflows") {
    throw new Error("Original-source managed runtime identity is invalid");
  }
  const authority = {
    audience: RUNTIME_AUTHORITY_AUDIENCE,
    activationSha256: instance.activationSha256,
    componentId: instance.componentId,
    headRevision: instance.headRevision,
    manifestSha256: instance.manifestSha256,
    role: instance.role,
    runtimeIdentitySha256: instance.runtimeIdentitySha256,
    runtimeInstanceEpoch: instance.instanceEpoch,
    runtimeInstanceId: instance.runtimeInstanceId,
    transitionSha256: instance.transitionSha256,
    version: 1,
    workerId: instance.workerId,
  };
  return {
    worker_id: instance.workerId,
    runtime_instance_id: instance.runtimeInstanceId,
    runtime_instance_epoch: instance.instanceEpoch,
    runtime_authority_sha256: originalSourceSha256(
      canonicalOriginalSourceJson(authority),
    ),
    runtime_session_token: requiredRuntimeSessionToken(runtimeSessionToken),
  };
}

export function terminalRequestId(lease: OriginalSourceVerificationLease): string {
  const digest = createHash("sha256")
    .update("bluey-jobs-original-source-terminal-request-v1\0")
    .update(lease.assignment_id)
    .update("\0")
    .update(lease.attempt_id)
    .update("\0")
    .update(String(lease.fence))
    .digest("hex");
  return `osv-terminal-${digest}`;
}

function assignmentFingerprint(lease: OriginalSourceVerificationLease): string {
  return createHash("sha256")
    .update("bluey-jobs-original-source-log-v1\0")
    .update(lease.assignment_id)
    .digest("hex")
    .slice(0, 16);
}

function pollErrorCode(
  error: unknown,
): Extract<
  OriginalSourceVerificationWorkerLogEvent,
  { event: "original_source_verification_poll_failed" }
>["error_code"] {
  return error instanceof OriginalSourceVerificationApiError
    ? error.code
    : "worker_error";
}

function requiredRuntimeSessionToken(value: string): string {
  if (typeof value !== "string"
    || !/^[A-Za-z0-9_-]{43}$/.test(value)
    || Buffer.from(value, "base64url").length !== 32
    || Buffer.from(value, "base64url").toString("base64url") !== value) {
    throw new Error("Original-source managed runtime session token is invalid");
  }
  return value;
}

function requiredSha256(value: string, label: string): string {
  if (!/^[a-f0-9]{64}$/.test(value)) {
    throw new Error(`Original-source ${label} is invalid`);
  }
  return value;
}

function boundedInteger(value: number, minimum: number, maximum: number, label: string): number {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`Original-source ${label} is invalid`);
  }
  return value;
}

function interruptibleSleep(milliseconds: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) return resolve();
    const timer = setTimeout(done, milliseconds);
    signal.addEventListener("abort", done, { once: true });
    function done(): void {
      clearTimeout(timer);
      signal.removeEventListener("abort", done);
      resolve();
    }
  });
}
