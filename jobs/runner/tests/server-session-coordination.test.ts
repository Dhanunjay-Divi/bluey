import { describe, expect, it, vi } from "vitest";
import { ProfileRecoveryBlockedError } from "../src/recovery-isolation.js";
import {
  serializedBrowserSessionMutation,
  serializedKnownBrowserSessionMutation,
} from "../src/server.js";

const PROFILE_A = "a".repeat(40);
const PROFILE_B = "b".repeat(40);

describe("runner browser-session coordination", () => {
  it("serializes concurrent resume operations for the same browser session", async () => {
    const firstEntered = deferred();
    const releaseFirst = deferred();
    const order: string[] = [];
    const first = serializedBrowserSessionMutation(
      "cloud-application-serialize",
      PROFILE_A,
      async () => {
        order.push("first-entered");
        firstEntered.resolve();
        await releaseFirst.promise;
        order.push("first-finished");
        return "first";
      },
    );
    await firstEntered.promise;

    const second = serializedBrowserSessionMutation(
      "cloud-application-serialize",
      PROFILE_A,
      async () => {
        order.push("second-entered");
        return "second";
      },
    );
    await Promise.resolve();
    expect(order).toEqual(["first-entered"]);

    releaseFirst.resolve();
    await expect(Promise.all([first, second])).resolves.toEqual(["first", "second"]);
    expect(order).toEqual(["first-entered", "first-finished", "second-entered"]);
  });

  it("waits for every queued resume before coordinating DELETE", async () => {
    const resumeEntered = deferred();
    const releaseResume = deferred();
    const order: string[] = [];
    const resume = serializedBrowserSessionMutation(
      "cloud-application-delete",
      PROFILE_A,
      async () => {
        order.push("first-resume-entered");
        resumeEntered.resolve();
        await releaseResume.promise;
        order.push("first-resume-finished");
      },
    );
    await resumeEntered.promise;

    const queuedResume = serializedBrowserSessionMutation(
      "cloud-application-delete",
      PROFILE_A,
      async () => {
        order.push("second-resume-entered");
        order.push("second-resume-finished");
      },
    );

    const deleteOperation = vi.fn(async (profileScope: string) => {
      expect(profileScope).toBe(PROFILE_A);
      order.push("delete-entered");
    });
    const deletion = serializedKnownBrowserSessionMutation(
      "cloud-application-delete",
      undefined,
      deleteOperation,
    );
    await Promise.resolve();
    expect(deleteOperation).not.toHaveBeenCalled();

    releaseResume.resolve();
    await resume;
    await queuedResume;
    await expect(deletion).resolves.toBe(true);
    expect(deleteOperation).toHaveBeenCalledTimes(1);
    expect(order).toEqual([
      "first-resume-entered",
      "first-resume-finished",
      "second-resume-entered",
      "second-resume-finished",
      "delete-entered",
    ]);
    await expect(serializedKnownBrowserSessionMutation(
      "cloud-application-delete",
      undefined,
      async () => {},
    )).resolves.toBe(false);
  });

  it("fails closed when a pending browser session is reused across profiles", async () => {
    const firstEntered = deferred();
    const releaseFirst = deferred();
    const first = serializedBrowserSessionMutation(
      "cloud-application-cross-profile",
      PROFILE_A,
      async () => {
        firstEntered.resolve();
        await releaseFirst.promise;
      },
    );
    await firstEntered.promise;
    const conflictingOperation = vi.fn(async () => {});

    await expect(serializedBrowserSessionMutation(
      "cloud-application-cross-profile",
      PROFILE_B,
      conflictingOperation,
    )).rejects.toBeInstanceOf(ProfileRecoveryBlockedError);
    expect(conflictingOperation).not.toHaveBeenCalled();

    releaseFirst.resolve();
    await first;
  });
});

function deferred(): { promise: Promise<void>; resolve: () => void } {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
