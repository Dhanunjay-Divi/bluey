import { useEffect, useRef } from "react";
import { Mic, Monitor } from "lucide-react";

export interface TranscriptSegment {
  /** Absolute index of this segment in the active session’s transcript.
   *  Used by `LiveTranscript` to dedup catch-up vs live events. */
  index?: number;
  session_id: string;
  source: string;
  text: string;
  is_final: boolean;
  speaker: number | null;
  ts_ms: number;
}

interface Props {
  segments: TranscriptSegment[];
}

export function LiveTranscriptList({ segments }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const userScrolledUp = useRef(false);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const handleScroll = () => {
      const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
      userScrolledUp.current = distFromBottom > 100;
    };
    el.addEventListener("scroll", handleScroll);
    return () => el.removeEventListener("scroll", handleScroll);
  }, []);

  useEffect(() => {
    if (!userScrolledUp.current && containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [segments.length]);

  return (
    <div
      ref={containerRef}
      className="flex-1 overflow-y-auto space-y-1 p-4 font-mono text-sm"
    >
      {segments.map((seg, i) => (
        <div key={`${seg.ts_ms}-${i}`} className="flex items-start gap-2">
          <span className="mt-0.5 shrink-0">
            {seg.source === "microphone" ? (
              <Mic size={14} className="text-accent-subtle-text" />
            ) : (
              <Monitor size={14} className="text-success" />
            )}
          </span>
          <span
            className={
              seg.is_final
                ? "text-text-primary"
                : "text-text-tertiary italic"
            }
          >
            {seg.text}
          </span>
          <span className="ml-auto shrink-0 text-footnote text-text-quaternary">
            {new Date(seg.ts_ms).toLocaleTimeString()}
          </span>
        </div>
      ))}
    </div>
  );
}
