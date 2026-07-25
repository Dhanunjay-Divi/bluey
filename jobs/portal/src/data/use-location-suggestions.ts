import { useEffect, useMemo, useState } from "react";
import { LOCATION_SUGGESTIONS, mergeCareerSuggestions } from "./career-suggestions";

interface LocationDataset {
  locations?: string[];
}

let cachedLocations: string[] | null = null;
let pendingLocations: Promise<string[]> | null = null;

export async function loadOfficialLocations(
  fetcher: typeof fetch = fetch,
  baseUrl = import.meta.env.BASE_URL,
): Promise<string[]> {
  if (cachedLocations) return cachedLocations;
  if (pendingLocations) return pendingLocations;

  pendingLocations = (async () => {
    let lastError: unknown;
    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        const response = await fetcher(`${baseUrl}us-locations.json`);
        if (!response.ok) throw new Error(`Location dataset returned ${response.status}`);
        const dataset = await response.json() as LocationDataset;
        const locations = Array.isArray(dataset.locations)
          ? dataset.locations.filter((value): value is string => typeof value === "string" && Boolean(value.trim()))
          : [];
        if (!locations.length) throw new Error("Location dataset is empty");
        cachedLocations = locations;
        return locations;
      } catch (error) {
        lastError = error;
      }
    }
    throw lastError instanceof Error ? lastError : new Error("Location suggestions are unavailable");
  })().finally(() => {
    pendingLocations = null;
  });

  return pendingLocations;
}

export function resetOfficialLocationsForTests(): void {
  cachedLocations = null;
  pendingLocations = null;
}

export function useLocationSuggestions(seed: string[]): string[] {
  const [officialLocations, setOfficialLocations] = useState<string[]>(cachedLocations || []);

  useEffect(() => {
    if (cachedLocations) return;
    void loadOfficialLocations().then(setOfficialLocations).catch(() => {
      // Built-in locations remain available; a later mount retries the full dataset.
    });
  }, []);

  return useMemo(
    () => mergeCareerSuggestions(seed, LOCATION_SUGGESTIONS, officialLocations),
    [officialLocations, seed],
  );
}
