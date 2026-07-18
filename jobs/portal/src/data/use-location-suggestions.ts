import { useEffect, useMemo, useState } from "react";
import { LOCATION_SUGGESTIONS, mergeCareerSuggestions } from "./career-suggestions";

interface LocationDataset {
  locations?: string[];
}

let cachedLocations: string[] | null = null;
let pendingLocations: Promise<string[]> | null = null;

export function useLocationSuggestions(seed: string[]): string[] {
  const [officialLocations, setOfficialLocations] = useState<string[]>(cachedLocations || []);

  useEffect(() => {
    if (cachedLocations) return;
    pendingLocations ||= fetch(`${import.meta.env.BASE_URL}us-locations.json`)
      .then((response) => response.ok ? response.json() as Promise<LocationDataset> : { locations: [] })
      .then((dataset) => {
        cachedLocations = Array.isArray(dataset.locations) ? dataset.locations : [];
        return cachedLocations;
      })
      .catch(() => []);
    void pendingLocations.then(setOfficialLocations);
  }, []);

  return useMemo(
    () => mergeCareerSuggestions(seed, LOCATION_SUGGESTIONS, officialLocations),
    [officialLocations, seed],
  );
}
