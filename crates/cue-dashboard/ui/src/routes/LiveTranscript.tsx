import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LiveTranscriptList, TranscriptSegment } from "../components/LiveTranscriptList";

const MAX_SEGMENTS = 200;

/** Unique key for dedup: (session_id, index) */
function segKey(seg: TranscriptSegment): string {
  return `${seg.session_id}:${seg.index ?? -1}`;
}

export function LiveTranscript() {
  // Map keyed by "session_id:index" for O(1) dedup on both catch-up and live.
  const segMapRef = useRef<Map<string, TranscriptSegment>>(new Map());
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const sessionRef = useRef<string | null>(null);

  const rebuildFromMap = useCallback(() => {
    const sorted = Array.from(segMapRef.current.values()).sort(
      (a, b) => (a.index ?? 0) - (b.index ?? 0),
    );
    const tail = sorted.length > MAX_SEGMENTS ? sorted.slice(-MAX_SEGMENTS) : sorted;
    setSegments(tail);
  }, []);

  // Catch-up: load existing segments on mount.
  useEffect(() => {
    invoke<TranscriptSegment[]>("get_live_transcripts", { sinceIndex: 0 })
      .then((segs) => {
        if (segs.length === 0) return;
        sessionRef.current = segs[0].session_id;
        const map = segMapRef.current;
        for (const seg of segs) {
          map.set(segKey(seg), seg);
        }
        rebuildFromMap();
      })
      .catch((e) => console.warn("get_live_transcripts failed:", e));
  }, [rebuildFromMap]);

  // Subscribe to live events with {session_id, index} dedup.
  const handleEvent = useCallback((seg: TranscriptSegment) => {
    // Session change: clear map, start fresh.
    if (sessionRef.current && seg.session_id !== sessionRef.current) {
      segMapRef.current.clear();
      sessionRef.current = seg.session_id;
    }
    sessionRef.current = seg.session_id;

    const key = segKey(seg);
    const map = segMapRef.current;
    // Only insert if not already present (dedup).
    if (!map.has(key)) {
      map.set(key, seg);
      // Trim oldest if over limit.
      if (map.size > MAX_SEGMENTS) {
        const firstKey = map.keys().next().value;
        if (firstKey !== undefined) map.delete(firstKey);
      }
      rebuildFromMap();
    }
  }, [rebuildFromMap]);

  useEffect(() => {
    const unlisten = listen<TranscriptSegment>("live_transcript", (event) => {
      handleEvent(event.payload);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [handleEvent]);

  if (segments.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center text-zinc-500">
        <p>No active session. Start one with Cmd+Shift+L.</p>
      </div>
    );
  }

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="border-b border-zinc-800 px-4 py-2">
        <h2 className="text-sm font-semibold text-zinc-300">Live Transcript</h2>
      </div>
      <LiveTranscriptList segments={segments} />
    </div>
  );
}
