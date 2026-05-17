// Pure reducer for cue_response_chunk events emitted by the daemon while a
// streaming LLM response is in flight.
//
// Codex flagged in the R11 final chain review: the inline reducer in
// Responses.tsx dropped the in-flight entry on the *finished* chunk WITHOUT
// appending its `partial_text`. Today the daemon emits empty `partial_text`
// in the final chunk so this happens to work, but the contract is fragile.
// Round 12 hardens it: always append the delta first, then mark complete.
//
// The function is pure so we can unit-test it deterministically without
// driving Tauri events.

export type CueResponseKind = "answer" | "suggestion" | "recap";

export interface InflightResponse {
  kind: CueResponseKind;
  text: string;
  /** True once the daemon has emitted a chunk with `finished: true`. */
  done: boolean;
}

export interface CueResponseChunk {
  response_id: string;
  kind?: CueResponseKind;
  partial_text: string;
  finished: boolean;
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
 *      order received \u2014 we have no sequence numbers to reorder by.
 */
export function applyChunk(
  prev: ReadonlyMap<string, InflightResponse>,
  chunk: CueResponseChunk,
): Map<string, InflightResponse> {
  const next = new Map(prev);
  const existing = next.get(chunk.response_id);

  // Always append the delta, even on the finished chunk. This preserves any
  // trailing text the daemon includes in the last chunk (punctuation, final
  // tokens) instead of silently dropping it.
  const text = (existing?.text ?? "") + chunk.partial_text;
  const kind: CueResponseKind = chunk.kind ?? existing?.kind ?? "answer";

  next.set(chunk.response_id, {
    kind,
    text,
    done: chunk.finished,
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
