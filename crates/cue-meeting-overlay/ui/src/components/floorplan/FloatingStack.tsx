// The floating control stack — bottom-left vertical column of circular
// vector-icon buttons, the Open Floor Plan's replacement for the composer row.
//
//   System audio · Mic (your voice) · Screenshot · Attach · Capture page · Ask
//
// The Ask button is the dark anchor; tapping it springs open an inline input to
// its right (no modal). Attached context chips (screenshots/files) float just
// above the stack so you see what's queued. Every control maps to an existing
// client method — no new backend. Motion is spring-y and reduced-motion safe.

import { useEffect, useRef, useState } from "react";
import type { ListeningState } from "../../lib/types";
import {
  AlertIcon,
  AttachIcon,
  GlobeIcon,
  MicIcon,
  MicOffIcon,
  SendIcon,
  SparkleIcon,
  SpinnerIcon,
  StopIcon,
  SystemAudioIcon,
} from "../icons";

export function FloatingStack({
  listenState,
  micInputOn,
  askStreaming,
  agentName,
  onToggleSystemAudio,
  onToggleMic,
  onScreenshot,
  onAttach,
  onCapturePage,
  onAsk,
  onAskRecent,
}: {
  listenState: ListeningState;
  micInputOn: boolean;
  askStreaming: boolean;
  agentName?: string;
  onToggleSystemAudio: () => void;
  onToggleMic: () => void;
  onScreenshot: () => void;
  onAttach: () => void;
  onCapturePage: () => void;
  onAsk: (question: string) => void;
  /** Single-click on Ask → answer the most recent meeting question immediately
   *  (no input). Double-click opens the typing bar. */
  onAskRecent: () => void;
}) {
  const [asking, setAsking] = useState(false);
  const [text, setText] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  // Disambiguate single vs double click on the Ask anchor: a single click fires
  // after a short delay UNLESS a second click arrives first (→ double).
  const clickTimer = useRef<number | null>(null);

  useEffect(() => {
    if (asking) inputRef.current?.focus();
  }, [asking]);

  useEffect(
    () => () => {
      if (clickTimer.current) window.clearTimeout(clickTimer.current);
    },
    [],
  );

  const submit = () => {
    const q = text.trim();
    if (!q) {
      setAsking(false);
      return;
    }
    onAsk(q);
    setText("");
    setAsking(false);
  };

  // Ask anchor click routing. If the typing bar is already open, a click just
  // closes it. Otherwise: wait ~230ms; if no second click lands, treat as a
  // SINGLE click → answer the recent question. A second click cancels the timer
  // and opens the typing bar (DOUBLE click).
  const onAskClick = () => {
    if (askStreaming) return;
    if (asking) {
      setAsking(false);
      return;
    }
    if (clickTimer.current) {
      // second click → double: cancel the pending single, open the input.
      window.clearTimeout(clickTimer.current);
      clickTimer.current = null;
      setAsking(true);
      return;
    }
    clickTimer.current = window.setTimeout(() => {
      clickTimer.current = null;
      onAskRecent();
    }, 230);
  };

  const sys = systemAudioMeta(listenState);

  return (
    <div className="fp-stack">
      <div className="fp-stack-row">
        <div className="fp-stack-col">
          <StackButton
            label={sys.label}
            active={listenState === "listening"}
            tone={sys.tone}
            onClick={onToggleSystemAudio}
          >
            {sys.glyph}
          </StackButton>

          <StackButton
            label={micInputOn ? "Mute your mic" : "Capture your voice"}
            active={micInputOn}
            tone={micInputOn ? "accent" : undefined}
            onClick={onToggleMic}
          >
            {/* Distinct on/off glyph — a slashed mic when muted — plus a
                pop animation keyed to the state so the toggle feels alive. */}
            <span key={micInputOn ? "on" : "off"} className="fp-mic-glyph">
              {micInputOn ? <MicIcon size={17} /> : <MicOffIcon size={17} />}
            </span>
          </StackButton>

          <StackButton label="Take a screenshot" onClick={onScreenshot}>
            <ScreenshotGlyph />
          </StackButton>

          <StackButton label="Attach files" onClick={onAttach}>
            <AttachIcon size={17} />
          </StackButton>

          <StackButton label="Capture page" onClick={onCapturePage}>
            <GlobeIcon size={17} />
          </StackButton>

          <StackButton
            label={
              agentName
                ? `Ask ${agentName} — click to answer the last question, double-click to type`
                : "Click to answer the last question, double-click to type"
            }
            anchor
            active={asking}
            disabled={askStreaming}
            onClick={onAskClick}
          >
            <SparkleIcon size={17} />
          </StackButton>
        </div>

        {/* the Ask input springs open beside the stack */}
        {asking && (
          <form
            className="fp-ask-form"
            onSubmit={(e) => {
              e.preventDefault();
              submit();
            }}
          >
            <input
              ref={inputRef}
              className="fp-ask-input"
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") setAsking(false);
              }}
              onBlur={() => {
                if (!text.trim()) setAsking(false);
              }}
              placeholder="Ask Bluey…"
            />
            <button
              type="submit"
              className="fp-ask-send"
              aria-label="Send"
              disabled={!text.trim()}
            >
              <SendIcon size={16} />
            </button>
          </form>
        )}
      </div>
    </div>
  );
}

