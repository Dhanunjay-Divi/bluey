import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LiveTranscriptList, TranscriptSegment } from "../components/LiveTranscriptList";

const MAX_SEGMENTS = 200;

export function LiveTranscript() {
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const sessionRef = useRef<string | null>(null);

  // Catch-up: load existing segments on mount.
  useEffect(() => {
    invoke<TranscriptSegment[]>("get_live_transcripts", { sinceIndex: 0 })
      .then((segs) => {
        if (segs.length > 0) {
          sessionRef.current = segs[0].session_id;
          setSegments(segs.slice(-MAX_SEGMENTS));
        }
      })
      .catch((e) => console.warn("get_live_transcripts failed:", e));
  }, []);

  // Subscribe to live events.
  const handleEvent = useCallback((seg: TranscriptSegment) => {
    // Session change: clear and start fresh.
    if (sessionRef.current && seg.session_id !== sessionRef.current) {
      setSegments([seg]);
      sessionRef.current = seg.session_id;
      return;
    }
    sessionRef.current = seg.session_id;
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
