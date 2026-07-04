// The composer — the primary place to ask. Context pill above, + menu, mic,
// ⌘↵ hint, send. Submits on Enter (⌘↵ or plain Enter) when there's text.

import { useState, type ReactNode } from "react";
import type { AskMode, ListeningState } from "../lib/types";
import { PlusMenu } from "./PlusMenu";
import { ModelPicker } from "./ModelPicker";
import { AlertIcon, MicIcon, SpinnerIcon, StopIcon } from "./icons";

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
  onSubmit,
  onMic,
  listenState = "idle",
  mode,
  onModeChange,
  models,
  selectedModel,
  onModelChange,
}: {
  placeholder: string;
  contextLabel?: string;
  onSubmit: (text: string) => void;
  onMic?: () => void;
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
        glyph: <MicIcon size={16} />,
        label: "Listen",
        title: "Listen",
        bg: "transparent",
        fg: "var(--ink-2)",
      };
  }
}
