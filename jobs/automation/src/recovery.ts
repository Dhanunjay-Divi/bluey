export type DurableRunPhase =
  | "prepared"
  | "needs_input"
  | "provider_review"
  | "final_submit_started"
  | "final_submit_activated"
  | "side_effect_unknown";

export type RestartDisposition = "restore" | "expired" | "side_effect_unknown";

const SAFE_RESTART_PHASES = new Set<DurableRunPhase>([
  "prepared",
  "needs_input",
  "provider_review",
]);

/**
 * Decide whether a durable browser run can be restored after a process crash.
 * A durable final-submit marker always wins over a stale pre-submit checkpoint:
 * crashes between those two writes must never turn into an automatic replay.
 */
export function restartDisposition(
  phase: DurableRunPhase,
  expiresAtMs: number,
  nowMs: number = Date.now(),
  irreversibleMarkerExists = false,
): RestartDisposition {
  if (irreversibleMarkerExists || !SAFE_RESTART_PHASES.has(phase)) {
    return "side_effect_unknown";
  }
  if (!Number.isSafeInteger(expiresAtMs) || expiresAtMs <= nowMs) return "expired";
  return "restore";
}

export function isDurableRunPhase(value: unknown): value is DurableRunPhase {
  return typeof value === "string" && [
    "prepared",
    "needs_input",
    "provider_review",
    "final_submit_started",
    "final_submit_activated",
    "side_effect_unknown",
  ].includes(value);
}
