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
}) {
  const [asking, setAsking] = useState(false);
  const [text, setText] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (asking) inputRef.current?.focus();
  }, [asking]);

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
            <MicIcon size={17} />
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
            label={agentName ? `Ask ${agentName}` : "Ask"}
            anchor
            active={asking}
            disabled={askStreaming}
            onClick={() => setAsking((v) => !v)}
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
  const cls = [
    "fp-fab",
    anchor ? "fp-fab-anchor" : "",
    active && tone === "live" ? "fp-fab-live" : "",
    active && tone !== "live" ? "fp-fab-on" : "",
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
      onClick={onClick}
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