function StackButton({
  children,
  label,
  active,
  anchor,
  tone,
  disabled,
  onClick,
}: {
  children: React.ReactNode;
  label: string;
  active?: boolean;
  anchor?: boolean;
  tone?: "accent" | "live";
  disabled?: boolean;
  onClick: () => void;
}) {
  // A transient tap-pop on every click (the subtle "dopamine" feedback). The
  // class is toggled off after the animation so the NEXT click re-triggers it.
  const [tapped, setTapped] = useState(false);
  const tapTimer = useRef<number | null>(null);
  useEffect(
    () => () => {
      if (tapTimer.current) window.clearTimeout(tapTimer.current);
    },
    [],
  );
  const cls = [
    "fp-fab",
    anchor ? "fp-fab-anchor" : "",
    active && tone === "live" ? "fp-fab-live" : "",
    active && tone !== "live" ? "fp-fab-on" : "",
    tapped ? "is-tapped" : "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <button
      className={cls}
      title={label}
      aria-label={label}
      aria-pressed={active}
      disabled={disabled}
      onClick={() => {
        setTapped(false);
        // next frame → re-add so the animation restarts on every click
        requestAnimationFrame(() => setTapped(true));
        if (tapTimer.current) window.clearTimeout(tapTimer.current);
        tapTimer.current = window.setTimeout(() => setTapped(false), 320);
        onClick();
      }}
    >
      {children}
    </button>
  );
}

// A capture/frame glyph for the screenshot control (corner brackets).
function ScreenshotGlyph() {
  return (
    <svg
      width={17}
      height={17}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M4 8V6a2 2 0 0 1 2-2h2" />
      <path d="M16 4h2a2 2 0 0 1 2 2v2" />
      <path d="M20 16v2a2 2 0 0 1-2 2h-2" />
      <path d="M8 20H6a2 2 0 0 1-2-2v-2" />
      <circle cx="12" cy="12" r="3" />
    </svg>
  );
}

// Per-state visual for the system-audio button (mirrors the glass composer's
// micMeta, so connecting/failed/denied are never silently swallowed).
function systemAudioMeta(state: ListeningState): {
  glyph: React.ReactNode;
  label: string;
  tone?: "live";
} {
  switch (state) {
    case "listening":
      return { glyph: <StopIcon size={16} />, label: "Stop listening", tone: "live" };
    case "connecting":
      return { glyph: <Spin />, label: "Connecting…" };
    case "failed":
      return { glyph: <AlertIcon size={16} />, label: "Audio failed — retry" };
    case "permission_denied":
      return { glyph: <AlertIcon size={16} />, label: "Grant Screen Recording" };
    default:
      return { glyph: <SystemAudioIcon size={17} />, label: "Listen to the call" };
  }
}

function Spin() {
  return (
    <span className="fp-spin">
      <SpinnerIcon size={16} />
    </span>
  );
}

