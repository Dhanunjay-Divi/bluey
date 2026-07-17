// A review-gated fix proposal (Fix-button slice F3). The daemon drove the
// attached agent in PROPOSE-ONLY mode and pushed a diagnosis + reasoning + an
// optional unified diff. This card PREVIEWS that proposal for review — it never
// mutates anything on its own.
//
// BETA GUARD: no agent may edit a tester's repo in beta, so the Apply button is
// rendered but DISABLED and labeled — testers see the full diagnosis + diff but
// cannot trigger an apply. Only Dismiss is live (it clears the proposal). The
// visual language mirrors AnswerCard: glass surface, CSS-var colors, the same
// eyebrow/label rhythm and quiet actions.

import type { FixProposal } from "../lib/types";

export function FixProposalCard({
  proposal,
  onDismiss,
}: {
  proposal: FixProposal;
  onDismiss?: () => void;
}) {
  return (
    <div
      role="group"
      aria-label="Proposed fix"
      style={{
        margin: "4px 12px",
        padding: "15px 16px 13px",
        borderRadius: "var(--r-lg)",
        background: "var(--glass-2)",
        boxShadow:
          "0 10px 36px -16px rgba(60,50,140,.3), inset 0 0 0 1px rgba(255,255,255,.5)",
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
        PROPOSED FIX
        <span
          style={{ fontWeight: 430, letterSpacing: 0, color: "var(--ink-4)" }}
        >
          preview only
        </span>
      </div>

      {/* Diagnosis — what the agent thinks is wrong. The headline of the card. */}
      <Section label="Diagnosis" text={proposal.diagnosis} />

      {/* Reasoning — why this change. Quieter than the diagnosis. */}
      {proposal.reasoning.trim() !== "" && (
        <Section label="Why" text={proposal.reasoning} />
      )}

      {/* The diff — monospace, scrollable, never wrapping mid-line so a unified
          diff stays readable. Absent for commands-only / prose-only fixes. */}
      {proposal.diff && proposal.diff.trim() !== "" && (
        <div style={{ marginTop: 11 }}>
          <div style={sectionLabel}>Diff</div>
          <pre
            aria-label="Proposed diff"
            tabIndex={0}
            style={{
              margin: "5px 0 0",
              maxHeight: 220,
              overflow: "auto",
              padding: "10px 12px",
              borderRadius: "var(--r-md, 10px)",
              background: "var(--glass-solid)",
              border: "1px solid var(--line)",
              fontSize: 11.5,
              lineHeight: 1.5,
              fontFamily:
                "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
              color: "var(--ink-2)",
              whiteSpace: "pre",
            }}
          >
            {proposal.diff}
          </pre>
        </div>
      )}

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 7,
          marginTop: 12,
        }}
      >
        {/* BETA: Apply is visible but INERT — no agent may edit a tester's repo
            in beta. Disabled + aria-disabled + an explanatory title so the block
            is honest and accessible, never a dead-looking control. */}
        <button
          type="button"
          disabled
          aria-disabled="true"
          title="Apply is disabled in beta — Bluey won't edit your repo yet"
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 6,
            fontSize: 12,
            cursor: "not-allowed",
            color: "var(--ink-4)",
            background: "var(--glass-solid)",
            border: "1px solid var(--line)",
            borderRadius: 9,
            padding: "6px 11px",
            opacity: 0.6,
          }}
        >
          <span aria-hidden>✦</span>
          Apply disabled in beta
        </button>

        {onDismiss && (
          <button
            type="button"
            onClick={onDismiss}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 6,
              fontSize: 12,
              cursor: "pointer",
              color: "var(--ink-2)",
              background: "var(--glass-solid)",
              border: "1px solid var(--line)",
              borderRadius: 9,
              padding: "6px 11px",
              transition: ".15s",
            }}
          >
            <span aria-hidden>✕</span>
            Dismiss
          </button>
        )}
      </div>
    </div>
  );
}

// One labeled prose section (diagnosis / reasoning) — the eyebrow label above a
// readable body, matching AnswerCard's ink hierarchy.
function Section({ label, text }: { label: string; text: string }) {
  return (
    <div style={{ marginTop: label === "Diagnosis" ? 0 : 10 }}>
      <div style={sectionLabel}>{label}</div>
      <div
        style={{
          fontSize: 13.5,
          lineHeight: 1.55,
          color: "var(--ink)",
          marginTop: 3,
          whiteSpace: "pre-wrap",
          overflowWrap: "anywhere",
        }}
      >
        {text}
      </div>
    </div>
  );
}

const sectionLabel = {
  fontSize: 10,
  fontWeight: 680,
  letterSpacing: ".08em",
  textTransform: "uppercase",
  color: "var(--ink-4)",
} as const;
