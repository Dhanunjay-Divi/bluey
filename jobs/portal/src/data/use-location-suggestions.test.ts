import { afterEach, describe, expect, it, vi } from "vitest";
import {
  loadOfficialLocations,
  resetOfficialLocationsForTests,
} from "./use-location-suggestions";

describe("official location suggestion loading", () => {
  afterEach(() => {
    resetOfficialLocationsForTests();
  });

  it("retries a transient dataset failure", async () => {
    const fetcher = vi.fn()
      .mockResolvedValueOnce(new Response("", { status: 503 }))
      .mockResolvedValueOnce(new Response(
        JSON.stringify({ locations: ["Arlington, VA", "Austin, TX"] }),
        { status: 200 },
      ));

    await expect(loadOfficialLocations(fetcher, "/jobs/")).resolves.toEqual([
      "Arlington, VA",
      "Austin, TX",
    ]);
    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(fetcher).toHaveBeenLastCalledWith("/jobs/us-locations.json");
  });

  it("does not cache an empty or failed response", async () => {
    const unavailable = vi.fn()
      .mockImplementation(async () => (
        new Response(JSON.stringify({ locations: [] }), { status: 200 })
      ));
    await expect(loadOfficialLocations(unavailable, "/jobs/")).rejects.toThrow(
      "Location dataset is empty",
    );

    const recovered = vi.fn()
      .mockResolvedValue(new Response(
        JSON.stringify({ locations: ["Falls Church, VA"] }),
        { status: 200 },
      ));
    await expect(loadOfficialLocations(recovered, "/jobs/")).resolves.toEqual([
      "Falls Church, VA",
    ]);
  });
});
