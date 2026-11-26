import { describe, expect, it } from "vitest";

import { createLatestResumeSelection, type ResumeSelectionState } from "./resume-import-selection";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((next, fail) => {
    resolve = next;
    reject = fail;
  });
  return { promise, resolve, reject };
}

describe("latest resume selection", () => {
  it("publishes loading synchronously and ignores a slow older success", async () => {
    const first = deferred<string>();
    const second = deferred<string>();
    const reads = new Map([["a", first.promise], ["b", second.promise]]);
    const states: ResumeSelectionState<string, string>[] = [];
    const selection = createLatestResumeSelection((file: string) => reads.get(file)!);

    const selectingA = selection.select("a", (state) => states.push(state));
    expect(states).toEqual([{ kind: "loading" }]);
    const selectingB = selection.select("b", (state) => states.push(state));
    expect(states).toEqual([{ kind: "loading" }, { kind: "loading" }]);

    second.resolve("parsed-b");
    await selectingB;
    first.resolve("parsed-a");
    await selectingA;

    expect(states.map((state) => state.kind)).toEqual(["loading", "loading", "ready"]);
    expect(states[2]).toMatchObject({ kind: "ready", file: "b", parsed: "parsed-b" });
  });

  it("keeps stale rejections silent but publishes the active selection error", async () => {
    const first = deferred<string>();
    const second = deferred<string>();
    const states: ResumeSelectionState<string, string>[] = [];
    const selection = createLatestResumeSelection((file: string) => file === "a" ? first.promise : second.promise);

    const selectingA = selection.select("a", (state) => states.push(state));
    const selectingB = selection.select("b", (state) => states.push(state));
    first.reject(new Error("old failure"));
    await selectingA;
    expect(states.map((state) => state.kind)).toEqual(["loading", "loading"]);

    const active = new Error("active failure");
    second.reject(active);
    await selectingB;
    expect(states).toEqual([{ kind: "loading" }, { kind: "loading" }, { kind: "error", error: active }]);
  });

  it("does not let an old success replace the newest error", async () => {
    const first = deferred<string>();
    const second = deferred<string>();
    const states: ResumeSelectionState<string, string>[] = [];
    const selection = createLatestResumeSelection((file: string) => file === "a" ? first.promise : second.promise);

    const selectingA = selection.select("a", (state) => states.push(state));
    const selectingB = selection.select("b", (state) => states.push(state));
    const newestError = new Error("newest failure");
    second.reject(newestError);
    await selectingB;
    first.resolve("old parsed result");
    await selectingA;

    expect(states).toEqual([{ kind: "loading" }, { kind: "loading" }, { kind: "error", error: newestError }]);
  });

  it("invalidates without publishing and supports a later new selection", async () => {
    const first = deferred<string>();
    const second = deferred<string>();
    const states: ResumeSelectionState<string, string>[] = [];
    const selection = createLatestResumeSelection((file: string) => file === "a" ? first.promise : second.promise);

    const selectingA = selection.select("a", (state) => states.push(state));
    selection.invalidate();
    first.resolve("ignored");
    await selectingA;
    expect(states).toEqual([{ kind: "loading" }]);

    const selectingB = selection.select("b", (state) => states.push(state));
    second.resolve("parsed-b");
    await selectingB;
    expect(states.map((state) => state.kind)).toEqual(["loading", "loading", "ready"]);
  });

  it("revokes the previous ready token as soon as another file is selected", async () => {
    const first = deferred<string>();
    const second = deferred<string>();
    const states: ResumeSelectionState<string, string>[] = [];
    const selection = createLatestResumeSelection((file: string) => file === "a" ? first.promise : second.promise);

    const selectingA = selection.select("a", (state) => states.push(state));
    first.resolve("parsed-a");
    await selectingA;
    const ready = states.at(-1);
    expect(ready?.kind).toBe("ready");
    if (ready?.kind !== "ready") throw new Error("expected ready state");
    expect(selection.isCurrent(ready.token)).toBe(true);

    const selectingB = selection.select("b", (state) => states.push(state));
    expect(selection.isCurrent(ready.token)).toBe(false);
    second.resolve("parsed-b");
    await selectingB;
  });

  it("does not treat publisher exceptions as parser failures", async () => {
    let reads = 0;
    const selection = createLatestResumeSelection(async () => {
      reads += 1;
      return "parsed";
    });
    const publisherError = new Error("render failed");

    await expect(selection.select("a", () => {
      throw publisherError;
    })).rejects.toBe(publisherError);
    expect(reads).toBe(0);
  });
});
