// The composer — the primary place to ask. Context pill above, + menu, mic,
// ⌘↵ hint, send. Submits on Enter (⌘↵ or plain Enter) when there's text.

import { useState } from "react";

export function Composer({
  placeholder,
  contextLabel,
  onSubmit,
  onPlus,
  onMic,
}: {
  placeholder: string;
  contextLabel?: string;
  onSubmit: (text: string) => void;
  onPlus?: () => void;
  onMic?: () => void;
}) {
  const [text, setText] = useState("");
  const submit = () => {
    const t = text.trim();
    if (!t) return;
    onSubmit(t);
    setText("");
  };
  return (
    <div style={{ padding: "11px 14px 13px" }}>
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
        <button onClick={onPlus} aria-label="Add context" style={iconBtn}>＋</button>
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
        <button onClick={onMic} aria-label="Listen" style={iconBtn}>🎙</button>
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
