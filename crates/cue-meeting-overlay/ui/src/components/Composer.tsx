// The composer — the primary place to ask. Context pill above, + menu, mic,
// ⌘↵ hint, send. Submits on Enter (⌘↵ or plain Enter) when there's text.

import { useState, type ReactNode } from "react";
import type { AskMode, ContextItem, ListeningState } from "../lib/types";
import { PlusMenu } from "./PlusMenu";
import { ModelPicker } from "./ModelPicker";
import {
  AlertIcon,
  MicIcon,
  SpinnerIcon,
  StopIcon,
  SystemAudioIcon,
} from "./icons";

/** The three answer-speed presets, ordered fast → deep. `hint` is the title
 *  tooltip; the label is what the pill shows. Data-driven so adding/removing a
 *  preset is a one-line edit. */
const MODES: ReadonlyArray<{ id: AskMode; label: string; hint: string }> = [
  { id: "fast", label: "Fast", hint: "Quickest reply, less depth" },
  {
    id: "balanced",
    label: "Balanced",
    hint: "Default — balance speed and depth",
  },
  { id: "deep", label: "Deep", hint: "Most thorough, slower" },
];

export function Composer({
  placeholder,
  contextLabel,
  contextItems,
  onRemoveContext,
  onSubmit,
  onMic,
  onToggleMicInput,
  micInputOn = false,
  listenState = "idle",
  mode,
  onModeChange,
  models,
  selectedModel,
  onModelChange,
  onAskRecent,
  askRecentDisabled = false,
}: {
  placeholder: string;
  contextLabel?: string;
  /** Attached context artifacts (the "+" menu). Rendered as ChatGPT-style chips
   *  above the input: image kinds show a thumbnail, others a glyph + title. */
  contextItems?: ContextItem[];
  /** Remove one attached artifact by id (the chip's ✕). */
  onRemoveContext?: (id: string) => void;
  onSubmit: (text: string) => void;
  onMic?: () => void;
  /** Toggle MICROPHONE capture (your own voice) — independent of the system-audio
   *  button above, which captures the call. Omit to hide the mic button. */
  onToggleMicInput?: () => void;
  /** Whether microphone capture is currently on (drives the mic button's tint). */
  micInputOn?: boolean;
  /** Manual "ask about what was just said" — fires the same ask pipeline as a
   *  typed question with a canonical prompt. Omit to hide the button. */
  onAskRecent?: () => void;
  /** Disables the ask-recent button while an ask is already streaming. */
  askRecentDisabled?: boolean;
  /** Daemon listening-pipeline state — drives the mic button's visual state so a
   *  connecting/failed start is never silently swallowed. */
  listenState?: ListeningState;
  /** Currently selected answer-speed preset (forwarded to the daemon's `mode`). */
  mode?: AskMode;
  /** Called when the user picks a different speed preset. */
  onModeChange?: (mode: AskMode) => void;
  /** Selectable models for the attached agent (element [0] is always "auto").
   *  The picker sits on the speed row and is shown only when there is more than
   *  one choice; omit / pass ≤1 entry to hide it (e.g. no agent attached, or an
   *  agent whose CLI exposes no model list). */
  models?: string[];
  /** Currently selected model id ("auto" = no override). */
  selectedModel?: string;
  /** Called when the user picks a different model. */
  onModelChange?: (model: string) => void;
}) {
  const [text, setText] = useState("");
  const [plusOpen, setPlusOpen] = useState(false);
  const submit = () => {
    const t = text.trim();
    if (!t) return;
    onSubmit(t);
    setText("");
  };
  return (
    <div style={{ padding: "11px 14px 13px", position: "relative" }}>
      {plusOpen && <PlusMenu onClose={() => setPlusOpen(false)} />}
      {contextLabel && (
        <span
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 7,
            fontSize: 10.5,
            color: "var(--ink-2)",
            background: "var(--glass-solid)",
            border: "1px solid var(--line)",
            borderRadius: "var(--r-pill)",
            padding: "4px 11px",
            marginBottom: 10,
          }}
        >
          ✓ {contextLabel}
        </span>
      )}
      {contextItems && contextItems.length > 0 && (
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: 7,
            marginBottom: 10,
          }}
        >
          {contextItems.map((item) => (
            <ContextChip
              key={item.id}
              item={item}
              onRemove={
                onRemoveContext ? () => onRemoveContext(item.id) : undefined
              }
            />
          ))}
        </div>
      )}
      {(onModeChange || (onModelChange && (models?.length ?? 0) > 1)) && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            flexWrap: "wrap",
            gap: 8,
            marginBottom: 10,
          }}
        >
          {onModeChange && (
            <div
              role="radiogroup"
              aria-label="Answer speed"
              style={{
                display: "inline-flex",
                gap: 2,
                padding: 2,
                background: "var(--glass-solid)",
                border: "1px solid var(--line-2)",
                borderRadius: "var(--r-pill)",
              }}
            >
              {MODES.map((m) => {
                const active = (mode ?? "balanced") === m.id;
                return (
                  <button
                    key={m.id}
                    role="radio"
                    aria-checked={active}
                    title={m.hint}
                    onClick={() => onModeChange(m.id)}
                    style={{
                      border: "none",
                      cursor: "pointer",
                      fontSize: 10.5,
                      fontWeight: active ? 580 : 460,
                      lineHeight: 1,
                      padding: "5px 11px",
                      borderRadius: "var(--r-pill)",
                      background: active ? "var(--tint-wash)" : "transparent",
                      color: active ? "var(--tint-ink)" : "var(--ink-3)",
                      transition: "color .12s ease, background .12s ease",
                    }}
                  >
                    {m.label}
                  </button>
                );
              })}
            </div>
          )}
          {onModelChange && models && models.length > 1 && (
            <ModelPicker
              models={models}
              value={selectedModel ?? "auto"}
              onChange={onModelChange}
            />
          )}
        </div>
      )}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 9,
          background: "var(--glass-solid)",
          border: "1px solid var(--line-2)",
          borderRadius: 15,
          padding: "8px 9px 8px 13px",
          boxShadow: "inset 0 1px 2px rgba(30,30,60,.04)",
        }}
      >
        <button
          onClick={() => setPlusOpen((v) => !v)}
          aria-label="Add context"
          aria-haspopup="menu"
          aria-expanded={plusOpen}
          style={{
            ...iconBtn,
            background: plusOpen ? "var(--tint-wash)" : "transparent",
            color: plusOpen ? "var(--tint-ink)" : "var(--ink-2)",
          }}
        >
          ＋
        </button>
        {onAskRecent && (
          <button
            onClick={onAskRecent}
            disabled={askRecentDisabled}
            aria-label="Ask about what was just said"
            title="Ask about what was just said"
            style={{
              ...iconBtn,
              fontSize: 14,
              color: askRecentDisabled ? "var(--ink-4)" : "var(--tint-ink)",
              cursor: askRecentDisabled ? "default" : "pointer",
              opacity: askRecentDisabled ? 0.55 : 1,
            }}
          >
            ✦
          </button>
        )}
        <input
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              submit();
            }
          }}
          placeholder={placeholder}
          style={{
            flex: 1,
            border: "none",
            outline: "none",
            background: "transparent",
            fontSize: 13.5,
            color: "var(--ink)",
            fontFamily: "var(--font)",
          }}
        />
        <button
          onClick={onMic}
          aria-label={micMeta(listenState).label}
          aria-pressed={listenState === "listening"}
          aria-busy={listenState === "connecting"}
          title={micMeta(listenState).title}
          style={{
            ...iconBtn,
            background: micMeta(listenState).bg,
            color: micMeta(listenState).fg,
          }}
        >
          <span
            style={{
              display: "inline-flex",
              animation:
                listenState === "connecting"
                  ? "aurora-spin 1.1s linear infinite"
                  : undefined,
            }}
          >
            {micMeta(listenState).glyph}
          </span>
        </button>
        {onToggleMicInput && (
          <button
            onClick={onToggleMicInput}
            aria-label={
              micInputOn ? "Stop microphone input" : "Start microphone input"
            }
            aria-pressed={micInputOn}
            title={
              micInputOn
                ? "Microphone on — your voice is captured (click to stop)"
                : "Microphone off — click to capture your voice too"
            }
            style={{
              ...iconBtn,
              background: micInputOn ? "rgba(99,102,241,.14)" : "transparent",
              color: micInputOn ? "var(--tint-ink)" : "var(--ink-2)",
            }}
          >
            <MicIcon size={16} />
          </button>
        )}
        <span
          style={{
            fontSize: 10,
            color: "var(--ink-3)",
            border: "1px solid var(--line-2)",
            borderRadius: 6,
            padding: "3px 6px",
            fontFamily: "var(--mono)",
          }}
        >
          ⌘↵
        </span>
        <button
          onClick={submit}
          aria-label="Send"
          style={{
            width: 30,
            height: 30,
            borderRadius: 9,
            border: "none",
            cursor: "pointer",
            background: "linear-gradient(140deg,var(--tint),#8f7af5)",
            color: "#fff",
            boxShadow: "0 4px 12px -2px rgba(111,106,240,.5)",
            fontSize: 15,
          }}
        >
          ↑
        </button>
      </div>
    </div>
  );
}

