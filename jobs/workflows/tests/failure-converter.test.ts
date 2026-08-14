import { describe, expect, it } from "vitest";
import {
  ActivityFailure,
  ApplicationFailure,
  defaultPayloadConverter,
  RetryState,
  TimeoutFailure,
  TimeoutType,
} from "@temporalio/common";
import { failureConverter } from "../src/failure-converter.js";

describe("opaque Temporal failure conversion", () => {
  it("removes messages, stacks, types, details, and nested causes", () => {
    const cause = ApplicationFailure.nonRetryable(
      "answer=private-answer",
      "PrivateEmployerFailure",
      { accountId: "account-private", takeoverUrl: "https://private.example/session" },
    );
    cause.stack = "/Users/private/project/activity.ts:42";
    const error = ApplicationFailure.fromError(cause, {
      message: "OTP 123456 failed for private@example.com",
      type: "PrivateOtpFailure",
    });

    const converted = failureConverter.errorToFailure(error, defaultPayloadConverter);
    const serialized = JSON.stringify(converted);

    expect(converted.message).toBe("opaque_failure");
    expect(converted.stackTrace).toBe("");
    expect(converted.applicationFailureInfo?.type).toBe("opaque_failure");
    expect(converted.applicationFailureInfo?.details).toBeUndefined();
    for (const privateValue of [
      "private-answer",
      "PrivateEmployerFailure",
      "PrivateOtpFailure",
      "account-private",
      "private.example",
      "private@example.com",
      "123456",
      "/Users/private",
    ]) {
      expect(serialized).not.toContain(privateValue);
    }
  });

  it("recursively removes heartbeat details and activity identity", () => {
    const timeout = new TimeoutFailure(
      "private timeout message",
      {
        accountId: "account-private",
        answer: "private-answer",
        otp: "123456",
      },
      TimeoutType.HEARTBEAT,
    );
    const error = new ActivityFailure(
      "private activity failure",
      "private-activity-type",
      "private-activity-id",
      RetryState.TIMEOUT,
      "private-worker-identity",
      timeout,
    );

    const converted = failureConverter.errorToFailure(error, defaultPayloadConverter);
    const serialized = JSON.stringify(converted);

    expect(converted.activityFailureInfo).toMatchObject({
      activityId: "opaque_activity",
      activityType: { name: "opaque_activity" },
    });
    expect(converted.cause?.timeoutFailureInfo?.lastHeartbeatDetails).toBeUndefined();
    for (const privateValue of [
      "account-private",
      "private-answer",
      "123456",
      "private timeout",
      "private activity",
      "private-worker",
    ]) {
      expect(serialized).not.toContain(privateValue);
    }
  });

  it.each([
    "api_request_rejected",
    "identity_conflict",
    "invalid_authority",
    "invalid_request",
    "invalid_response",
    "not_found",
    "runner_release_rejected",
  ])(
    "preserves the closed %s reason without details",
    (type) => {
      const error = ApplicationFailure.nonRetryable(
        "private account detail",
        type,
        { answer: "private-answer" },
      );

      const converted = failureConverter.errorToFailure(error, defaultPayloadConverter);

      expect(converted.applicationFailureInfo?.type).toBe(type);
      expect(converted.applicationFailureInfo?.details).toBeUndefined();
      expect(JSON.stringify(converted)).not.toContain("private");
    },
  );
});
