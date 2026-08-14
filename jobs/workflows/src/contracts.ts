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

/**
 * Protocol-v2 workflow history contains only opaque command authority. The
 * application packet and intervention content stay behind activity boundaries.
 */
export interface WorkflowCommandAuthority {
  schemaVersion: 2;
  requestId: string;
  workflowId: string;
  payloadDigest: string;
}

export interface WorkflowResumeCommandAuthority extends WorkflowCommandAuthority {
  interventionId: string;
}

export interface WorkflowUpdateReceipt extends WorkflowResumeCommandAuthority {
  outcome: "accepted";
}

export type WorkflowCommandOperation = "start" | "resume";

export type WorkflowGatewayCommand = WorkflowCommandAuthority & (
  | { operation: "start" }
  | { operation: "resume"; interventionId: string }
);

export interface WorkflowGatewayReceipt extends WorkflowCommandAuthority {
  outcome: "accepted" | "already_accepted";
  temporalRunId: string;
  interventionId?: string;
}

export type WorkflowGatewayErrorReason =
  | "identity_conflict"
  | "invalid_request"
  | "unsupported_protocol"
  | "workflow_not_found"
  | "workflow_closed"
  | "describe_ambiguous"
  | "temporal_unavailable";

export interface WorkflowGatewayError {
  schemaVersion: 2;
  outcome: "identity_conflict" | "rejected" | "delivery_unknown";
  reason: WorkflowGatewayErrorReason;
}

export interface WorkflowCleanupAuthority {
  schemaVersion: 2;
  cleanupRequestId: string;
  generation: number;
  targetSetDigest: string;
  cleanupFence: number;
  workflowId: string;
  startRequestId: string;
  startPayloadDigest: string;
  firstExecutionRunId?: string;
}

export type WorkflowCleanupPendingReason =
  | "termination_pending"
  | "history_delete_pending"
  | "visibility_pending"
  | "temporal_unavailable";

export type WorkflowCleanupReceipt = WorkflowCleanupAuthority & {
  evidenceDigest: string;
} & (
  | { outcome: "complete"; reason: "absence_proved" }
  | { outcome: "pending"; reason: WorkflowCleanupPendingReason }
);

export type WorkflowTerminalState = "failed" | "side_effect_unknown";

export type WorkflowTerminalReasonCode =
  | "runner_failed"
  | "runner_ambiguous"
  | "intervention_timeout"
  | "intervention_limit";

export type WorkflowCommandStep =
  | { state: "submitted" | WorkflowTerminalState }
  | { state: "intervention_prepared"; interventionId: string };

export interface WorkflowInterventionAuthority {
  command: WorkflowCommandAuthority | WorkflowResumeCommandAuthority;
  interventionId: string;
}

export interface WorkflowPublishedIntervention {
  state: "needs_input";
  interventionId: string;
}

export interface WorkflowTerminalCommand {
  command: WorkflowCommandAuthority | WorkflowResumeCommandAuthority;
  terminalState: WorkflowTerminalState;
  reasonCode: WorkflowTerminalReasonCode;
  openInterventionId?: string;
}

export interface OpaqueWorkflowActivities {
  executeApplicationCommand(
    authority: WorkflowCommandAuthority,
  ): Promise<WorkflowCommandStep>;
  resumeApplicationCommand(input: {
    workflow: WorkflowCommandAuthority;
    command: WorkflowResumeCommandAuthority;
  }): Promise<WorkflowCommandStep>;
  publishApplicationIntervention(
    authority: WorkflowInterventionAuthority,
  ): Promise<WorkflowPublishedIntervention>;
  finalizeApplicationCommand(
    input: WorkflowTerminalCommand,
  ): Promise<{ state: WorkflowTerminalState }>;
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
