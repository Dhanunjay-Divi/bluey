// The MEETINGS lens ("my past meetings") — distinct from the AGENT-SESSION lens
// (AgentsScreen). Lists meetings Bluey recorded, newest-first; clicking one opens
// a READ-ONLY viewer of its transcript + Q&A.
//
// SAFETY: the viewer NEVER subscribes to the live stream and NEVER mutates the
// singleton MeetingProvider (which owns the LIVE meeting). It seeds a LOCAL
// grouper from the openMeeting snapshot, so opening a past meeting cannot clobber
// a live one on the frontend side — complementing the daemon's pure-read guard.

import { useEffect, useMemo, useState } from "react";
import { getClient } from "../lib";
import { useDataStore } from "../lib/dataStore";
import { createTranscriptGrouper } from "../lib/transcriptGrouping";
import type {
  MeetingSummary,
  MeetingViewState,
  TranscriptLine,
} from "../lib/types";

// A read-only Q&A turn, mirroring the shape MeetingProvider seeds (meetingState
// .tsx) — question + finished answer text. No live status/streaming state.
interface ViewTurn {
  id: number;
  question: string;
  answer: string;
}

export function MeetingsScreen({
  onResumeAgentThread,
  onContinue,
}: {
  /** Resume the agent thread this meeting chained (Decision 3). `kind` is the
   *  agent the meeting ACTUALLY used (from the link) — `undefined` for legacy
   *  links recorded before the kind was stored, where App falls back to the
   *  attached agent. The App wires this to attach(kind, sessionId) → Ask tab. */
  onResumeAgentThread: (kind: string | undefined, sessionId: string) => void;
  /** Continue this past meeting in the Ask screen: make it the ACTIVE meeting.
   *  App wires this to client.continueMeeting(id) → (unless blocked) Ask tab. */
  onContinue: (id: string) => void;
}) {
  // Past meetings come from the shared SWR store — cached across tab switches and
  // revalidated in the background (the History>Meetings lens triggers the
  // revalidate on focus), so opening this screen shows data instantly with no
  // throw-away refetch. null → first-load spinner; [] → empty.
  const { meetings } = useDataStore();
  const [selected, setSelected] = useState<MeetingSummary | null>(null);

  if (selected) {
    return (
      <MeetingViewer
        summary={selected}
        onBack={() => setSelected(null)}
        onResumeAgentThread={onResumeAgentThread}
        onContinue={onContinue}
      />
    );
  }

  if (meetings === null) return <Loading text="Loading meetings…" />;
  if (meetings.length === 0) {
    return (
      <Empty text="No past meetings yet. Start listening to record one." />
    );
  }

  return (
    <div
      style={{
        padding: "6px 12px 12px",
        // Fill the parent + scroll internally (was maxHeight:480 → dead space
        // below on taller windows).
        flex: 1,
        minHeight: 0,
        overflowY: "auto",
      }}
    >
      {meetings.map((m) => (
        <button key={m.id} onClick={() => setSelected(m)} style={meetingRow}>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={rowTitle}>
              <span style={titleText}>{m.title || "Untitled meeting"}</span>
              {m.isActive && <span style={activePill}>active</span>}
              {m.agentSessionId != null && (
                <span title="Has a linked agent thread" style={threadGlyph}>
                  ↩ thread
                </span>
              )}
            </div>
            <div style={rowMeta}>
              {relativeTime(m.startedAt)} · {m.transcriptCount} lines ·{" "}
              {m.turnCount} Q&amp;A
            </div>
            {m.preview && <div style={rowPreview}>{m.preview}</div>}
          </div>
          <span style={openBtn}>Open →</span>
        </button>
      ))}
    </div>
  );
}

