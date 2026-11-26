import { describe, expect, it } from "vitest";
import type { CommunicationActionDetail, CommunicationActionSummary } from "../types";
import hashVectorFixture from "../fixtures/communication-payload-hash-vectors.json";
import {
  communicationActionCanApprove,
  communicationActionCanCancel,
  communicationActionStatus,
  canonicalCommunicationPayloadJson,
  canonicalCommunicationPayloadSha256,
  communicationApprovalUnavailableReason,
  communicationCancellationDescription,
  communicationActionNeedsPeriodicRefresh,
  communicationActionsNeedPeriodicRefresh,
  communicationDetailMatchesSummary,
  communicationTransitionMatches,
  CommunicationRequestLineage,
  decodeCommunicationActionDetail,
  decodeCommunicationActionSummaries,
  mergeCommunicationActionSummaries,
  reconcileCommunicationActionDetail,
  reviewedCommunicationPayload,
  verifiedCommunicationPayload,
} from "./communication-actions";

const HASH = "a".repeat(64);

function summary(
  overrides: Partial<CommunicationActionSummary> = {},
): CommunicationActionSummary {
  return {
    id: "action-one",
    application_id: "application-one",
    connection_id: "connection-one",
    source_message_id: "message-one",
    kind: "reply",
    provider: "gmail",
    payload_sha256: HASH,
    action_revision: 1,
    status: "awaiting_approval",
    execution_available: true,
    execution_unavailable_reason: "",
    approved_at_ms: null,
    dispatched_at_ms: null,
    created_at_ms: 1_786_000_000_000,
    updated_at_ms: 1_786_000_000_000,
    ...overrides,
  };
}

function detail(
  overrides: Partial<CommunicationActionDetail> = {},
): CommunicationActionDetail {
  return {
    ...summary(),
    connection_account_label: "candidate@gmail.com",
    source_context: {
      sender: "recruiter@example.org",
      reply_target: "recruiter@example.org",
      subject: "Interview availability",
      received_at_ms: 1_785_999_000_000,
    },
    payload: {
      to: "recruiter@example.org",
      subject: "Interview availability",
      body_text: "Tuesday afternoon works for me.\n\nThank you,\nTaylor",
    },
    ...overrides,
  };
}

