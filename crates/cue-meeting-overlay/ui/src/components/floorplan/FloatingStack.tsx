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
  onAskDirect,
  onNote,
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
  /** Tap-to-Ask: fire an Ask directly (the agent decides whether to answer a
   *  detected question or summarize what was just discussed). Long-press opens
   *  the type-a-question bar instead. */
  onAskDirect: () => void;
  /** Add a free-text note to the meeting (becomes context + memory). */
  onNote: (text: string) => void;
}) {
  const [asking, setAsking] = useState(false);
  const [text, setText] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const [noting, setNoting] = useState(false);
  const [noteText, setNoteText] = useState("");
  const noteRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (asking) inputRef.current?.focus();
  }, [asking]);
  useEffect(() => {
    if (noting) noteRef.current?.focus();
  }, [noting]);

  const submitNote = () => {
    const n = noteText.trim();
    if (n) onNote(n);
    setNoteText("");
    setNoting(false);
  };

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

  // Tap = Ask instantly (the agent decides whether to answer a detected question
  // or summarize what was just discussed). Long-press = open the type-a-question
  // search bar. A press-and-hold timer distinguishes them WITHOUT delaying the
  // tap: the Ask fires on release only if the hold never crossed the long-press
  // threshold, so the common tap has zero lag.
  const LONG_PRESS_MS = 350;
  const holdTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const openedByHold = useRef(false);

  const onAskPointerDown = () => {
    if (askStreaming) return;
    openedByHold.current = false;
    holdTimer.current = setTimeout(() => {
      openedByHold.current = true;
      setAsking(true); // long-press → open the search bar
    }, LONG_PRESS_MS);
  };

  const onAskPointerUp = () => {
    if (askStreaming) return;
    if (holdTimer.current) {
      clearTimeout(holdTimer.current);
      holdTimer.current = null;
    }
    // If the search bar is already open, a tap just closes it. Otherwise a tap
    // (no long-press) fires the direct Ask; a long-press already opened the bar.
    if (asking) {
      setAsking(false);
    } else if (!openedByHold.current) {
      onAskDirect();
    }
  };

  const onAskPointerLeave = () => {
    if (holdTimer.current) {
      clearTimeout(holdTimer.current);
      holdTimer.current = null;
    }
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

          <StackButton
            label="Add a note"
            onClick={() => setNoting((v) => !v)}
          >
            {/* Lines-on-paper note glyph (no dedicated icon in the set). */}
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" aria-hidden>
              <path
                d="M5 3.5h11.5L20 7v13.5H5z"
                stroke="currentColor"
                strokeWidth="1.6"
                strokeLinejoin="round"
              />
              <path
                d="M8.5 10.5h7M8.5 14h7M8.5 17h4"
                stroke="currentColor"
                strokeWidth="1.6"
                strokeLinecap="round"
              />
            </svg>
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
            label={
              agentName
                ? `Ask ${agentName} — tap to ask, hold to type`
                : "Ask Bluey — tap to ask, hold to type"
            }
            anchor
            active={asking}
            disabled={askStreaming}
            onPointerDown={onAskPointerDown}
            onPointerUp={onAskPointerUp}
            onPointerLeave={onAskPointerLeave}
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

        {noting && (
          <form
            className="fp-ask-form fp-note-form"
            onSubmit={(e) => {
              e.preventDefault();
              submitNote();
            }}
          >
            <input
              ref={noteRef}
              className="fp-ask-input"
              value={noteText}
              onChange={(e) => setNoteText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") setNoting(false);
              }}
              onBlur={() => {
                if (!noteText.trim()) setNoting(false);
              }}
              placeholder="Add a note…"
            />
            <button
              type="submit"
              className="fp-ask-send"
              aria-label="Add note"
              disabled={!noteText.trim()}
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
  onPointerDown,
  onPointerUp,
  onPointerLeave,
}: {
  children: React.ReactNode;
  label: string;
  active?: boolean;
  anchor?: boolean;
  tone?: "accent" | "live";
  disabled?: boolean;
  /** Simple click action. Mutually exclusive with the pointer handlers below
   *  (the Ask button uses tap/long-press via pointer events instead). */
  onClick?: () => void;
  onPointerDown?: () => void;
  onPointerUp?: () => void;
  onPointerLeave?: () => void;
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
      onClick={
        onClick
          ? () => {
              popAnimation();
              onClick();
            }
          : undefined
      }
      onPointerDown={
        onPointerDown
          ? () => {
              popAnimation();
              onPointerDown();
            }
          : undefined
      }
      onPointerUp={onPointerUp}
      onPointerLeave={onPointerLeave}
    >
      {children}
    </button>
  );

  function popAnimation() {
    setTapped(false);
    // next frame → re-add so the animation restarts on every interaction
    requestAnimationFrame(() => setTapped(true));
    if (tapTimer.current) window.clearTimeout(tapTimer.current);
    tapTimer.current = window.setTimeout(() => setTapped(false), 320);
  }
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