const iconBtn = {
  width: 31,
  height: 31,
  borderRadius: 9,
  border: "none",
  background: "transparent",
  color: "var(--ink-2)",
  cursor: "pointer",
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  fontSize: 16,
} as const;

/** One attached-context chip. Image/diagram kinds render the daemon-supplied
 *  `data:` thumbnail (ChatGPT-style); other kinds show a glyph + title. The ✕
 *  removes the artifact. */
function ContextChip({
  item,
  onRemove,
}: {
  item: ContextItem;
  onRemove?: () => void;
}) {
  const isImage =
    (item.kind === "image" || item.kind === "diagram") && !!item.thumbnail;
  return (
    <span
      title={item.title}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 7,
        maxWidth: 190,
        fontSize: 11,
        color: "var(--ink-2)",
        background: "var(--glass-solid)",
        border: "1px solid var(--line)",
        borderRadius: 10,
        padding: isImage ? "4px 7px 4px 4px" : "5px 7px 5px 9px",
      }}
    >
      {isImage ? (
        <img
          src={item.thumbnail}
          alt={item.title}
          style={{
            width: 26,
            height: 26,
            objectFit: "cover",
            borderRadius: 6,
            display: "block",
            flexShrink: 0,
          }}
        />
      ) : (
        <span style={{ fontSize: 13, flexShrink: 0 }}>
          {contextKindGlyph(item.kind)}
        </span>
      )}
      <span
        style={{
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {item.title}
      </span>
      {onRemove && (
        <button
          onClick={onRemove}
          aria-label={`Remove ${item.title}`}
          title="Remove"
          style={{
            border: "none",
            background: "transparent",
            color: "var(--ink-3)",
            cursor: "pointer",
            fontSize: 13,
            lineHeight: 1,
            padding: 0,
            marginLeft: 1,
            flexShrink: 0,
          }}
        >
          ✕
        </button>
      )}
    </span>
  );
}

