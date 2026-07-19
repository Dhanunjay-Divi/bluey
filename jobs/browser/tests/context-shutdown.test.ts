import { describe, expect, it } from "vitest";
import { closeBrowserContexts } from "../src/context-shutdown.js";

describe("explicit browser shutdown", () => {
  it("attempts every isolated context even when one close fails", async () => {
    const closed: string[] = [];
    await closeBrowserContexts([
      { close: async () => { closed.push("first"); } },
      { close: async () => { closed.push("second"); throw new Error("close failed"); } },
      { close: async () => { closed.push("third"); } },
    ]);
    expect(closed.sort()).toEqual(["first", "second", "third"]);
  });
});
