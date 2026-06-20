// The answer is the hero. Headline-first, grounded source rows, quiet actions.
// Renders the streamed agent answer (with a blinking cursor while live) — the
// deference principle: chrome recedes, the grounded answer leads.

import type { AnswerSource } from "../lib/types";
import { SourceRow } from "./primitives";

export interface AnswerState {
  agentLabel: string;
  text: string;
  sources: AnswerSource[];
  tools: string[];
  done: boolean;
  cost?: string;
}

export function AnswerCard({
  answer,
  onCopy,
  onFix,
  onSendToChat,
}: {
  answer: AnswerState;
  onCopy?: () => void;
  onFix?: () => void;
  onSendToChat?: () => void;
}) {
  return (
    <div
      style={{
        margin: "4px 12px",
        padding: "15px 16px 13px",
        borderRadius: "var(--r-lg)",
        background: "var(--glass-2)",
        boxShadow: "0 10px 36px -16px rgba(60,50,140,.3), inset 0 0 0 1px rgba(255,255,255,.5)",
        animation: "aurora-fade-in .35s ease both",
      }}
    >
      <div
        style={{
          fontSize: 10,
          fontWeight: 680,
          letterSpacing: ".1em",
          color: "var(--tint-ink)",
          marginBottom: 6,
          display: "flex",
          alignItems: "center",
          gap: 8,
        }}
      >
        {answer.agentLabel.toUpperCase()}
        <span style={{ fontWeight: 430, letterSpacing: 0, color: "var(--ink-4)" }}>
          grounded in your repo &amp; tickets
        </span>
      </div>

      <div style={{ fontSize: 14, lineHeight: 1.62, color: "var(--ink)" }}>
        {answer.text}
        {!answer.done && (
          <span
            style={{
              display: "inline-block",
              width: 2,
              height: 15,
              background: "var(--tint)",
              verticalAlign: "-3px",
              marginLeft: 3,
              borderRadius: 1,
              animation: "aurora-blink 1.1s step-end infinite",
            }}
          />
        )}

        {answer.sources.length > 0 && (
          <div style={{ display: "flex", flexDirection: "column", gap: 6, marginTop: 11 }}>
            {answer.sources.map((s, i) => (
              <SourceRow key={i} kind={s.kind} label={s.label} note={s.note} />
            ))}
          </div>
        )}
      </div>

      {answer.done && (
        <div style={{ display: "flex", alignItems: "center", gap: 7, marginTop: 12 }}>
          <CardAction glyph="⧉" label="Copy" onClick={onCopy} />
          <CardAction glyph="✦" label="Fix this" accent onClick={onFix} />
          <CardAction glyph="⤴" label="Send to chat" onClick={onSendToChat} />
          <span style={{ flex: 1 }} />
          {answer.cost && <span style={{ fontSize: 10.5, color: "var(--ink-4)" }}>{answer.cost}</span>}
        </div>
      )}
    </div>
  );
}

function CardAction({
  glyph,
  label,
  accent = false,
  onClick,
}: {
  glyph: string;
  label: string;
  accent?: boolean;
  onClick?: () => void;
}) {
  return (
    <button
      onClick={onClick}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        fontSize: 12,
        cursor: "pointer",
        color: accent ? "var(--tint-ink)" : "var(--ink-2)",
        background: accent ? "var(--tint-wash)" : "var(--glass-solid)",
        border: accent ? "1px solid transparent" : "1px solid var(--line)",
        borderRadius: 9,
        padding: "6px 11px",
        transition: ".15s",
      }}
    >
      <span aria-hidden>{glyph}</span>
      {label}
    </button>
  );
}
