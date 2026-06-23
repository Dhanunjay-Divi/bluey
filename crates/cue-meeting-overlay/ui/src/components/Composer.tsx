// The composer — the primary place to ask. Context pill above, + menu, mic,
// ⌘↵ hint, send. Submits on Enter (⌘↵ or plain Enter) when there's text.

import { useState } from "react";
import type { ListeningState } from "../lib/types";
import { PlusMenu } from "./PlusMenu";

export function Composer({
  placeholder,
  contextLabel,
  onSubmit,
  onMic,
  listenState = "idle",
}: {
  placeholder: string;
  contextLabel?: string;
  onSubmit: (text: string) => void;
  onMic?: () => void;
  /** Daemon listening-pipeline state — drives the mic button's visual state so a
   *  connecting/failed start is never silently swallowed. */
  listenState?: ListeningState;
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
          {micMeta(listenState).glyph}
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
  glyph: string;
  label: string;
  title: string;
  bg: string;
  fg: string;
} {
  switch (state) {
    case "listening":
      return {
        glyph: "🎙",
        label: "Stop listening",
        title: "Listening — click to stop",
        bg: "rgba(229,72,77,.12)",
        fg: "#e5484d",
      };
    case "connecting":
      return {
        glyph: "◌",
        label: "Connecting",
        title: "Starting audio…",
        bg: "rgba(180,140,40,.12)",
        fg: "#b8860b",
      };
    case "failed":
      return {
        glyph: "⚠",
        label: "Audio failed — click to retry",
        title: "Audio couldn't start (setup needed) — click to retry",
        bg: "rgba(229,72,77,.12)",
        fg: "#e5484d",
      };
    case "paused":
    case "idle":
    default:
      return {
        glyph: "🎙",
        label: "Listen",
        title: "Listen",
        bg: "transparent",
        fg: "var(--ink-2)",
      };
  }
}
