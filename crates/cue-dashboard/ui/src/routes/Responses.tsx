import { useEffect, useState, useRef } from "react";
import { invoke } from "../lib/tauri";
import { listen } from "@tauri-apps/api/event";
import { applyChunk, clearInflight, type CueResponseChunk, type InflightResponse } from "./responseReducer";
import { LaneBadge } from "./LaneBadge";

interface CueResponse {
  id: string;
  kind: string;
  text: string;
  ts_ms: number;
  source_session_id: string;
  source_text: string | null;
  cost_cents?: number | null;
  balance_cents_after?: number | null;
  provider?: string | null;
  model?: string | null;
  input_tokens?: number | null;
  output_tokens?: number | null;
  cost_label?: string | null;
  artifact_type?: string | null;
  artifact_body?: string | null;
  artifact_confidence?: number | null;
}

export function Responses() {
  const [responses, setResponses] = useState<CueResponse[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [inflight, setInflight] = useState<Map<string, InflightResponse>>(new Map());
  const inflightRef = useRef(inflight);
  inflightRef.current = inflight;

  useEffect(() => {
    invoke<string | null>("get_active_session").then((id) => {
      if (id) {
        setSessionId(id);
        invoke<CueResponse[]>("list_responses", { sessionId: id, limit: 50 })
          .then(setResponses)
          .catch((e) => console.warn("list_responses failed:", e));
      }
    });
  }, []);

  useEffect(() => {
    const unlisten = listen<CueResponse>("cue_response", (event) => {
      const r = event.payload;
      // Remove from inflight when final arrives
      setInflight((prev) => clearInflight(prev, r.id));
      setResponses((prev) => [r, ...prev]);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  useEffect(() => {
    const unlisten = listen<CueResponseChunk>("cue_response_chunk", (event) => {
      const chunk = event.payload;
      setInflight((prev) => applyChunk(prev, chunk));
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const grouped = {
    answer: responses.filter((r) => r.kind === "answer"),
    suggestion: responses.filter((r) => r.kind === "suggestion"),
    recap: responses.filter((r) => r.kind === "recap"),
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text).catch(() => {});
  };

  const formatTime = (ts: number) => new Date(ts).toLocaleTimeString();

  const formatCents = (cents?: number | null) => {
    if (cents === null || cents === undefined) return null;
    return `$${(cents / 100).toFixed(2)}`;
  };

  const renderCostPill = (r: Pick<CueResponse, "cost_cents" | "balance_cents_after" | "provider" | "model" | "cost_label">) => {
    if (r.cost_label) {
      return (
        <span className="rounded-full bg-accent-subtle px-2 py-0.5 text-caption text-accent-subtle-text">
          {r.cost_label}
        </span>
      );
    }
    const cost = formatCents(r.cost_cents);
    if (!cost) return null;
    const balance = formatCents(r.balance_cents_after);
    return (
      <span className="rounded-full bg-accent-subtle px-2 py-0.5 text-caption text-accent-subtle-text">
        {cost}
        {r.model ? <span className="text-text-tertiary"> · {r.model}</span> : null}
        {balance ? <span className="text-text-tertiary"> · bal {balance}</span> : null}
      </span>
    );
  };

  const renderArtifact = (
    artifact: Pick<CueResponse, "artifact_type" | "artifact_body" | "artifact_confidence">,
  ) => {
    if (!artifact.artifact_type || !artifact.artifact_body) return null;
    const label = artifact.artifact_type.replace(/_/g, " ");
    const confidence =
      artifact.artifact_confidence === null || artifact.artifact_confidence === undefined
        ? null
        : `${Math.round(artifact.artifact_confidence * 100)}%`;
    return (
      <div className="mt-3 rounded-md border border-hairline bg-bg-input p-3">
        <div className="mb-2 flex items-center justify-between gap-2">
          <span className="text-caption font-semibold uppercase tracking-[0.16em] text-accent-subtle-text">
            {label} canvas
          </span>
          {confidence ? <span className="text-caption text-text-tertiary">{confidence}</span> : null}
        </div>
        <pre className="max-h-64 overflow-auto whitespace-pre-wrap text-footnote leading-relaxed text-text-secondary">
          {artifact.artifact_body}
        </pre>
      </div>
    );
  };

  const renderCard = (r: CueResponse) => (
    <div key={r.id} className="glass rounded-lg p-3 space-y-1">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="text-footnote text-text-tertiary">{formatTime(r.ts_ms)}</span>
          {renderCostPill(r)}
        </div>
        <button
          onClick={() => copyToClipboard(r.text)}
          className="text-footnote text-accent-subtle-text transition-colors duration-200 hover:text-text-primary"
        >
          Copy
        </button>
      </div>
      {r.source_text && (
        <p className="text-footnote text-text-tertiary italic truncate">{r.source_text}</p>
      )}
      <p className="text-callout text-text-primary whitespace-pre-wrap">{r.text}</p>
      {renderArtifact(r)}
    </div>
  );

  const renderInflightCard = (id: string, data: InflightResponse) => (
    <div key={`inflight-${id}`} className="glass-strong rounded-lg p-3 space-y-1 animate-pulse">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <span className="text-footnote text-accent-subtle-text">
            {data.refined ? "Refined" : "Streaming…"}
          </span>
          <span className="text-footnote text-text-tertiary">{data.kind}</span>
          {renderCostPill(data)}
        </div>
        {data.routerMeta ? (
          <LaneBadge meta={data.routerMeta} refined={data.refined} />
        ) : null}
      </div>
      <p className="text-callout text-text-primary whitespace-pre-wrap">
        {data.text || (data.done ? <span className="text-text-quaternary">&mdash;</span> : "")}
        {!data.done ? (
          <span className="inline-block w-1 h-4 bg-accent ml-0.5 animate-pulse" />
        ) : null}
      </p>
      {renderArtifact(data)}
    </div>
  );

  if (!sessionId) {
    return (
      <div className="p-6 text-callout text-text-tertiary">
        No active session. Start a meeting to see AI responses.
      </div>
    );
  }

  const inflightEntries = Array.from(inflight.entries());

  return (
    <div className="p-6 space-y-6 overflow-y-auto max-h-[calc(100vh-4rem)]">
      <h1 className="text-title-1 text-text-primary">AI Responses</h1>

      {inflightEntries.length > 0 && (
        <section>
          <h2 className="text-headline text-accent-subtle-text mb-2">In Progress</h2>
          <div className="space-y-2">
            {inflightEntries.map(([id, data]) => renderInflightCard(id, data))}
          </div>
        </section>
      )}

      {grouped.answer.length > 0 && (
        <section>
          <h2 className="text-headline text-text-primary mb-2">Answers</h2>
          <div className="space-y-2">{grouped.answer.map(renderCard)}</div>
        </section>
      )}

      {grouped.suggestion.length > 0 && (
        <section>
          <h2 className="text-headline text-success mb-2">Suggestions</h2>
          <div className="space-y-2">{grouped.suggestion.map(renderCard)}</div>
        </section>
      )}

      {grouped.recap.length > 0 && (
        <section>
          <h2 className="text-headline text-text-secondary mb-2">Recaps</h2>
          <div className="space-y-2">{grouped.recap.map(renderCard)}</div>
        </section>
      )}

      {responses.length === 0 && inflightEntries.length === 0 && (
        <p className="text-callout text-text-tertiary">No AI responses yet for this session.</p>
      )}
    </div>
  );
}
