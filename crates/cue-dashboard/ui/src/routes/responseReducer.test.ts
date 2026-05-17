import { describe, expect, it } from "vitest";
import {
  applyChunk,
  clearInflight,
  type CueResponseChunk,
  type InflightResponse,
} from "./responseReducer";

const empty = (): ReadonlyMap<string, InflightResponse> => new Map();

const chunk = (
  partial_text: string,
  finished: boolean,
  response_id = "r1",
  kind?: "answer" | "suggestion" | "recap",
): CueResponseChunk => ({
  response_id,
  partial_text,
  finished,
  kind,
});

describe("responseReducer.applyChunk", () => {
  it("appends a single delta into a fresh entry", () => {
    const next = applyChunk(empty(), chunk("Hello", false));
    expect(next.get("r1")).toEqual({
      kind: "answer",
      text: "Hello",
      done: false,
    });
  });

  it("concatenates multiple deltas in order", () => {
    let m = applyChunk(empty(), chunk("Hello", false));
    m = applyChunk(m, chunk(" ", false));
    m = applyChunk(m, chunk("world", false));
    m = applyChunk(m, chunk("!", true));
    expect(m.get("r1")).toEqual({
      kind: "answer",
      text: "Hello world!",
      done: true,
    });
  });

  it("appends the final delta on finished:true (does NOT drop it)", () => {
    // Regression for the codex-flagged bug: the inline reducer in
    // Responses.tsx would `next.delete(response_id)` on finished, losing
    // any text in that final chunk.
    let m = applyChunk(empty(), chunk("Hello", false));
    m = applyChunk(m, chunk(" world", true)); // non-empty final delta
    const final = m.get("r1");
    expect(final?.text).toBe("Hello world");
    expect(final?.done).toBe(true);
  });

  it("handles empty partial_text on finished:true without corruption", () => {
    let m = applyChunk(empty(), chunk("complete answer", false));
    m = applyChunk(m, chunk("", true));
    expect(m.get("r1")?.text).toBe("complete answer");
    expect(m.get("r1")?.done).toBe(true);
  });

  it("respects a kind override from a later chunk", () => {
    let m = applyChunk(empty(), chunk("text", false, "r1", "answer"));
    m = applyChunk(m, chunk(" more", false, "r1", "suggestion"));
    expect(m.get("r1")?.kind).toBe("suggestion");
  });

  it("preserves prior kind when later chunk omits it", () => {
    let m = applyChunk(empty(), chunk("text", false, "r1", "recap"));
    m = applyChunk(m, chunk(" more", false));
    expect(m.get("r1")?.kind).toBe("recap");
  });

  it("defaults to 'answer' kind when never specified", () => {
    const m = applyChunk(empty(), chunk("text", false));
    expect(m.get("r1")?.kind).toBe("answer");
  });

  it("isolates entries by response_id", () => {
    let m = applyChunk(empty(), chunk("A1", false, "r1"));
    m = applyChunk(m, chunk("B1", false, "r2"));
    m = applyChunk(m, chunk(" A2", true, "r1"));
    m = applyChunk(m, chunk(" B2", true, "r2"));
    expect(m.get("r1")?.text).toBe("A1 A2");
    expect(m.get("r2")?.text).toBe("B1 B2");
  });

  it("never mutates the input map", () => {
    const before = new Map<string, InflightResponse>([
      ["r1", { kind: "answer", text: "seed", done: false }],
    ]);
    const beforeJson = JSON.stringify(Array.from(before.entries()));
    applyChunk(before, chunk(" added", false));
    const afterJson = JSON.stringify(Array.from(before.entries()));
    expect(afterJson).toBe(beforeJson);
  });

  it("treats out-of-order chunks deterministically (append in arrival order)", () => {
    // We have no sequence numbers; we rely on transport order. Verify the
    // reducer is at least deterministic given the order it sees.
    let m = applyChunk(empty(), chunk("part-three ", false));
    m = applyChunk(m, chunk("part-one ", false));
    m = applyChunk(m, chunk("part-two", true));
    expect(m.get("r1")?.text).toBe("part-three part-one part-two");
    expect(m.get("r1")?.done).toBe(true);
  });
});

describe("responseReducer.clearInflight", () => {
  it("removes the entry by response_id", () => {
    let m = applyChunk(empty(), chunk("text", true));
    m = clearInflight(m, "r1");
    expect(m.has("r1")).toBe(false);
  });

  it("is a no-op for unknown response_id", () => {
    let m = applyChunk(empty(), chunk("text", false));
    m = clearInflight(m, "does-not-exist");
    expect(m.get("r1")?.text).toBe("text");
  });

  it("does not mutate the input map", () => {
    const before = new Map<string, InflightResponse>([
      ["r1", { kind: "answer", text: "seed", done: false }],
    ]);
    const beforeJson = JSON.stringify(Array.from(before.entries()));
    clearInflight(before, "r1");
    const afterJson = JSON.stringify(Array.from(before.entries()));
    expect(afterJson).toBe(beforeJson);
  });
});