describe("reviewed communication action decoding", () => {
  it("accepts the exact bounded summary and detail contracts", () => {
    expect(decodeCommunicationActionSummaries([summary()])).toEqual([summary()]);
    expect(decodeCommunicationActionDetail(detail())).toEqual(detail());
  });

  it("preserves the full reply body including whitespace", () => {
    const payload = reviewedCommunicationPayload(detail());

    expect(payload.kind).toBe("reply");
    if (payload.kind === "reply") {
      expect(payload.body_text).toBe("Tuesday afternoon works for me.\n\nThank you,\nTaylor");
    }
  });

  it("consumes the shared Rust/TypeScript UTF-8 SHA-256 vectors", async () => {
    expect(hashVectorFixture.schema_version).toBe(1);
    const vectors = hashVectorFixture.vectors as Array<{
      name: string;
      kind: "reply" | "calendar";
      payload: unknown;
      canonical_json: string;
      sha256: string;
    }>;
    for (const vector of vectors) {
      const action = detail(vector.kind === "reply" ? {
        payload_sha256: vector.sha256,
        payload: vector.payload,
      } : {
        kind: "calendar",
        provider: "google_calendar",
        source_message_id: null,
        source_context: null,
        payload_sha256: vector.sha256,
        payload: vector.payload,
      });
      const payload = reviewedCommunicationPayload(action);
      expect(canonicalCommunicationPayloadJson(payload), vector.name).toBe(vector.canonical_json);
      await expect(canonicalCommunicationPayloadSha256(payload)).resolves.toBe(vector.sha256);
      await expect(verifiedCommunicationPayload(action)).resolves.toMatchObject({
        kind: vector.kind,
      });
    }

    const reply = detail({
      payload_sha256: vectors[0].sha256,
      payload: vectors[0].payload,
    });
    await expect(verifiedCommunicationPayload({
      ...reply,
      payload_sha256: "0".repeat(64),
    })).rejects.toThrow("could not verify");
  });

  it("binds a reply to the canonical provider reply target, not display sender", () => {
    const outlook = detail({
      provider: "outlook_email",
      source_context: {
        sender: "recruiter@example.org",
        reply_target: "talent-team@example.org",
        subject: "Interview availability",
        received_at_ms: 1_785_999_000_000,
      },
      payload: {
        to: "talent-team@example.org",
        subject: "Interview availability",
        body_text: "Tuesday afternoon works for me.",
      },
    });

    expect(reviewedCommunicationPayload(outlook)).toMatchObject({
      kind: "reply",
      to: "talent-team@example.org",
    });
    expect(() => reviewedCommunicationPayload({
      ...outlook,
      payload: { ...outlook.payload as object, to: "recruiter@example.org" },
    })).toThrow("could not verify");
  });

  it("accepts a bounded calendar action with every attendee", () => {
    const action = detail({
      kind: "calendar",
      provider: "google_calendar",
      source_message_id: null,
      source_context: null,
      payload: {
        title: "Interview with Acme",
        starts_at_ms: 2_000_000_000_000,
        ends_at_ms: 2_000_003_600_000,
        time_zone: "America/New_York",
        attendees: ["candidate@example.org", "recruiter@example.org"],
      },
    });

    expect(reviewedCommunicationPayload(action)).toEqual({
      kind: "calendar",
      title: "Interview with Acme",
      starts_at_ms: 2_000_000_000_000,
      ends_at_ms: 2_000_003_600_000,
      time_zone: "America/New_York",
      attendees: ["candidate@example.org", "recruiter@example.org"],
    });
  });

  it.each([
    ["unknown summary field", { ...summary(), private_worker: "worker-one" }],
    ["unknown status", { ...summary(), status: "retrying" }],
    ["unknown provider", { ...summary(), provider: "smtp" }],
    ["provider-kind mismatch", { ...summary(), provider: "google_calendar" }],
    ["reply without a source", { ...summary(), source_message_id: null }],
    ["contradictory readiness", {
      ...summary(),
      execution_available: true,
      execution_unavailable_reason: "Provider delivery is unavailable.",
    }],
    ["missing readiness reason", {
      ...summary(),
      execution_available: false,
      execution_unavailable_reason: "",
    }],
  ])("fails closed for an %s", (_label, value) => {
    expect(() => decodeCommunicationActionSummaries([value])).toThrow(
      "could not verify this reviewed communication action",
    );
  });

  it("requires exact bound account and source context on details", () => {
    const missingAccount = { ...detail() } as Record<string, unknown>;
    delete missingAccount.connection_account_label;
    const missingSource = { ...detail() } as Record<string, unknown>;
    delete missingSource.source_context;
    const missingReplyTarget = structuredClone(detail()) as unknown as Record<string, unknown>;
    delete (missingReplyTarget.source_context as Record<string, unknown>).reply_target;
    const extraSourceAuthority = structuredClone(detail()) as unknown as Record<string, unknown>;
    (extraSourceAuthority.source_context as Record<string, unknown>).provider_id = "private";
    const unexpectedCalendarSource = detail({
      kind: "calendar",
      provider: "google_calendar",
      source_message_id: null,
      payload: {
        title: "Interview",
        starts_at_ms: 2_000_000_000_000,
        ends_at_ms: 2_000_003_600_000,
        time_zone: "America/New_York",
        attendees: [],
      },
    });

    expect(() => decodeCommunicationActionDetail(missingAccount)).toThrow(
      "could not verify this reviewed communication action",
    );
    expect(() => decodeCommunicationActionDetail(missingSource)).toThrow(
      "could not verify this reviewed communication action",
    );
    expect(() => decodeCommunicationActionDetail(missingReplyTarget)).toThrow(
      "could not verify this reviewed communication action",
    );
    expect(() => decodeCommunicationActionDetail(extraSourceAuthority)).toThrow(
      "could not verify this reviewed communication action",
    );
    expect(() => decodeCommunicationActionDetail(unexpectedCalendarSource)).toThrow(
      "could not verify this reviewed communication action",
    );
  });

  it("rejects unsafe or out-of-range original-message context", () => {
    const unsafeSubject = detail({
      source_context: {
        sender: "recruiter@example.org",
        reply_target: "recruiter@example.org",
        subject: "Interview\nBcc: hidden@example.org",
        received_at_ms: 1_785_999_000_000,
      },
    });
    const invalidDate = detail({
      source_context: {
        sender: "recruiter@example.org",
        reply_target: "recruiter@example.org",
        subject: "Interview",
        received_at_ms: 8_640_000_000_000_001,
      },
    });
    const unicodeControl = detail({
      source_context: {
        sender: "recruiter@example.org",
        reply_target: "recruiter@example.org",
        subject: "Interview\u0085availability",
        received_at_ms: 1_785_999_000_000,
      },
    });

    expect(() => decodeCommunicationActionDetail(unsafeSubject)).toThrow(
      "could not verify this reviewed communication action",
    );
    expect(() => decodeCommunicationActionDetail(invalidDate)).toThrow(
      "could not verify this reviewed communication action",
    );
    expect(() => decodeCommunicationActionDetail(unicodeControl)).toThrow(
      "could not verify this reviewed communication action",
    );
  });

  it.each([
    ["unknown reply field", {
      ...(detail().payload as Record<string, unknown>),
      cc: "other@example.org",
    }],
    ["malformed email", {
      ...(detail().payload as Record<string, unknown>),
      to: "not-an-email",
    }],
    ["blank body", {
      ...(detail().payload as Record<string, unknown>),
      body_text: "   ",
    }],
    ["header injection", {
      ...(detail().payload as Record<string, unknown>),
      subject: "Interview\r\nBcc: hidden@example.org",
    }],
    ["subject control character", {
      ...(detail().payload as Record<string, unknown>),
      subject: "Interview\u007favailability",
    }],
    ["subject bidi control", {
      ...(detail().payload as Record<string, unknown>),
      subject: "Interview \u202Eavailability",
    }],
    ["reply target bidi control", {
      ...(detail().payload as Record<string, unknown>),
      to: "recruiter\u202E@example.org",
    }],
    ["body line separator", {
      ...(detail().payload as Record<string, unknown>),
      body_text: "Looks good\u2028hidden",
    }],
    ["body bidi override", {
      ...(detail().payload as Record<string, unknown>),
      body_text: "Looks good\u202Ehidden",
    }],
    ["body bidi isolate", {
      ...(detail().payload as Record<string, unknown>),
      body_text: "Looks good\u2066hidden",
    }],
    ["body escape control", {
      ...(detail().payload as Record<string, unknown>),
      body_text: "Looks good\u001Bhidden",
    }],
    ["body backspace control", {
      ...(detail().payload as Record<string, unknown>),
      body_text: "Looks good\u0008hidden",
    }],
    ["body lone surrogate", {
      ...(detail().payload as Record<string, unknown>),
      body_text: "Looks good\ud800hidden",
    }],
    ["NUL in body", {
      ...(detail().payload as Record<string, unknown>),
      body_text: "Looks good\0hidden",
    }],
  ])("fails closed for an %s", (_label, payload) => {
    expect(() => reviewedCommunicationPayload(detail({ payload }))).toThrow(
      "could not verify this reviewed communication action",
    );
  });

  it("preserves reviewed newlines, tabs, ZWNJ, and emoji ZWJ sequences", () => {
    const payload = reviewedCommunicationPayload(detail({
      payload: {
        ...(detail().payload as Record<string, unknown>),
        body_text: "Line one\n\tمی\u200cروم 👨\u200d👩\u200d👧\u200d👦",
      },
    }));

    expect(payload).toMatchObject({
      kind: "reply",
      body_text: "Line one\n\tمی\u200cروم 👨\u200d👩\u200d👧\u200d👦",
    });
  });

  it("rejects malformed calendar times and unknown employer-facing fields", () => {
    const malformed = detail({
      kind: "calendar",
      provider: "outlook_calendar",
      source_message_id: null,
      source_context: null,
      payload: {
        title: "Interview",
        starts_at_ms: 2_000,
        ends_at_ms: 1_000,
        time_zone: "America/New_York",
        attendees: [],
        description: "This field was not reviewed by the supported schema.",
      },
    });

    expect(() => reviewedCommunicationPayload(malformed)).toThrow(
      "could not verify this reviewed communication action",
    );
  });

  it("rejects noncanonical emails, duplicate attendees, and unsafe calendar titles", () => {
    expect(() => reviewedCommunicationPayload(detail({
      payload: {
        ...(detail().payload as Record<string, unknown>),
        to: "Recruiter@example.org",
      },
    }))).toThrow("could not verify");
    expect(() => reviewedCommunicationPayload(detail({
      kind: "calendar",
      provider: "google_calendar",
      source_message_id: null,
      source_context: null,
      payload: {
        title: "Interview",
        starts_at_ms: 2_000_000_000_000,
        ends_at_ms: 2_000_003_600_000,
        time_zone: "America/New_York",
        attendees: ["candidate@example.org", "candidate@example.org"],
      },
    }))).toThrow("could not verify");
    expect(() => reviewedCommunicationPayload(detail({
      kind: "calendar",
      provider: "google_calendar",
      source_message_id: null,
      source_context: null,
      payload: {
        title: "Interview\nBcc: hidden@example.org",
        starts_at_ms: 2_000_000_000_000,
        ends_at_ms: 2_000_003_600_000,
        time_zone: "America/New_York",
        attendees: [],
      },
    }))).toThrow("could not verify");
    expect(() => reviewedCommunicationPayload(detail({
      kind: "calendar",
      provider: "google_calendar",
      source_message_id: null,
      source_context: null,
      payload: {
        title: "Interview \u202E10:00",
        starts_at_ms: 2_000_000_000_000,
        ends_at_ms: 2_000_003_600_000,
        time_zone: "America/New_York",
        attendees: [],
      },
    }))).toThrow("could not verify");
  });

  it("enforces exact status, timestamp, hash, and readiness invariants", () => {
    const time = summary().created_at_ms;
    const invalid = [
      summary({ payload_sha256: "A".repeat(64) }),
      summary({ status: "sent", execution_available: true }),
      summary({ status: "approved", approved_at_ms: null }),
      summary({
        status: "dispatching",
        approved_at_ms: time,
        dispatched_at_ms: null,
      }),
      summary({
        status: "needs_input",
        execution_available: false,
        execution_unavailable_reason: "A fresh grant is unavailable.",
        approved_at_ms: null,
        dispatched_at_ms: null,
      }),
      summary({ approved_at_ms: time + 1 }),
      summary({ created_at_ms: time + 1, updated_at_ms: time }),
      summary({ action_revision: 0 }),
      summary({ action_revision: Number.MAX_SAFE_INTEGER + 1 }),
    ];

    for (const value of invalid) {
      expect(() => decodeCommunicationActionSummaries([value])).toThrow("could not verify");
    }
  });

  it.each([undefined, "", "Not/A_Real_Zone", "America/New York"])(
    "rejects the missing or invalid calendar time zone %s",
    (timeZone) => {
      const payload: Record<string, unknown> = {
        title: "Interview",
        starts_at_ms: 2_000_000_000_000,
        ends_at_ms: 2_000_003_600_000,
        attendees: [],
      };
      if (timeZone !== undefined) payload.time_zone = timeZone;
      const action = detail({
        kind: "calendar",
        provider: "google_calendar",
        source_message_id: null,
        source_context: null,
        payload,
      });

      expect(() => reviewedCommunicationPayload(action)).toThrow(
        "could not verify this reviewed communication action",
      );
    },
  );

  it("requires the calendar attendees key and a bounded time zone", () => {
    const missingAttendees = detail({
      kind: "calendar",
      provider: "google_calendar",
      source_message_id: null,
      source_context: null,
      payload: {
        title: "Interview",
        starts_at_ms: 2_000_000_000_000,
        ends_at_ms: 2_000_003_600_000,
        time_zone: "America/New_York",
      },
    });
    const longTimeZone = detail({
      kind: "calendar",
      provider: "google_calendar",
      source_message_id: null,
      source_context: null,
      payload: {
        title: "Interview",
        starts_at_ms: 2_000_000_000_000,
        ends_at_ms: 2_000_003_600_000,
        time_zone: `Etc/${"A".repeat(61)}`,
        attendees: [],
      },
    });

    expect(() => reviewedCommunicationPayload(missingAttendees)).toThrow(
      "could not verify this reviewed communication action",
    );
    expect(() => reviewedCommunicationPayload(longTimeZone)).toThrow(
      "could not verify this reviewed communication action",
    );
  });

  it("rejects timestamps outside the ECMAScript Date range", () => {
    expect(() => decodeCommunicationActionSummaries([
      summary({ updated_at_ms: 8_640_000_000_000_001 }),
    ])).toThrow("could not verify this reviewed communication action");
    expect(() => reviewedCommunicationPayload(detail({
      kind: "calendar",
      provider: "google_calendar",
      source_message_id: null,
      source_context: null,
      payload: {
        title: "Interview",
        starts_at_ms: 8_640_000_000_000_001,
        ends_at_ms: 8_640_000_000_000_002,
        time_zone: "America/New_York",
        attendees: [],
      },
    }))).toThrow("could not verify this reviewed communication action");
  });

  it("requires every immutable summary field to match the detail", () => {
    expect(communicationDetailMatchesSummary(summary(), detail())).toBe(true);
    const mismatches: Array<Partial<CommunicationActionDetail>> = [
      { id: "action-two" },
      { application_id: "application-two" },
      { connection_id: "connection-two" },
      { source_message_id: "message-two" },
      { kind: "calendar" },
      { provider: "outlook_email" },
      { payload_sha256: "b".repeat(64) },
      { created_at_ms: summary().created_at_ms + 1 },
    ];
    for (const mismatch of mismatches) {
      expect(communicationDetailMatchesSummary(summary(), detail(mismatch))).toBe(false);
    }
    expect(communicationDetailMatchesSummary(
      summary({ action_revision: 2, updated_at_ms: summary().updated_at_ms + 1 }),
      detail(),
    )).toBe(false);
    expect(communicationDetailMatchesSummary(
      summary(),
      detail({ action_revision: 2 }),
    )).toBe(false);
    expect(communicationDetailMatchesSummary(summary(), detail({
      status: "approved",
      approved_at_ms: summary().updated_at_ms,
    }))).toBe(false);
  });

  it("merges selected-application fallbacks without stale status regression", () => {
    const visible = summary({
      id: "visible",
      action_revision: 3,
      updated_at_ms: 300,
      status: "sent",
    });
    const olderSelected = summary({
      id: "older-selected",
      created_at_ms: 100,
      updated_at_ms: 100,
    });
    const merged = mergeCommunicationActionSummaries([visible], [olderSelected]);
    const afterStalePoll = mergeCommunicationActionSummaries(merged, [
      { ...visible, action_revision: 2, status: "dispatching", updated_at_ms: 299 },
    ]);

    expect(afterStalePoll.map((item) => item.id)).toContain("older-selected");
    expect(afterStalePoll.find((item) => item.id === "visible")?.status).toBe("sent");
  });

  it("rejects equal-version conflicts and immutable poll conflicts", () => {
    const current = summary();
    expect(() => mergeCommunicationActionSummaries([current], [{
      ...current,
      status: "approved",
      approved_at_ms: current.updated_at_ms,
    }])).toThrow("could not verify");
    expect(() => mergeCommunicationActionSummaries([current], [{
      ...current,
      connection_id: "connection-two",
      action_revision: current.action_revision + 1,
      updated_at_ms: current.updated_at_ms + 1,
    }])).toThrow("could not verify");
    expect(() => mergeCommunicationActionSummaries([current], [{
      ...current,
      action_revision: current.action_revision + 1,
    }])).toThrow("could not verify");
    expect(() => reconcileCommunicationActionDetail(detail(), {
      ...current,
      action_revision: current.action_revision + 1,
    })).toThrow("could not verify");
    const newer = {
      ...current,
      action_revision: current.action_revision + 1,
      updated_at_ms: current.updated_at_ms + 1,
    };
    expect(() => mergeCommunicationActionSummaries([newer], [{
      ...current,
      updated_at_ms: newer.updated_at_ms,
    }])).toThrow("could not verify");
    expect(() => reconcileCommunicationActionDetail(detail({
      action_revision: newer.action_revision,
      updated_at_ms: newer.updated_at_ms,
    }), {
      ...current,
      updated_at_ms: newer.updated_at_ms,
    })).toThrow("could not verify");
  });

  it("accepts dynamic readiness changes without inventing an action version conflict", () => {
    const available = summary();
    const unavailable = {
      ...available,
      execution_available: false,
      execution_unavailable_reason: "The provider grant changed.",
    };

    expect(communicationDetailMatchesSummary(unavailable, detail())).toBe(true);
    expect(mergeCommunicationActionSummaries([available], [unavailable])).toEqual([unavailable]);

    const reconciled = reconcileCommunicationActionDetail(detail(), unavailable);
    expect(reconciled.execution_available).toBe(false);
    expect(reconciled.execution_unavailable_reason).toBe("The provider grant changed.");
    expect(reconciled.status).toBe(available.status);
  });

  it("reconciles newer polled status and readiness into an open exact detail", () => {
    const current = detail();
    const { payload: _payload, connection_account_label: _label, source_context: _source, ...base } = current;
    void _payload;
    void _label;
    void _source;
    const approvedAtMs = current.updated_at_ms + 1;
    const updated = reconcileCommunicationActionDetail(current, {
      ...base,
      action_revision: current.action_revision + 1,
      status: "approved",
      approved_at_ms: approvedAtMs,
      updated_at_ms: approvedAtMs,
      execution_available: false,
      execution_unavailable_reason: "The provider grant changed.",
    });

    expect(updated.status).toBe("approved");
    expect(updated.execution_available).toBe(false);
    expect(updated.payload).toBe(current.payload);
    expect(() => reconcileCommunicationActionDetail(current, {
      ...base,
      connection_id: "connection-conflict",
      updated_at_ms: approvedAtMs,
    })).toThrow("could not verify");
  });

  it("accepts only exact approved and cancelled mutation transitions", () => {
    const current = detail();
    const nextTime = current.updated_at_ms + 1;
    const approved = detail({
      action_revision: current.action_revision + 1,
      status: "approved",
      approved_at_ms: nextTime,
      updated_at_ms: nextTime,
    });
    const cancelled = detail({
      action_revision: current.action_revision + 1,
      status: "cancelled",
      execution_available: false,
      execution_unavailable_reason: "This communication is not awaiting executable approval.",
      updated_at_ms: nextTime,
    });

    expect(communicationTransitionMatches(current, approved, "approved")).toBe(true);
    expect(communicationTransitionMatches(current, cancelled, "cancelled")).toBe(true);
    expect(communicationTransitionMatches(current, { ...approved, status: "sent" }, "approved"))
      .toBe(false);
    expect(communicationTransitionMatches(current, {
      ...approved,
      source_context: approved.source_context
        ? { ...approved.source_context, reply_target: "other@example.org" }
        : null,
    }, "approved")).toBe(false);
    expect(communicationTransitionMatches(current, {
      ...cancelled,
      approved_at_ms: nextTime,
    }, "cancelled")).toBe(false);
    expect(communicationTransitionMatches(current, {
      ...approved,
      action_revision: current.action_revision,
      approved_at_ms: current.updated_at_ms,
      updated_at_ms: current.updated_at_ms,
    }, "approved")).toBe(false);
  });

  it("clears a superseded loading request without disturbing a newer loading request", () => {
    const lineage = new CommunicationRequestLineage();
    const firstLoading = lineage.begin(true);
    const poll = lineage.begin(false);

    expect(lineage.isCurrent(firstLoading)).toBe(false);
    expect(lineage.isCurrent(poll)).toBe(true);
    expect(lineage.finishLoading(firstLoading)).toBe(true);

    const secondLoading = lineage.begin(true);
    expect(lineage.finishLoading(firstLoading)).toBe(false);
    expect(lineage.finishLoading(secondLoading)).toBe(true);
  });

  it("polls only states that can transition without another user action", () => {
    expect(communicationActionNeedsPeriodicRefresh(summary({ status: "approved" }))).toBe(true);
    expect(communicationActionNeedsPeriodicRefresh(summary({ status: "dispatching" }))).toBe(true);
    expect(communicationActionNeedsPeriodicRefresh(
      summary({ status: "side_effect_unknown" }),
    )).toBe(true);
    expect(communicationActionNeedsPeriodicRefresh(summary())).toBe(false);
    expect(communicationActionNeedsPeriodicRefresh(summary({ status: "failed" }))).toBe(false);
    expect(communicationActionsNeedPeriodicRefresh([summary()], true)).toBe(true);
    expect(communicationActionsNeedPeriodicRefresh([summary()], false)).toBe(false);
  });
});

