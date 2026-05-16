import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LiveTranscriptList, TranscriptSegment } from "../components/LiveTranscriptList";

const MAX_SEGMENTS = 200;

export function LiveTranscript() {
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const sessionRef = useRef<string | null>(null);
  // Per-session highest index already rendered. Live events with
  // `index <= lastSeenIndexRef.current` are duplicates of catch-up
  // segments and must be skipped to avoid the catch-up/poller race.
  const lastSeenIndexRef = useRef<number>(-1);

  // Catch-up: load existing segments on mount.
  useEffect(() => {
    invoke<TranscriptSegment[]>("get_live_transcripts", { sinceIndex: 0 })
      .then((segs) => {
        if (segs.length === 0) return;
        sessionRef.current = segs[0].session_id;
        const tail = segs.slice(-MAX_SEGMENTS);
        // Highest index in catch-up — anything <= this is a duplicate.
        lastSeenIndexRef.current = Math.max(
          ...tail.map((s) => s.index ?? -1),
          -1,
        );
        setSegments(tail);
      })
      .catch((e) => console.warn("get_live_transcripts failed:", e));
  }, []);

  // Subscribe to live events with index-based dedup.
  const handleEvent = useCallback((seg: TranscriptSegment) => {
    // Session change: clear, start fresh, reset cursor.
    if (sessionRef.current && seg.session_id !== sessionRef.current) {
      sessionRef.current = seg.session_id;
      lastSeenIndexRef.current = seg.index ?? -1;
      setSegments([seg]);
      return;
    }
    sessionRef.current = seg.session_id;

    // Skip duplicates: catch-up may have already rendered this index.
    const incomingIdx = seg.index ?? -1;
    if (incomingIdx >= 0 && incomingIdx <= lastSeenIndexRef.current) {
      return;
    }
    lastSeenIndexRef.current = Math.max(lastSeenIndexRef.current, incomingIdx);

    setSegments((prev) => {
      const next = [...prev, seg];
      return next.length > MAX_SEGMENTS ? next.slice(-MAX_SEGMENTS) : next;
    });
  }, []);

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
