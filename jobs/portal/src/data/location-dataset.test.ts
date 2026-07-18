import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";

interface LocationDataset {
  count: number;
  locations: string[];
}

describe("official US location suggestions", () => {
  it("includes nationwide city, state, and territory choices", async () => {
    const path = new URL("../../public/us-locations.json", import.meta.url);
    const dataset = JSON.parse(await readFile(path, "utf8")) as LocationDataset;

    expect(dataset.count).toBeGreaterThan(32_000);
    expect(dataset.locations).toContain("Arlington, VA");
    expect(dataset.locations).toContain("Truth or Consequences, NM");
    expect(dataset.locations).toContain("Virginia");
    expect(dataset.locations).toContain("VA");
    expect(dataset.locations).toContain("Puerto Rico");
  });
});
