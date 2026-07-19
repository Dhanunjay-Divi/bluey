import { describe, expect, it, vi } from "vitest";
import { ExecutionSingleFlight } from "../src/execution-single-flight.js";

describe("local execution single-flight", () => {
  it("fails a concurrent fresh run closed when the same identity is already executing", async () => {
    const flights = new ExecutionSingleFlight();
    const gate = deferred<void>();
    const firstOperation = vi.fn(async () => {
      await gate.promise;
      return "first";
    });
    const secondOperation = vi.fn(async () => "second");
    const first = flights.run(
      { runId: "run-1", applicationIdentityId: "identity-1" },
      firstOperation,
      identityBusy,
    );
    const second = flights.run(
      { runId: "run-2", applicationIdentityId: "identity-1" },
      secondOperation,
      identityBusy,
    );

    await expect(second).rejects.toThrow("identity_busy");
    expect(secondOperation).not.toHaveBeenCalled();
    gate.resolve();
    await expect(first).resolves.toBe("first");
    expect(firstOperation).toHaveBeenCalledOnce();
  });

  it("coalesces concurrent resumes for one run into one final-submit path", async () => {
    const flights = new ExecutionSingleFlight();
    const gate = deferred<void>();
    let finalSubmitCount = 0;
    const operation = vi.fn(async () => {
      await gate.promise;
      finalSubmitCount += 1;
      return "submitted";
    });
    const key = { runId: "run-1", applicationIdentityId: "identity-1" };
    const first = flights.run(key, operation, identityBusy);
    const duplicate = flights.run(key, operation, identityBusy);

    expect(duplicate).toBe(first);
    gate.resolve();
    await expect(Promise.all([first, duplicate])).resolves.toEqual(["submitted", "submitted"]);
    expect(operation).toHaveBeenCalledOnce();
    expect(finalSubmitCount).toBe(1);
  });

  it("coalesces joined failures into one fenced reporting path", async () => {
    const flights = new ExecutionSingleFlight();
    const gate = deferred<void>();
    let failureReportCount = 0;
    const operation = async () => {
      await gate.promise;
      failureReportCount += 1;
      return { status: "failed" };
    };
    const key = { runId: "run-1", applicationIdentityId: "identity-1" };
    const first = flights.run(key, operation, identityBusy);
    const duplicate = flights.run(key, operation, identityBusy);
    gate.resolve();
    await Promise.all([first, duplicate]);
    expect(failureReportCount).toBe(1);
  });

  it("releases both keys after the full operation settles", async () => {
    const flights = new ExecutionSingleFlight();
    await flights.run(
      { runId: "run-1", applicationIdentityId: "identity-1" },
      async () => "done",
      identityBusy,
    );
    await expect(flights.run(
      { runId: "run-2", applicationIdentityId: "identity-1" },
      async () => "next",
      identityBusy,
    )).resolves.toBe("next");
  });
});

function identityBusy(): Error {
  return new Error("identity_busy");
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve(value: T | PromiseLike<T>): void;
} {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}
