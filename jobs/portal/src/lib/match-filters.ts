import type { JobPosting } from "../types";

export type MatchWorkplaceFilter = "all" | "remote" | "hybrid" | "on-site";
export type MatchDensity = "comfortable" | "compact";

export interface MatchFilterState {
  query: string;
  activeTrack: string;
  minimumScore: number;
  workplace: MatchWorkplaceFilter;
  onlyUnprepared: boolean;
  showOutsideTrack: boolean;
  showPassed: boolean;
  density: MatchDensity;
}

export const DEFAULT_MATCH_FILTERS: MatchFilterState = {
  query: "",
  activeTrack: "all",
  minimumScore: 0,
  workplace: "all",
  onlyUnprepared: false,
  showOutsideTrack: false,
  showPassed: false,
  density: "comfortable",
};

export function readMatchFilters(
  searchParams: URLSearchParams,
  activeTrackIds: ReadonlySet<string>,
): MatchFilterState {
  const requestedTrack = searchParams.get("track") || "all";
  const workplace = searchParams.get("workplace");
  const score = Number(searchParams.get("score") || 0);

  return {
    query: searchParams.get("q") || "",
    activeTrack: requestedTrack === "all" || activeTrackIds.has(requestedTrack)
      ? requestedTrack
      : "all",
    minimumScore: Number.isFinite(score)
      ? Math.min(100, Math.max(0, Math.round(score / 5) * 5))
      : 0,
    workplace: isWorkplaceFilter(workplace) ? workplace : "all",
    onlyUnprepared: searchParams.get("unprepared") === "1",
    showOutsideTrack: searchParams.get("outside") === "1",
    showPassed: searchParams.get("passed") === "1",
    density: searchParams.get("density") === "compact" ? "compact" : "comfortable",
  };
}

export function writeMatchFilters(
  current: URLSearchParams,
  filters: MatchFilterState,
): URLSearchParams {
  const next = new URLSearchParams();
  preservePreviewParameter(current, next, "preview");
  preservePreviewParameter(current, next, "scenario");

  const query = filters.query.trim() ? filters.query : "";
  if (query) next.set("q", query);
  if (filters.activeTrack !== "all") next.set("track", filters.activeTrack);
  if (filters.minimumScore > 0) next.set("score", String(filters.minimumScore));
  if (filters.workplace !== "all") next.set("workplace", filters.workplace);
  if (filters.onlyUnprepared) next.set("unprepared", "1");
  if (filters.showOutsideTrack) next.set("outside", "1");
  if (filters.showPassed) next.set("passed", "1");
  if (filters.density === "compact") next.set("density", "compact");

  return next;
}

export function hasMatchViewFilters(filters: MatchFilterState): boolean {
  return Boolean(normalizeMatchSearch(filters.query))
    || filters.minimumScore > 0
    || filters.workplace !== "all"
    || filters.onlyUnprepared;
}

export function filterMatches(
  matches: JobPosting[],
  filters: MatchFilterState,
  preparedJobIds: ReadonlySet<string>,
): JobPosting[] {
  const needle = normalizeMatchSearch(filters.query);

  return matches.filter((job) => {
    const matchesTrack = filters.activeTrack === "all"
      || job.track_id === filters.activeTrack;
    const haystack = normalizeMatchSearch(
      `${job.company} ${job.title} ${job.location} ${job.workplace}`,
    );
    const matchesQuery = !needle || haystack.includes(needle);
    const matchesScore = job.match_score >= filters.minimumScore;
    const matchesWorkplace = filters.workplace === "all"
      || normalizeWorkplace(job.workplace) === filters.workplace;
    const matchesPacket = !filters.onlyUnprepared || !preparedJobIds.has(job.id);

    return matchesTrack
      && matchesQuery
      && matchesScore
      && matchesWorkplace
      && matchesPacket;
  });
}

export function normalizeMatchSearch(value: string): string {
  return value.trim().toLowerCase().replace(/\s+/g, " ");
}

export function normalizeWorkplace(value: string): MatchWorkplaceFilter | "unknown" {
  const normalized = normalizeMatchSearch(value).replace(/[_–—]/g, "-");
  if (normalized.includes("remote")) return "remote";
  if (normalized.includes("hybrid")) return "hybrid";
  if (
    normalized.includes("on-site")
    || normalized.includes("onsite")
    || normalized.includes("in-person")
    || normalized.includes("in person")
    || normalized.includes("office")
  ) {
    return "on-site";
  }
  return "unknown";
}

export function clearMatchViewFilters(filters: MatchFilterState): MatchFilterState {
  return {
    ...filters,
    query: "",
    minimumScore: 0,
    workplace: "all",
    onlyUnprepared: false,
  };
}

function isWorkplaceFilter(value: string | null): value is MatchWorkplaceFilter {
  return value === "all" || value === "remote" || value === "hybrid" || value === "on-site";
}

function preservePreviewParameter(
  current: URLSearchParams,
  next: URLSearchParams,
  key: "preview" | "scenario",
): void {
  const value = current.get(key);
  if (value) next.set(key, value);
}