/** Emoji glyph per context kind — the non-image chip's leading icon. */
function contextKindGlyph(kind: string): string {
  switch (kind) {
    case "image":
    case "diagram":
      return "🖼️";
    case "code":
      return "📄";
    case "document":
      return "📕";
    case "text":
      return "📝";
    default:
      return "📎";
  }
}

// Per-state visual for the mic button, so a connecting/failed start is visible
// (the bug before: any non-"listening" state silently snapped back to off).
function micMeta(state: ListeningState): {
  glyph: ReactNode;
  label: string;
  title: string;
  bg: string;
  fg: string;
} {
  switch (state) {
    case "listening":
      return {
        glyph: <StopIcon size={16} />,
        label: "Stop listening",
        title: "Listening — click to stop",
        bg: "rgba(229,72,77,.12)",
        fg: "#e5484d",
      };
    case "connecting":
      return {
        glyph: <SpinnerIcon size={16} />,
        label: "Connecting",
        title: "Starting audio…",
        bg: "rgba(180,140,40,.12)",
        fg: "#b8860b",
      };
    case "failed":
      return {
        glyph: <AlertIcon size={16} />,
        label: "Audio failed — click to retry",
        title: "Audio couldn't start (setup needed) — click to retry",
        bg: "rgba(229,72,77,.12)",
        fg: "#e5484d",
      };
    case "permission_denied":
      return {
        glyph: <AlertIcon size={16} />,
        label: "Grant Screen Recording — click to open Settings",
        title: "Screen Recording permission needed — click to open Settings",
        bg: "rgba(184,117,3,.14)",
        fg: "#b87503",
      };
    case "paused":
    case "idle":
    default:
      return {
        // SPEAKER, not a mic: this button toggles SYSTEM audio (the other people
        // in the call). The microphone is its own button beside it — showing a
        // mic here implied this captured YOUR voice, which it never did.
        glyph: <SystemAudioIcon size={16} />,
        label: "Listen to system audio",
        title: "Listen to system audio (the call)",
        bg: "transparent",
        fg: "var(--ink-2)",
      };
  }
}
