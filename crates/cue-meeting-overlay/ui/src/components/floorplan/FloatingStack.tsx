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
import type { AudioPermissionSource, ListeningState } from "../../lib/types";
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
  systemInputOn,
  micInputOn,
  permissionDeniedSource,
  permissionSettingsOpenedFor,
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
  systemInputOn: boolean;
  micInputOn: boolean;
  permissionDeniedSource: AudioPermissionSource | null;
  permissionSettingsOpenedFor: AudioPermissionSource[];
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

  // A single click always opens/closes the input. The old single-vs-double-click
  // timer made this shortcut feel unresponsive and a single click could appear
  // to do nothing when there was no recent detected question.
  const onAskClick = () => {
    if (askStreaming) return;
    setAsking((open) => !open);
  };

  const sys = systemAudioMeta(
    listenState,
    systemInputOn,
    permissionDeniedSource,
    permissionSettingsOpenedFor.includes("system"),
  );
  const microphonePermissionDenied =
    !micInputOn &&
    (permissionDeniedSource === "microphone" ||
      (listenState === "permission_denied" &&
        permissionDeniedSource !== "system"));

  return (
    <div className="fp-stack">
      <div className="fp-stack-row">
        <div className="fp-stack-col">
          <StackButton
            label={sys.label}
            active={systemInputOn && listenState === "listening"}
            tone={sys.tone}
            onClick={onToggleSystemAudio}
          >
            {sys.glyph}
          </StackButton>

          <StackButton
            label={
              microphonePermissionDenied
                ? permissionSettingsOpenedFor.includes("microphone")
                  ? "Retry microphone"
                  : "Grant Microphone"
                : micInputOn
                  ? "Mute your mic"
                  : "Capture your voice"
            }
            active={micInputOn}
            tone={micInputOn ? "accent" : undefined}
            onClick={onToggleMic}
          >
            {/* Distinct on/off glyph — a slashed mic when muted — plus a
                pop animation keyed to the state so the toggle feels alive. */}
            <span key={micInputOn ? "on" : "off"} className="fp-mic-glyph">
              {microphonePermissionDenied ? (
                <AlertIcon size={16} />
              ) : micInputOn ? (
                <MicIcon size={17} />
              ) : (
                <MicOffIcon size={17} />
              )}
            </span>
          </StackButton>

          <StackButton label="Take a screenshot" onClick={onScreenshot}>
            <ScreenshotGlyph />
          </StackButton>

          <StackButton label="Attach files" onClick={onAttach}>
            <AttachIcon size={17} />
          </StackButton>

          {/* Capture page (active-browser-tab scrape) — HIDDEN for now, revisit
              later. TODO(agent-reach): consider re-building this on top of
              Agent-Reach (github.com/Panniantong/Agent-Reach), a key-free CLI that
              gives agents unified read/search over ~16 web platforms (Twitter/X,
              Reddit, YouTube, GitHub, LinkedIn, RSS, Exa search, arbitrary URLs) —
              so the user can pull a SPECIFIC url/source, not just the frontmost
              tab. onCapturePage stays plumbed so re-enabling is a one-line flip. */}
          {false && (
            <StackButton label="Capture page" onClick={onCapturePage}>
              <GlobeIcon size={17} />
            </StackButton>
          )}

          <StackButton
            label={agentName ? `Ask ${agentName}` : "Ask Bluey"}
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
function systemAudioMeta(
  state: ListeningState,
  systemInputOn: boolean,
  permissionDeniedSource: AudioPermissionSource | null,
  permissionRetryReady: boolean,
): {
  glyph: React.ReactNode;
  label: string;
  tone?: "live";
} {
  if (
    permissionDeniedSource === "system" ||
    (state === "permission_denied" && permissionDeniedSource !== "microphone")
  ) {
    return {
      glyph: <AlertIcon size={16} />,
      label: permissionRetryReady
        ? "Retry system audio"
        : "Grant Screen Recording",
    };
  }
  if (state === "failed") {
    return { glyph: <AlertIcon size={16} />, label: "Audio failed — retry" };
  }
  if (!systemInputOn) {
    return {
      glyph: <SystemAudioIcon size={17} />,
      label: "Listen to the call",
    };
  }
  switch (state) {
    case "listening":
      return {
        glyph: <StopIcon size={16} />,
        label: "Stop listening",
        tone: "live",
      };
    case "connecting":
      return { glyph: <Spin />, label: "Connecting…" };
    default:
      return {
        glyph: <SystemAudioIcon size={17} />,
        label: "Listen to the call",
      };
  }
}

function Spin() {
  return (
    <span className="fp-spin">
      <SpinnerIcon size={16} />
    </span>
  );
}