describe("reviewed communication action controls", () => {
  it("requires both awaiting approval and server-owned execution readiness", () => {
    const unavailable = summary({
      execution_available: false,
      execution_unavailable_reason: "Provider delivery has not passed its launch gate.",
    });

    expect(communicationActionCanApprove(summary())).toBe(true);
    expect(communicationActionCanApprove(unavailable)).toBe(false);
    expect(communicationApprovalUnavailableReason(unavailable)).toContain("launch gate");
    expect(communicationActionCanApprove(summary({ status: "needs_input" }))).toBe(true);
    expect(communicationActionCanApprove(summary({ status: "approved" }))).toBe(false);
  });

  it("surfaces the server-owned reason when approved delivery becomes unavailable", () => {
    const unavailable = communicationActionStatus(summary({
      status: "approved",
      execution_available: false,
      execution_unavailable_reason: "Reconnect the exact mailbox before continuing.",
    }));

    expect(unavailable.label).toBe("Approved · unavailable");
    expect(unavailable.detail).toContain("Reconnect the exact mailbox before continuing.");
    expect(unavailable.detail).toContain("message has not been completed");
  });

  it("allows cancellation only before dispatch", () => {
    expect(communicationActionCanCancel(summary())).toBe(true);
    expect(communicationActionCanCancel(summary({ status: "needs_input" }))).toBe(true);
    expect(communicationActionCanCancel(summary({ status: "approved" }))).toBe(true);
    expect(communicationActionCanCancel(summary({ status: "dispatching" }))).toBe(false);
    expect(communicationActionCanCancel(summary({ status: "side_effect_unknown" }))).toBe(false);
    expect(communicationCancellationDescription("reply")).toContain(
      "The inbound message is not deleted.",
    );
  });

  it.each([
    ["awaiting_approval", "Needs approval"],
    ["approved", "Approved · waiting"],
    ["dispatching", "Sending…"],
    ["sent", "Sent"],
    ["needs_input", "Review again"],
    ["failed", "Delivery unverified"],
    ["side_effect_unknown", "Outcome unknown"],
    ["cancelled", "Cancelled"],
  ] as const)("presents %s truthfully", (status, label) => {
    expect(communicationActionStatus(summary({ status })).label).toBe(label);
  });

  it("never presents an uncertain action as retryable", () => {
    const uncertain = summary({ status: "side_effect_unknown" });
    const presentation = communicationActionStatus(uncertain);

    expect(presentation.detail).toContain("Do not retry");
    expect(communicationActionCanApprove(uncertain)).toBe(false);
    expect(communicationActionCanCancel(uncertain)).toBe(false);
  });

  it("never treats a legacy failed action as proof of no side effect", () => {
    const failed = communicationActionStatus(summary({ status: "failed" }));

    expect(failed.detail).toContain("does not prove");
    expect(failed.detail).toContain("Do not retry");
    expect(communicationActionCanApprove(summary({ status: "failed" }))).toBe(false);
  });

  it("describes provider-authoritative absence as a fresh approval, never a retry", () => {
    const needsInput = summary({ status: "needs_input" });
    const presentation = communicationActionStatus(needsInput);

    expect(presentation.detail).toContain("new approval revision");
    expect(presentation.detail).toContain("never retry automatically");
    expect(communicationActionCanApprove(needsInput)).toBe(true);
    expect(communicationActionCanCancel(needsInput)).toBe(true);
  });

  it("distinguishes cancellation before dispatch from cancellation after proved absence", () => {
    expect(communicationActionStatus(summary({ status: "cancelled" })).detail).toContain(
      "before any provider attempt",
    );
    const afterAbsence = communicationActionStatus(summary({
      status: "cancelled",
      dispatched_at_ms: summary().updated_at_ms,
    })).detail;
    expect(afterAbsence).toContain("Provider evidence showed no message was created");
    expect(afterAbsence).toContain("prevents any further provider attempt");
    expect(afterAbsence).not.toContain("before dispatch");
  });
});
