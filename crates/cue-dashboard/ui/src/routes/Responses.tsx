import { useEffect, useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
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

  const renderCostPill = (r: Pick<CueResponse, "cost_cents" | "balance_cents_after" | "provider" | "model">) => {
    const cost = formatCents(r.cost_cents);
    if (!cost) return null;
    const balance = formatCents(r.balance_cents_after);
    return (
      <span className="rounded-full border border-cyan-500/40 bg-cyan-500/10 px-2 py-0.5 text-[11px] font-medium text-cyan-200">
        {cost}
        {r.model ? <span className="text-cyan-300/70"> · {r.model}</span> : null}
        {balance ? <span className="text-cyan-300/70"> · bal {balance}</span> : null}
      </span>
    );
  };

  const renderCard = (r: CueResponse) => (
    <div key={r.id} className="rounded border border-zinc-700 bg-zinc-800 p-3 space-y-1">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="text-xs text-zinc-500">{formatTime(r.ts_ms)}</span>
          {renderCostPill(r)}
        </div>
        <button
          onClick={() => copyToClipboard(r.text)}
          className="text-xs text-blue-400 hover:text-blue-300"
        >
          Copy
        </button>
      </div>
      {r.source_text && (
        <p className="text-xs text-zinc-500 italic truncate">{r.source_text}</p>
      )}
      <p className="text-sm text-zinc-200 whitespace-pre-wrap">{r.text}</p>
    </div>
  );

  const renderInflightCard = (id: string, data: InflightResponse) => (
    <div key={`inflight-${id}`} className="rounded border border-blue-600 bg-zinc-800 p-3 space-y-1 animate-pulse">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <span className="text-xs text-blue-400 font-medium">
            {data.refined ? "Refined" : "Streaming…"}
          </span>
          <span className="text-xs text-zinc-500">{data.kind}</span>
          {renderCostPill(data)}
        </div>
        {data.routerMeta ? (
          <LaneBadge meta={data.routerMeta} refined={data.refined} />
        ) : null}
      </div>
      <p className="text-sm text-zinc-200 whitespace-pre-wrap">
        {data.text || "⏳"}
        {!data.done ? (
          <span className="inline-block w-1 h-4 bg-blue-400 ml-0.5 animate-pulse" />
        ) : null}
      </p>
    </div>
  );

  if (!sessionId) {
    return (
      <div className="p-6 text-zinc-400">
        No active session. Start a meeting to see AI responses.
      </div>
    );
  }

  const inflightEntries = Array.from(inflight.entries());

  return (
    <div className="p-6 space-y-6 overflow-y-auto max-h-[calc(100vh-4rem)]">
      <h1 className="text-2xl font-bold">AI Responses</h1>

      {inflightEntries.length > 0 && (
        <section>
          <h2 className="text-lg font-semibold text-blue-400 mb-2">In Progress</h2>
          <div className="space-y-2">
            {inflightEntries.map(([id, data]) => renderInflightCard(id, data))}
          </div>
        </section>
      )}

      {grouped.answer.length > 0 && (
        <section>
          <h2 className="text-lg font-semibold text-blue-400 mb-2">Answers</h2>
          <div className="space-y-2">{grouped.answer.map(renderCard)}</div>
        </section>
      )}

      {grouped.suggestion.length > 0 && (
        <section>
          <h2 className="text-lg font-semibold text-green-400 mb-2">Suggestions</h2>
          <div className="space-y-2">{grouped.suggestion.map(renderCard)}</div>
        </section>
      )}

      {grouped.recap.length > 0 && (
        <section>
          <h2 className="text-lg font-semibold text-purple-400 mb-2">Recaps</h2>
          <div className="space-y-2">{grouped.recap.map(renderCard)}</div>
        </section>
      )}

      {responses.length === 0 && inflightEntries.length === 0 && (
        <p className="text-zinc-500">No AI responses yet for this session.</p>
      )}
    </div>
  );
}
