import type {
  ApplicationPacket,
  ApplicationState,
  NormalizedJob,
  RunnerKind,
  SubmissionReceipt,
} from "@bluey/jobs-automation";

export interface ApplicationWorkflowInput {
  accountId: string;
  applicationId: string;
  jobId: string;
  canonicalJobKey: string;
  packetId: string;
  applicationIdentityId: string;
  browserProfileId: string;
  packet: ApplicationPacket;
  job: NormalizedJob;
  runner: RunnerKind;
  url: string;
  idempotencyKey: string;
}

export interface ApplicationWorkflowResult {
  state: ApplicationState;
  receipt?: SubmissionReceipt;
  interventionId?: string;
  requiresReapproval?: boolean;
}

export interface SubmissionReceiptAuthority {
  leaseToken: string;
  fence: number;
}

export interface RunnerExecutionResult {
  /**
   * A deep-allowlisted receipt safe to serialize into Temporal history. The
   * runner's durable evidence, local paths, and fenced capability stay behind
   * the activity boundary.
   */
  receipt: SubmissionReceipt;
}

export function isSubmissionReceiptAuthority(
  value: unknown,
): value is SubmissionReceiptAuthority {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const record = value as Record<string, unknown>;
  return Object.keys(record).length === 2
    && typeof record.leaseToken === "string"
    && /^[A-Za-z0-9_-]{43}$/.test(record.leaseToken)
    && typeof record.fence === "number"
    && Number.isSafeInteger(record.fence)
    && record.fence > 0;
}

export interface InterventionResolution {
  action: string;
  field?: string;
  answer?: string;
}

export interface JobsActivities {
  assertEntitlement(input: ApplicationWorkflowInput): Promise<void>;
  loadPacket(input: ApplicationWorkflowInput): Promise<void>;
  allocateBrowser(input: ApplicationWorkflowInput): Promise<{ browserSessionId: string }>;
  runApplication(input: ApplicationWorkflowInput & { browserSessionId: string }): Promise<RunnerExecutionResult>;
  resumeApplication(input: ApplicationWorkflowInput & {
    browserSessionId: string;
    requestId: string;
    resolution: InterventionResolution;
  }): Promise<RunnerExecutionResult>;
  persistSubmissionReceipt(input: ApplicationWorkflowInput & {
    browserSessionId: string;
    resultRequestId: string;
  }): Promise<void>;
  releaseBrowser(browserSessionId: string): Promise<void>;
  recordState(input: ApplicationWorkflowInput, state: ApplicationState): Promise<void>;
  createIntervention(input: ApplicationWorkflowInput, receipt: SubmissionReceipt): Promise<string>;
}
