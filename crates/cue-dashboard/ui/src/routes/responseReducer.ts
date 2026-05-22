// Pure reducer for cue_response_chunk events emitted by the daemon while a
// streaming LLM response is in flight.
//
// Codex flagged in the R11 final chain review: the inline reducer in
// Responses.tsx dropped the in-flight entry on the *finished* chunk WITHOUT
// appending its `partial_text`. Today the daemon emits empty `partial_text`
// in the final chunk so this happens to work, but the contract is fragile.
// Round 12 hardens it: always append the delta first, then mark complete.
//
// Round 14 (R14.4 + R14.5) extends the contract for Bluey Auto:
// - `replace_body: true` on a chunk means "replace the entire card body
//   with this text, do not append" — used by the SpeculativeRouter when the
//   Deep lane completes and we want to swap the draft for the refined
//   answer cleanly.
// - `router_meta` carries Auto Router classification + chosen provider so
//   the UI can render a lane badge.
//
// The function is pure so we can unit-test it deterministically without
// driving Tauri events.

export type CueResponseKind = "answer" | "suggestion" | "recap";

/** Auto Router metadata mirrored from the daemon's RouterMeta. */
export interface RouterMeta {
  task_type: string;
  latency_lane: string;
  provider_lane: string;
  provider_name: string;
  model: string;
  confidence: number;
}

export interface InflightResponse {
  kind: CueResponseKind;
  text: string;
  /** True once the daemon has emitted a chunk with `finished: true`. */
  done: boolean;
  /** Auto Router classification + provider for this entry, if known. */
  routerMeta?: RouterMeta;
  cost_cents?: number | null;
  balance_cents_after?: number | null;
  provider?: string | null;
  model?: string | null;
  cost_label?: string | null;
  artifact_type?: string | null;
  artifact_body?: string | null;
  artifact_confidence?: number | null;
  /**
   * True if a `replace_body` chunk has fired for this entry. Used by the UI
   * to render a "refined" indicator distinguishing the deep answer from the
   * earlier draft.
   */
  refined?: boolean;
}

export interface CueResponseChunk {
  response_id: string;
  kind?: CueResponseKind;
  partial_text: string;
  finished: boolean;
  /**
   * When true, the chunk REPLACES the entire current text instead of
   * appending. Used by the SpeculativeRouter Deep lane Final chunk so the
   * draft → final transition is a clean swap instead of a "[refined]\n…"
   * concatenation hack.
   */
  replace_body?: boolean;
  /**
   * Auto Router metadata. Daemon emits this on the FIRST chunk only;
   * subsequent chunks carry None. The reducer pins it onto the inflight
   * entry so it persists across all chunks.
   */
  router_meta?: RouterMeta;
  cost_cents?: number | null;
  balance_cents_after?: number | null;
  provider?: string | null;
  model?: string | null;
  cost_label?: string | null;
  artifact_type?: string | null;
  artifact_body?: string | null;
  artifact_confidence?: number | null;
}

/**
 * Apply a streaming chunk to the in-flight map.
 *
 * Contract:
 *   1. The chunk's `partial_text` is a DELTA (just the new bytes), not the
 *      cumulative text. The reducer concatenates onto the running buffer.
 *   2. On `finished: true`, the delta is STILL appended first, then the
 *      entry is marked done. The entry stays in the map so the UI can
 *      render the full final text until the matching `cue_response`
 *      event replaces it; the caller is responsible for cleanup.
 *   3. If a chunk arrives with a `kind` field, it overrides any prior
 *      kind for that response_id (the daemon may upgrade a partial answer
 *      from "answer" to a more specific kind).
 *   4. Out-of-order chunks (impossible in our daemon today, but possible
 *      if the underlying transport ever reordered) are appended in the
 *      order received — we have no sequence numbers to reorder by.
 *   5. NEW (R14.4): if `replace_body: true`, the entry's text is set to
 *      `partial_text` instead of being appended; the entry is also
 *      marked `refined: true` for UI hinting.
 *   6. NEW (R14.5): if `router_meta` is present, it is stored on the
 *      entry; subsequent chunks without `router_meta` keep the previously
 *      stored value.
 */
export function applyChunk(
  prev: ReadonlyMap<string, InflightResponse>,
  chunk: CueResponseChunk,
): Map<string, InflightResponse> {
  const next = new Map(prev);
  const existing = next.get(chunk.response_id);

  const text = chunk.replace_body
    ? chunk.partial_text
    : (existing?.text ?? "") + chunk.partial_text;

  const kind: CueResponseKind = chunk.kind ?? existing?.kind ?? "answer";

  // Sticky router_meta: keep prior value if this chunk does not include one.
  const routerMeta = chunk.router_meta ?? existing?.routerMeta;
  const cost_cents = chunk.cost_cents ?? existing?.cost_cents;
  const balance_cents_after = chunk.balance_cents_after ?? existing?.balance_cents_after;
  const provider = chunk.provider ?? existing?.provider;
  const model = chunk.model ?? existing?.model;
  const cost_label = chunk.cost_label ?? existing?.cost_label;
  const artifact_type = chunk.artifact_type ?? existing?.artifact_type;
  const artifact_body = chunk.artifact_body ?? existing?.artifact_body;
  const artifact_confidence = chunk.artifact_confidence ?? existing?.artifact_confidence;

  // Refined flag: sticky once set true.
  const refined = chunk.replace_body || existing?.refined || false;

  next.set(chunk.response_id, {
    kind,
    text,
    done: chunk.finished,
    routerMeta,
    cost_cents,
    balance_cents_after,
    provider,
    model,
    cost_label,
    artifact_type,
    artifact_body,
    artifact_confidence,
    refined,
  });

  return next;
}

/**
 * Remove an entry from the inflight map. Called by the `cue_response` event
 * handler once the daemon has emitted the final, persisted response record.
 */
export function clearInflight(
  prev: ReadonlyMap<string, InflightResponse>,
  response_id: string,
): Map<string, InflightResponse> {
  const next = new Map(prev);
  next.delete(response_id);
  return next;
}