function MeetingViewer({
  summary,
  onBack,
  onResumeAgentThread,
  onContinue,
}: {
  summary: MeetingSummary;
  onBack: () => void;
  onResumeAgentThread: (kind: string | undefined, sessionId: string) => void;
  onContinue: (id: string) => void;
}) {
  const [view, setView] = useState<MeetingViewState | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let live = true;
    setView(null);
    setFailed(false);
    getClient()
      .openMeeting(summary.id)
      .then((v) => live && setView(v))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [summary.id]);

  // Fold the snapshot transcript into flowing lines through the SAME pure grouper
  // the live provider uses (paused=false, matching the seed loop), so the past
  // meeting renders identically to a rehydrated live one. Local to this viewer —
  // the live MeetingProvider is never touched.
  const history: TranscriptLine[] = useMemo(() => {
    if (!view) return [];
    const grouper = createTranscriptGrouper();
    let out: TranscriptLine[] = [];
    for (const l of view.transcript) {
      const { history: h } = grouper.foldSegment(
        {
          id: l.id,
          source: l.source,
          speaker: l.speaker,
          text: l.text,
          final: true,
        },
        false,
      );
      out = h;
    }
    return out;
  }, [view]);

  const turns: ViewTurn[] = useMemo(
    () =>
      view
        ? view.conversation.map((t, i) => ({
            id: i + 1,
            question: t.question,
            answer: t.answer,
          }))
        : [],
    [view],
  );

  const banner = view?.readOnly
    ? summary.isActive
      ? "Viewing a past meeting — read only · your live meeting is still recording"
      : "Viewing a past meeting — read only"
    : "Past meeting";

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
      }}
    >
      <div style={viewerHeader}>
        <button onClick={onBack} style={backBtn}>
          ← Back
        </button>
        <span style={viewerTitle}>{summary.title || "Untitled meeting"}</span>
        <button onClick={() => onContinue(summary.id)} style={continueBtn}>
          Continue in Ask →
        </button>
        {summary.agentSessionId != null && (
          <button
            onClick={() =>
              onResumeAgentThread(
                summary.agentKind,
                summary.agentSessionId as string,
              )
            }
            style={resumeBtn}
          >
            Resume agent thread →
          </button>
        )}
      </div>

      <div style={bannerStyle}>{banner}</div>

      <div
        style={{ flex: 1, minHeight: 0, overflowY: "auto", padding: "6px 0" }}
      >
        {failed && <Empty text="Couldn't load this meeting." />}
        {!failed && view === null && <Loading text="Opening meeting…" />}

        {view !== null && !failed && (
          <>
            {/* Q&A feed — every asked question + its answer, oldest first. */}
            {turns.map((turn) => (
              <div key={turn.id}>
                <div style={trig}>
                  <div style={ln} />
                  <span style={trigLbl}>✦ {turn.question}</span>
                  <div style={ln} />
                </div>
                {turn.answer && <div style={answerText}>{turn.answer}</div>}
              </div>
            ))}

            {turns.length === 0 && history.length === 0 && (
              <Empty text="This meeting has no transcript or Q&A." />
            )}

            {/* The transcript, grouped identically to the live view. */}
            {history.length > 0 && (
              <div style={transcriptPanel}>
                {history.map((line, i) => (
                  <div key={i} style={captionWrap}>
                    <span style={captionDot} />
                    <span style={captionWho}>
                      {line.source === "mic" ? "You" : "They"}
                    </span>
                    <span style={captionText}>
                      {line.text.replace(/^\s+/, "")}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}

function relativeTime(v: string): string {
  // MeetingSummary.startedAt is an epoch-MS string (see the contract). Fall back
  // gracefully for anything non-numeric.
  const n = Number(v);
  if (!Number.isFinite(n) || n <= 0) return "recently";
  const secs = Math.max(0, Math.floor((Date.now() - n) / 1000));
  if (secs < 3600) return `${Math.max(1, Math.floor(secs / 60))}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

function Loading({ text }: { text: string }) {
  return (
    <div style={loadingWrap}>
      <span style={spinner} />
      {text}
    </div>
  );
}
function Empty({ text }: { text: string }) {
  return <div style={emptyWrap}>{text}</div>;
}

const meetingRow = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  width: "100%",
  textAlign: "left",
  border: "1px solid var(--line)",
  background: "var(--glass-2)",
  borderRadius: "var(--r)",
  padding: "10px 12px",
  marginBottom: 6,
  cursor: "pointer",
} as const;
const rowTitle = {
  display: "flex",
  alignItems: "center",
  gap: 7,
  minWidth: 0,
} as const;
const titleText = {
  fontSize: 13,
  fontWeight: 500,
  color: "var(--ink)",
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
} as const;
const rowMeta = { fontSize: 11, color: "var(--ink-3)", marginTop: 1 } as const;
const rowPreview = {
  fontSize: 11.5,
  color: "var(--ink-3)",
  marginTop: 3,
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
} as const;
const activePill = {
  fontSize: 10,
  color: "var(--ok)",
  background: "#e7f8f1",
  padding: "2px 7px",
  borderRadius: "var(--r-pill)",
  flex: "none",
} as const;
const threadGlyph = {
  fontSize: 10,
  color: "var(--tint-ink)",
  background: "var(--tint-wash)",
  padding: "2px 7px",
  borderRadius: "var(--r-pill)",
  flex: "none",
} as const;
const openBtn = {
  fontSize: 11.5,
  color: "var(--tint-ink)",
  fontWeight: 540,
  flex: "none",
} as const;

const viewerHeader = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  padding: "6px 14px",
} as const;
const backBtn = {
  fontSize: 12,
  fontWeight: 540,
  color: "var(--ink-2)",
  background: "var(--glass-solid)",
  border: "1px solid var(--line-2)",
  borderRadius: 9,
  padding: "5px 11px",
  cursor: "pointer",
  flex: "none",
} as const;
const viewerTitle = {
  flex: 1,
  minWidth: 0,
  fontSize: 13,
  fontWeight: 560,
  color: "var(--ink)",
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
} as const;
const resumeBtn = {
  fontSize: 11.5,
  fontWeight: 600,
  color: "#fff",
  background: "var(--tint)",
  border: "none",
  borderRadius: "var(--r-pill)",
  padding: "6px 12px",
  cursor: "pointer",
  flex: "none",
} as const;
// Secondary (outlined) variant next to the solid Resume button, so the two
// actions are visually distinct: Continue reactivates the meeting, Resume
// reattaches its agent thread.
const continueBtn = {
  fontSize: 11.5,
  fontWeight: 600,
  color: "var(--tint-ink)",
  background: "var(--tint-wash)",
  border: "1px solid var(--tint)",
  borderRadius: "var(--r-pill)",
  padding: "6px 12px",
  cursor: "pointer",
  flex: "none",
} as const;
const bannerStyle = {
  fontSize: 11,
  color: "var(--ink-3)",
  padding: "4px 16px 8px",
  borderBottom: "1px solid var(--line)",
} as const;
const answerText = {
  fontSize: 13,
  lineHeight: 1.5,
  color: "var(--ink-2)",
  padding: "2px 16px 10px",
  whiteSpace: "pre-wrap",
  overflowWrap: "anywhere",
} as const;
const transcriptPanel = {
  borderTop: "1px solid var(--line)",
  marginTop: 6,
} as const;

const loadingWrap = {
  padding: "40px 16px",
  textAlign: "center",
  color: "var(--ink-3)",
  fontSize: 13,
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  gap: 12,
} as const;
const spinner = {
  width: 18,
  height: 18,
  borderRadius: "50%",
  background:
    "conic-gradient(var(--violet),var(--blue),var(--mint),var(--violet))",
  animation: "aurora-spin 1.4s linear infinite",
} as const;
const emptyWrap = {
  padding: "40px 16px",
  textAlign: "center",
  color: "var(--ink-3)",
  fontSize: 13,
} as const;

// ---- transcript line styling (matches AskScreen's ambient caption) ----
const captionWrap = {
  display: "flex",
  alignItems: "flex-start",
  gap: 8,
  padding: "5px 16px",
  minWidth: 0,
} as const;
const captionDot = {
  width: 6,
  height: 6,
  borderRadius: 999,
  background: "var(--mint)",
  flex: "none",
} as const;
const captionWho = {
  fontSize: 10.5,
  fontWeight: 600,
  color: "var(--ink-4)",
  letterSpacing: ".04em",
  flex: "none",
} as const;
const captionText = {
  fontSize: 12,
  color: "var(--ink-3)",
  whiteSpace: "pre-wrap",
  overflowWrap: "anywhere",
  wordBreak: "break-word",
  lineHeight: 1.4,
  flex: 1,
  minWidth: 0,
} as const;
const trig = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  padding: "5px 16px",
} as const;
const ln = { flex: 1, height: 1, background: "var(--line-2)" } as const;
const trigLbl = {
  fontSize: 10.5,
  color: "var(--tint-ink)",
  fontWeight: 540,
  background: "var(--tint-wash)",
  padding: "3px 10px",
  borderRadius: "var(--r-pill)",
} as const;
