import { describe, expect, it } from "vitest";
import { shouldApplySearchResponse } from "./searchRequestState";

describe("search request isolation", () => {
  it("rejects a delayed A response after the input advances to B", () => {
    const delayedA = { generation: 1, query: "alpha" };
    const currentB = { generation: 2, query: "beta" };

    expect(
      shouldApplySearchResponse(
        currentB.generation,
        currentB.query,
        delayedA,
        false,
      ),
    ).toBe(false);
    expect(
      shouldApplySearchResponse(
        currentB.generation,
        currentB.query,
        currentB,
        false,
      ),
    ).toBe(true);
  });

  it("rejects an aborted response even if its generation still matches", () => {
    const current = { generation: 4, query: "customer feedback" };
    expect(
      shouldApplySearchResponse(
        current.generation,
        current.query,
        current,
        true,
      ),
    ).toBe(false);
  });
});
