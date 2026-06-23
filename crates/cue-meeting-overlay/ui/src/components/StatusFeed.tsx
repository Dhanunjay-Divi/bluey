// The live status feed — the agent's REAL activity while it works an answer.
//
// This is the "thinking with tools" experience from Claude/ChatGPT, but every
// row is a genuine ACP event the driven agent emitted: its reasoning
// (AgentThoughtChunk) and its tool/connector calls (ToolCall/ToolCallUpdate)
// with live run state. Nothing here is fabricated — if the agent makes no tool
// calls, the feed shows only what actually happened.
//
// Tool rows show a state glyph (running spinner / done check / failed) so the
// user sees "Reading App.tsx" flip from running to done in real time. Reasoning
// rows show the model's thought text in a quiet, italic treatment.

import type { AnswerStatusStep, AnswerStatusState } from "../lib/types";

export function StatusFeed({ steps, done }: { steps: AnswerStatusStep[]; done: boolean }) {
  if (steps.length === 0) return null;
  return (
    <div
      style={{
        margin: "4px 12px 6px",
        padding: "9px 12px",
        borderRadius: "var(--r-lg)",
        background: "var(--glass-2)",
        boxShadow: "inset 0 0 0 1px rgba(255,255,255,.5)",
        display: "flex",
        flexDirection: "column",
        gap: 6,
        // Once the work is done the feed recedes (it's history, not the hero).
        opacity: done ? 0.62 : 1,
        transition: "opacity .3s",
      }}
    >
      {steps.map((step, i) =>
        step.kind === "reasoning" ? (
          <div
            key={`r${i}`}
            style={{
              fontSize: 11.5,
              fontStyle: "italic",
              color: "var(--ink-3)",
              lineHeight: 1.45,
              whiteSpace: "pre-wrap",
            }}
          >
            {step.text}
          </div>
        ) : (
          <div
            key={step.id}
            style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 12 }}
          >
            <ToolGlyph state={step.state} />
            <span
              style={{
                color: step.state === "failed" ? "var(--err, #c0392b)" : "var(--ink-2)",
                fontWeight: 500,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
              title={step.title}
            >
              {prettyToolTitle(step.title)}
            </span>
          </div>
        ),
      )}
    </div>
  );
}

// State glyph for a tool row: spinning ring while running, check when done.
function ToolGlyph({ state }: { state: AnswerStatusState }) {
  if (state === "done") {
    return (
      <span style={{ ...glyphBase, color: "var(--ok)" }} aria-label="done">
        ✓
      </span>
    );
  }
  if (state === "failed") {
    return (
      <span style={{ ...glyphBase, color: "var(--err, #c0392b)" }} aria-label="failed">
        ×
      </span>
    );
  }
  // pending / running — a small spinning aurora ring.
  return (
    <span
      aria-label={state}
      style={{
        width: 12,
        height: 12,
        flex: "none",
        borderRadius: "50%",
        background: "conic-gradient(var(--violet),var(--blue),var(--mint),var(--violet))",
        animation: "aurora-spin 1.2s linear infinite",
        // a hole in the middle to read as a ring, not a disc
        WebkitMask: "radial-gradient(circle 3.5px at center, transparent 98%, #000 100%)",
        mask: "radial-gradient(circle 3.5px at center, transparent 98%, #000 100%)",
      }}
    />
  );
}

const glyphBase = {
  width: 12,
  height: 12,
  flex: "none",
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  fontSize: 12,
  fontWeight: 700,
} as const;

// Tool titles arrive as raw ACP titles (often the MCP tool id, e.g.
// "mcp__perplexity__perplexity_ask" or "github-mcp-server-search_code"). Make
// them human-readable without losing meaning: drop the mcp__ prefix, take the
// connector + action, swap separators for spaces.
function prettyToolTitle(raw: string): string {
  const t = raw.trim();
  if (!t) return "Tool call";
  // mcp__<server>__<tool> → "<server>: <tool>"
  const mcp = t.match(/^mcp__([^_]+(?:_[^_]+)*?)__(.+)$/);
  if (mcp) {
    return `${humanize(mcp[1])}: ${humanize(mcp[2])}`;
  }
  return humanize(t);
}

function humanize(s: string): string {
  return s.replace(/[-_]+/g, " ").trim();
}
