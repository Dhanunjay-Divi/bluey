import { describe, expect, it } from "vitest";
import {
  applyChunk,
  clearInflight,
  responseBelongsToSession,
  shouldApplyResponseLoad,
  type CueResponseChunk,
  type InflightResponse,
  type RouterMeta,
} from "./responseReducer";

const empty = (): ReadonlyMap<string, InflightResponse> => new Map();

const chunk = (
  partial_text: string,
  finished: boolean,
  response_id = "r1",
  kind?: "answer" | "suggestion" | "recap",
  extras: Partial<CueResponseChunk> = {},
): CueResponseChunk => ({
  response_id,
  source_session_id: "session-a",
  partial_text,
  finished,
  kind,
  ...extras,
});

const meta: RouterMeta = {
  task_type: "system_design",
  latency_lane: "deep",
  provider_lane: "deep",
  provider_name: "anthropic",
  model: "claude-3-7-sonnet-latest",
  confidence: 0.9,
};

describe("responseReducer.applyChunk", () => {
  it("appends a single delta into a fresh entry", () => {
    const next = applyChunk(empty(), chunk("Hello", false));
    expect(next.get("r1")).toEqual({
      kind: "answer",
      text: "Hello",
      done: false,
      routerMeta: undefined,
      refined: false,
    });
  });

  it("concatenates multiple deltas in order", () => {
    let m = applyChunk(empty(), chunk("Hello", false));
    m = applyChunk(m, chunk(" ", false));
    m = applyChunk(m, chunk("world", false));
    m = applyChunk(m, chunk("!", true));
    expect(m.get("r1")?.text).toBe("Hello world!");
    expect(m.get("r1")?.done).toBe(true);
  });

  it("appends the final delta on finished:true (does NOT drop it)", () => {
    let m = applyChunk(empty(), chunk("Hello", false));
    m = applyChunk(m, chunk(" world", true));
    expect(m.get("r1")?.text).toBe("Hello world");
    expect(m.get("r1")?.done).toBe(true);
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

  // ─── R14.4: replace_body semantics ───────────────────────────────────────
  describe("replace_body", () => {
    it("replaces text instead of appending when replace_body is true", () => {
      let m = applyChunk(empty(), chunk("draft v1", false));
      m = applyChunk(m, chunk(" draft v2", false));
      m = applyChunk(
        m,
        chunk("REFINED FINAL ANSWER", true, "r1", undefined, {
          replace_body: true,
        }),
      );
      expect(m.get("r1")?.text).toBe("REFINED FINAL ANSWER");
      expect(m.get("r1")?.refined).toBe(true);
      expect(m.get("r1")?.done).toBe(true);
    });

    it("refined flag is sticky once set", () => {
      let m = applyChunk(
        empty(),
        chunk("REFINED", true, "r1", undefined, { replace_body: true }),
      );
      // A subsequent normal chunk for the same id (race / late delta)
      // should not unset refined.
      m = applyChunk(m, chunk(" stray", false));
      expect(m.get("r1")?.refined).toBe(true);
    });

    it("default chunk does not mark refined", () => {
      const m = applyChunk(empty(), chunk("hello", true));
      expect(m.get("r1")?.refined).toBe(false);
    });
  });

  // ─── R14.5: router_meta sticky behaviour ────────────────────────────────
  describe("router_meta", () => {
    it("stores router_meta on the entry when supplied", () => {
      const m = applyChunk(
        empty(),
        chunk("hello", false, "r1", undefined, { router_meta: meta }),
      );
      expect(m.get("r1")?.routerMeta).toEqual(meta);
    });

    it("keeps the prior router_meta when a later chunk omits it", () => {
      let m = applyChunk(
        empty(),
        chunk("hello", false, "r1", undefined, { router_meta: meta }),
      );
      m = applyChunk(m, chunk(" world", true)); // no router_meta
      expect(m.get("r1")?.routerMeta).toEqual(meta);
    });

    it("a later chunk's router_meta wins if both are set", () => {
      const meta2: RouterMeta = { ...meta, provider_name: "openai" };
      let m = applyChunk(
        empty(),
        chunk("hello", false, "r1", undefined, { router_meta: meta }),
      );
      m = applyChunk(
        m,
        chunk(" world", true, "r1", undefined, { router_meta: meta2 }),
      );
      expect(m.get("r1")?.routerMeta?.provider_name).toBe("openai");
    });
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
});

describe("response session isolation", () => {
  it("rejects a delayed A history load after the active session switches to B", () => {
    const delayedA = { generation: 1, sessionId: "session-a" };
    const currentB = { generation: 2, sessionId: "session-b" };

    expect(
      shouldApplyResponseLoad(currentB.generation, currentB.sessionId, delayedA),
    ).toBe(false);
    expect(
      shouldApplyResponseLoad(currentB.generation, currentB.sessionId, currentB),
    ).toBe(true);
  });

  it("drops A chunks and finals that arrive after an in-flight switch to B", () => {
    expect(responseBelongsToSession("session-b", "session-a")).toBe(false);
    expect(responseBelongsToSession("session-b", "session-b")).toBe(true);
    expect(responseBelongsToSession(null, "session-a")).toBe(false);
  });
});
