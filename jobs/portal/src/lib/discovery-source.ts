import type { DiscoverySource, DiscoverySourceHealth } from "../types";

export const DISCOVERY_SOURCE_STALE_AFTER_MS = 12 * 60 * 60 * 1_000;

export function discoverySourceState(
  source: Pick<DiscoverySource, "status" | "health" | "last_success_at_ms">,
  nowMs = Date.now(),
): DiscoverySourceHealth {
  if (source.status === "paused") return "paused";
  if (source.health !== "healthy") return source.health;
  if (!source.last_success_at_ms) return "waiting";
  if (source.last_success_at_ms < nowMs - DISCOVERY_SOURCE_STALE_AFTER_MS) return "degraded";
  return "healthy";
}

export function discoverySourceAction(state: DiscoverySourceHealth): string {
  if (state === "degraded") {
    return "Updates are delayed. Bluey is retrying; add an urgent job link meanwhile.";
  }
  if (state === "paused") return "Contact support to resume it. Paste urgent roles meanwhile.";
  if (state === "waiting") return "Waiting for the first sync.";
  return "No action needed.";
}
