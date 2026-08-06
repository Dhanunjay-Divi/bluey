import { join } from "node:path";
import type {
  ApplicationPacket,
  NormalizedJob,
} from "@bluey/jobs-automation";
import {
  ApprovedExecutionIntegrityError,
  CertifiedProviderJobKeyError,
  assertCertifiedProviderNavigationJob,
  assertApprovedExecutionChecksum,
} from "@bluey/jobs-automation";
import {
  scopedLocalRunAuthorization,
  scopedLocalRunReconciliationAuthorization,
  type LocalRunCapabilities,
  type LocalRunCapabilityOperation,
} from "./local-capabilities.js";
import { LocalBrowserError } from "./local-failure.js";
import type { SafeJobContext } from "./controller-state.js";
import { identityProfileDirectory } from "./profile.js";

export interface StartRunRequest {
  accountId: string;
  applicationIdentityId: string;
  runId: string;
  applicationId: string;
  browserProfileId?: string;
  url: string;
  packet: ApplicationPacket;
  job?: NormalizedJob;
}

export interface LocalRunDelivery {
  apiOrigin: string;
  capabilities: LocalRunCapabilities;
}

export function validateStartRunRequest(request: StartRunRequest): void {
  assertIdentifier(request.accountId);
  assertIdentifier(request.applicationIdentityId);
  assertIdentifier(request.runId);
  assertIdentifier(request.applicationId);
  if (request.packet.applicationId !== request.applicationId) {
    throw new LocalBrowserError("run_request_invalid");
  }
  if (request.packet.applicationIdentityId
    && request.packet.applicationIdentityId !== request.applicationIdentityId) {
    throw new LocalBrowserError("identity_mismatch");
  }
  if (!request.job) throw new LocalBrowserError("run_request_invalid");
  try {
    assertApprovedExecutionChecksum(request.packet, request.job);
    assertCertifiedProviderNavigationJob(request.url, request.job);
  } catch (error) {
    if (error instanceof ApprovedExecutionIntegrityError
      || error instanceof CertifiedProviderJobKeyError) {
      throw new LocalBrowserError("launch_mismatch");
    }
    throw error;
  }
}

export function localRunDirectory(userData: string, request: StartRunRequest): string {
  return join(
    identityProfileDirectory(userData, request.accountId, request.applicationIdentityId),
    "runs",
    request.runId,
  );
}

export function localRunDirectoryIfValid(
  userData: string,
  request: StartRunRequest,
): string | undefined {
  return identifiersAreValid(request.accountId, request.applicationIdentityId, request.runId)
    ? localRunDirectory(userData, request)
    : undefined;
}

export function localResumeUrl(runId: string, delivery: LocalRunDelivery): string {
  const { capability } = localRunAuthorization(delivery, "resume");
  return `bluey-jobs://resume/${encodeURIComponent(runId)}?capability=${encodeURIComponent(capability)}`;
}

export function localRunAuthorization(
  delivery: LocalRunDelivery,
  operation: LocalRunCapabilityOperation,
): { capability: string } {
  try {
    return scopedLocalRunAuthorization(delivery.capabilities, operation);
  } catch {
    throw new LocalBrowserError("launch_expired");
  }
}

export function localRunReconciliationAuthorization(
  delivery: LocalRunDelivery,
  nowMs = Date.now(),
): { capability: string } {
  try {
    return scopedLocalRunReconciliationAuthorization(delivery.capabilities, nowMs);
  } catch {
    throw new LocalBrowserError("launch_expired");
  }
}

export function jobsApiOrigin(environment: NodeJS.ProcessEnv = process.env): string {
  const value = (environment.BLUEY_JOBS_API_ORIGIN || "https://bluey.sh").replace(/\/$/, "");
  const url = new URL(value);
  if (url.username || url.password
    || (url.protocol !== "https:"
      && !(url.protocol === "http:" && ["127.0.0.1", "localhost"].includes(url.hostname)))) {
    throw new LocalBrowserError("configuration_invalid");
  }
  return url.toString().replace(/\/$/, "");
}

export function safeJobContext(request: StartRunRequest): SafeJobContext {
  return {
    ...(request.job?.company ? { company: request.job.company } : {}),
    ...(request.job?.title ? { role: request.job.title } : {}),
    identityAvailable: Boolean(
      request.applicationIdentityId
      && (!request.packet.applicationIdentityId
        || request.packet.applicationIdentityId === request.applicationIdentityId),
    ),
  };
}

function identifiersAreValid(...values: unknown[]): boolean {
  return values.every((value) => typeof value === "string" && /^[A-Za-z0-9_-]{3,160}$/.test(value));
}

function assertIdentifier(value: string): void {
  if (!identifiersAreValid(value)) throw new LocalBrowserError("run_request_invalid");
}
