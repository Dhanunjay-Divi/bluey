import { describe, expect, it } from "vitest";
import {
  enqueueProtocolArguments,
  PendingProtocolQueue,
  PENDING_PROTOCOL_CAP,
} from "../src/pending-protocol-queue.js";

describe("pre-ready protocol queue", () => {
  it("is bounded, deduplicated, and retains only Bluey protocol URLs", () => {
    const queue = new PendingProtocolQueue();
    expect(queue.push("https://bluey.sh/jobs")).toBe(false);
    expect(queue.push(`bluey-jobs://open?payload=${"x".repeat(8_192)}`)).toBe(false);
    expect(queue.push("bluey-jobs://open")).toBe(true);
    expect(queue.push("bluey-jobs://open")).toBe(true);
    for (let index = 0; index < PENDING_PROTOCOL_CAP + 10; index += 1) {
      queue.push(`bluey-jobs://run/run-${index}?ticket=${String(index).padStart(64, "a")}`);
    }
    expect(queue.size).toBe(PENDING_PROTOCOL_CAP);
    const drained = queue.drain();
    expect(drained).toHaveLength(PENDING_PROTOCOL_CAP);
    expect(queue.size).toBe(0);
    expect(drained.at(-1)).toContain(`run-${PENDING_PROTOCOL_CAP + 9}`);
  });

  it("captures a first-process protocol URL from the initial Windows argument list", () => {
    const queue = new PendingProtocolQueue();
    enqueueProtocolArguments(queue, [
      "C:\\Program Files\\Bluey Browser\\Bluey Browser.exe",
      "bluey-jobs://run/run-123?ticket=opaque",
    ]);
    expect(queue.drain()).toEqual(["bluey-jobs://run/run-123?ticket=opaque"]);
  });

  it("does nothing when a manual launch has no protocol URL", () => {
    const queue = new PendingProtocolQueue();
    enqueueProtocolArguments(queue, ["Bluey Browser.exe", "--user-data-dir=temp"]);
    expect(queue.drain()).toEqual([]);
  });

  it("ignores the background argument while retaining a protocol URL", () => {
    const queue = new PendingProtocolQueue();
    enqueueProtocolArguments(queue, [
      "Bluey Browser.exe",
      "--bluey-background",
      "bluey-jobs://open",
    ]);
    expect(queue.drain()).toEqual(["bluey-jobs://open"]);
  });

  it("deduplicates the same URL received from initial argv and an early OS event", () => {
    const queue = new PendingProtocolQueue();
    const url = "bluey-jobs://resume/run-123?capability=opaque";
    enqueueProtocolArguments(queue, ["Bluey Browser.exe", url]);
    expect(queue.push(url)).toBe(true);
    expect(queue.drain()).toEqual([url]);
  });
});
