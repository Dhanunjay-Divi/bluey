import {
  DefaultFailureConverter,
  type PayloadConverter,
  type ProtoFailure,
  type SerializationContext,
} from "@temporalio/common";

/**
 * Temporal's default converter writes arbitrary exception messages and stacks
 * into history. Protocol v2 deliberately retains only a closed failure marker
 * and structural retry metadata; private activity errors remain in the API and
 * runner services that already own that data.
 */
class OpaqueFailureConverter extends DefaultFailureConverter {
  override errorToFailure(
    error: unknown,
    payloadConverter: PayloadConverter,
    context?: SerializationContext,
  ): ProtoFailure {
    return redactFailure(super.errorToFailure(error, payloadConverter, context));
  }
}

function redactFailure(source: ProtoFailure): ProtoFailure {
  const failure: ProtoFailure = {
    message: "opaque_failure",
    source: "TypeScriptSDK",
    stackTrace: "",
    cause: source.cause ? redactFailure(source.cause) : undefined,
  };
  if (source.applicationFailureInfo) {
    failure.applicationFailureInfo = {
      type: closedFailureType(source.applicationFailureInfo.type),
      nonRetryable: source.applicationFailureInfo.nonRetryable,
      nextRetryDelay: source.applicationFailureInfo.nextRetryDelay,
      category: source.applicationFailureInfo.category,
    };
  }
  if (source.activityFailureInfo) {
    failure.activityFailureInfo = {
      scheduledEventId: source.activityFailureInfo.scheduledEventId,
      startedEventId: source.activityFailureInfo.startedEventId,
      activityId: "opaque_activity",
      activityType: { name: "opaque_activity" },
      retryState: source.activityFailureInfo.retryState,
    };
  }
  if (source.childWorkflowExecutionFailureInfo) {
    failure.childWorkflowExecutionFailureInfo = {
      namespace: "opaque_namespace",
      workflowExecution: {
        workflowId: "opaque_workflow",
        runId: "opaque_run",
      },
      workflowType: { name: "opaque_workflow" },
      initiatedEventId: source.childWorkflowExecutionFailureInfo.initiatedEventId,
      startedEventId: source.childWorkflowExecutionFailureInfo.startedEventId,
      retryState: source.childWorkflowExecutionFailureInfo.retryState,
    };
  }
  if (source.timeoutFailureInfo) {
    failure.timeoutFailureInfo = {
      timeoutType: source.timeoutFailureInfo.timeoutType,
    };
  }
  if (source.canceledFailureInfo) {
    failure.canceledFailureInfo = {};
  }
  if (source.terminatedFailureInfo) {
    failure.terminatedFailureInfo = {};
  }
  if (source.serverFailureInfo) {
    failure.serverFailureInfo = {
      nonRetryable: source.serverFailureInfo.nonRetryable,
    };
  }
  if (source.resetWorkflowFailureInfo) {
    failure.resetWorkflowFailureInfo = {};
  }
  if (source.nexusOperationExecutionFailureInfo) {
    failure.nexusOperationExecutionFailureInfo = {
      scheduledEventId: source.nexusOperationExecutionFailureInfo.scheduledEventId,
      endpoint: "opaque_endpoint",
      service: "opaque_service",
      operation: "opaque_operation",
    };
  }
  if (source.nexusHandlerFailureInfo) {
    failure.nexusHandlerFailureInfo = {
      type: "opaque_failure",
      retryBehavior: source.nexusHandlerFailureInfo.retryBehavior,
    };
  }
  return failure;
}

function closedFailureType(value: string | null | undefined): string {
  return value && CLOSED_FAILURE_TYPES.has(value) ? value : "opaque_failure";
}

const CLOSED_FAILURE_TYPES = new Set([
  "api_request_rejected",
  "identity_conflict",
  "invalid_authority",
  "invalid_request",
  "invalid_response",
  "not_found",
  "runner_release_rejected",
]);

export const failureConverter = new OpaqueFailureConverter();
