import { describe, expect, it, vi } from "vitest";
import {
  PROTOCOL_COMMAND_IN_FLIGHT_CAP,
  ProtocolCommandSingleFlight,
} from "../src/protocol-command-single-flight.js";

describe("protocol command single-flight", () => {
  it("bounds unique in-flight commands without retaining completed URLs", async () => {
    const flights = new ProtocolCommandSingleFlight();
    const gate = deferred<void>();
    const reject = vi.fn(() => -1);
    const pending = Array.from({ length: PROTOCOL_COMMAND_IN_FLIGHT_CAP }, (_, index) => (
      flights.run(
        `bluey-jobs://open?command=${index}`,
        async () => {
          await gate.promise;
          return index;
        },
        reject,
      )
    ));
    await expect(flights.run(
      "bluey-jobs://open?command=overflow",
      async () => 999,
      reject,
    )).resolves.toBe(-1);
    expect(reject).toHaveBeenCalledOnce();

    gate.resolve();
    await Promise.all(pending);
    await expect(flights.run(
      "bluey-jobs://open?command=0",
      async () => 1000,
      reject,
    )).resolves.toBe(1000);
  });
});

function deferred<T>(): {
  promise: Promise<T>;
  resolve(value?: T | PromiseLike<T>): void;
} {
  let resolve!: (value?: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}
