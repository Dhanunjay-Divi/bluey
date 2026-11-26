import { describe, expect, it } from "vitest";
import { projectC2CThread } from "../src/c2c-thread.js";
import type { C2CThreadEvidence, C2CThreadScope } from "../src/c2c-thread.js";

const OCCURRED_AT = 1_800_000_000_000;

function scope(): C2CThreadScope {
  return {
    accountId: "account-1",
    requirementId: "requirement-1",
    vendorId: "vendor-1",
    connectionId: "connection-1",
    provider: "gmail",
    providerThreadId: "thread-1",
  };
}

function evidence(overrides: Partial<C2CThreadEvidence> = {}): C2CThreadEvidence {
  return {
    ...scope(),
    providerMessageId: "message-1",
    evidenceSha256: "a".repeat(64),
    direction: "inbound",
    occurredAtMs: OCCURRED_AT,
    ...overrides,
  };
}

function expectNoReply(value: ReturnType<typeof projectC2CThread>): void {
  expect(value.canReply).toBe(false);
}

describe("C2C read-only thread projection", () => {
  it("orders evidence by occurrence time and then provider message ID", () => {
    const result = projectC2CThread(scope(), [
      evidence({ providerMessageId: "message-z", occurredAtMs: OCCURRED_AT + 1 }),
      evidence({ providerMessageId: "message-b", direction: "outbound" }),
      evidence({ providerMessageId: "message-a" }),
    ]);

    expect(result.state).toBe("read_only");
    if (result.state !== "read_only") {
      throw new Error("expected read-only projection");
    }
    expect(result.messages.map(({ direction, occurredAtMs }) => ({ direction, occurredAtMs }))).toEqual([
      { direction: "inbound", occurredAtMs: OCCURRED_AT },
      { direction: "outbound", occurredAtMs: OCCURRED_AT },
      { direction: "inbound", occurredAtMs: OCCURRED_AT + 1 },
    ]);
    expectNoReply(result);
  });

  it("collapses an identical evidence replay", () => {
    const replay = evidence();
    const result = projectC2CThread(scope(), [replay, { ...replay }]);

    expect(result.state).toBe("read_only");
    if (result.state !== "read_only") {
      throw new Error("expected read-only projection");
    }
    expect(result.messages).toHaveLength(1);
    expect(result.messages[0]).toMatchObject({ provider: "gmail", direction: "inbound" });
    expectNoReply(result);
  });

  it("blocks a conflicting duplicate provider message", () => {
    const result = projectC2CThread(scope(), [
      evidence(),
      evidence({ evidenceSha256: "b".repeat(64) }),
    ]);

    expect(result.state).toBe("blocked");
    expectNoReply(result);
  });

  it.each([
    ["account", { accountId: "account-2" }],
    ["requirement", { requirementId: "requirement-2" }],
    ["vendor", { vendorId: "vendor-2" }],
    ["connection", { connectionId: "connection-2" }],
    ["provider", { provider: "outlook_email" }],
    ["thread", { providerThreadId: "thread-2" }],
  ] as const)("blocks evidence outside the exact %s scope", (_field, overrides) => {
    const result = projectC2CThread(scope(), [evidence(overrides)]);

    expect(result.state).toBe("blocked");
    expectNoReply(result);
  });

  it.each([
    ["missing direction", { direction: undefined }],
    ["invalid direction", { direction: "sideways" }],
    ["missing thread", { providerThreadId: "" }],
    ["malformed evidence hash", { evidenceSha256: "A".repeat(64) }],
    ["invalid occurrence time", { occurredAtMs: -1 }],
  ] as const)("blocks %s", (_caseName, unsafeOverrides) => {
    const unsafeEvidence = {
      ...evidence(),
      ...unsafeOverrides,
    } as unknown as C2CThreadEvidence;
    const result = projectC2CThread(scope(), [unsafeEvidence]);

    expect(result.state).toBe("blocked");
    expectNoReply(result);
  });

  it("blocks an unknown provider", () => {
    const unsafeScope = { ...scope(), provider: "imap" } as unknown as C2CThreadScope;
    const unsafeEvidence = { ...evidence(), provider: "imap" } as unknown as C2CThreadEvidence;
    const result = projectC2CThread(unsafeScope, [unsafeEvidence]);

    expect(result.state).toBe("blocked");
    expectNoReply(result);
  });

  it("blocks an unsupported raw Outlook provider alias", () => {
    const unsafeScope = { ...scope(), provider: "outlook" } as unknown as C2CThreadScope;
    const unsafeEvidence = { ...evidence(), provider: "outlook" } as unknown as C2CThreadEvidence;
    const result = projectC2CThread(unsafeScope, [unsafeEvidence]);

    expect(result.state).toBe("blocked");
    expectNoReply(result);
  });

  it("blocks an empty evidence set rather than synthesizing a thread", () => {
    const result = projectC2CThread(scope(), []);

    expect(result.state).toBe("blocked");
    expectNoReply(result);
  });

  it("blocks a thread that exceeds the 250-message cap", () => {
    const messages = Array.from({ length: 251 }, (_, index) => evidence({
      providerMessageId: `message-${index + 1}`,
      occurredAtMs: OCCURRED_AT + index,
    }));
    const result = projectC2CThread(scope(), messages);

    expect(result.state).toBe("blocked");
    expectNoReply(result);
  });

  it("projects only display metadata, omitting all provider identifiers and evidence fingerprints", () => {
    const rawEvidence = {
      ...evidence(),
      subject: "C2C opportunity",
      body: "Call me at a private number.",
      transport: "smtp",
      delivery: "delivered",
    } as unknown as C2CThreadEvidence;
    const result = projectC2CThread(scope(), [rawEvidence]);

    expect(result.state).toBe("read_only");
    if (result.state !== "read_only") {
      throw new Error("expected read-only projection");
    }
    expect(result.messages).toEqual([{
      provider: "gmail",
      direction: "inbound",
      occurredAtMs: OCCURRED_AT,
    }]);
    const serialized = JSON.stringify(result);
    for (const hidden of [
      "account-1", "requirement-1", "vendor-1", "connection-1", "thread-1", "message-1",
      "a".repeat(64), "C2C opportunity", "Call me at a private number.", "smtp", "delivered",
    ]) {
      expect(serialized).not.toContain(hidden);
    }
    expectNoReply(result);
  });
});
