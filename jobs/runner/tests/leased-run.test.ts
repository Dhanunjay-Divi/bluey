import { randomBytes } from "node:crypto";
import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import type { ActiveExecutionLease, ExecutionLeaseClient } from "../src/execution-lease.js";
import {
  beginLeasedRun,
  finalizeLeasedRun,
  terminalLeaseOutcome,
} from "../src/leased-run.js";
import { readResult, resultPath, stageResult, writeResult } from "../src/result-store.js";

describe("leased runner lifecycle", () => {
  it("does not restore or launch a browser when a duplicate claim is rejected", async () => {
    const browserWork = vi.fn();
    const client = {
      claim: vi.fn(async () => { throw new Error("lease already held"); }),
    } as unknown as Pick<ExecutionLeaseClient, "claim">;

    await expect(beginLeasedRun(client, {
      accountId: "account-123",
      applicationId: "application-123",
      runId: "run-123",
      browserProfileId: "profile-123",
    }, browserWork)).rejects.toThrow("lease already held");

    expect(browserWork).not.toHaveBeenCalled();
  });

  it("finishes side_effect_unknown when result persistence fails after fencing", async () => {
    const order: string[] = [];
    const lease = fakeLease(true, order);

    await expect(finalizeLeasedRun({
      lease,
      intendedOutcome: "submitted",
      async cleanup() { order.push("cleanup"); },
      async stage() {
        order.push("stage");
        throw new Error("disk failure");
      },
      async commit() { return "stored"; },
    })).rejects.toMatchObject({ outcome: "side_effect_unknown" });

    expect(order).toEqual(["cleanup", "stage", "finish:side_effect_unknown"]);
  });

  it("persists a submitted result before finishing the durable lease", async () => {
    const order: string[] = [];
    const lease = fakeLease(true, order);

    const result = await finalizeLeasedRun({
      lease,
      intendedOutcome: "submitted",
      async cleanup() { order.push("cleanup"); },
      async stage() { order.push("stage"); },
      async commit() {
        order.push("commit");
        return "stored";
      },
    });

    expect(result).toBe("stored");
    expect(order).toEqual(["cleanup", "stage", "finish:submitted", "commit"]);
  });

  it("never persists a retryable receipt after a final-submit fence", async () => {
    const order: string[] = [];
    const lease = fakeLease(true, order);
    const stage = vi.fn();
    const commit = vi.fn();

    expect(terminalLeaseOutcome("needs_input", lease)).toBe("side_effect_unknown");
    await expect(finalizeLeasedRun({
      lease,
      intendedOutcome: "released",
      async cleanup() { order.push("cleanup"); },
      stage,
      commit,
    })).rejects.toMatchObject({ outcome: "side_effect_unknown" });
    expect(stage).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(order).toEqual(["cleanup", "finish:side_effect_unknown"]);
  });

  it("does not accept submitted evidence that predates the durable fence", () => {
    expect(terminalLeaseOutcome("submitted", { finalSubmitAttempted: false })).toBe("side_effect_unknown");
  });

  it("leaves a staged result uncommitted when durable finish fails", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-finish-replay-"));
    const key = randomBytes(32);
    const requestId = "run-123:resume:finish-lost";
    const resultContext = { requestId, profileScope: "a".repeat(40) };
    const lease = {
      finalSubmitAttempted: true,
      async finish() { throw new Error("finish response lost"); },
    } as unknown as ActiveExecutionLease;
    const result = { receipt: { status: "submitted" } };

    await expect(finalizeLeasedRun({
      lease,
      intendedOutcome: "submitted",
      async cleanup() {},
      async stage() { await stageResult(root, resultContext, result, key); },
      async commit() {
        await writeResult(root, resultContext, result, key);
        return result;
      },
    })).rejects.toMatchObject({ outcome: "side_effect_unknown" });

    await expect(readFile(resultPath(root, resultContext))).resolves.toBeInstanceOf(Buffer);
    await expect(readResult(root, resultContext, key)).resolves.toBeUndefined();
  });
});

function fakeLease(finalSubmitAttempted: boolean, order: string[]): ActiveExecutionLease {
  return {
    finalSubmitAttempted,
    async finish(outcome: string) { order.push(`finish:${outcome}`); },
  } as unknown as ActiveExecutionLease;
}
