import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface CueResponse {
  id: string;
  kind: string;
  text: string;
  ts_ms: number;
  source_session_id: string;
  source_text: string | null;
}

export function Responses() {
  const [responses, setResponses] = useState<CueResponse[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);

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
      setResponses((prev) => [event.payload, ...prev]);
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

  const renderCard = (r: CueResponse) => (
    <div key={r.id} className="rounded border border-zinc-700 bg-zinc-800 p-3 space-y-1">
      <div className="flex items-center justify-between">
        <span className="text-xs text-zinc-500">{formatTime(r.ts_ms)}</span>
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

  if (!sessionId) {
    return (
      <div className="p-6 text-zinc-400">
        No active session. Start a meeting to see AI responses.
      </div>
    );
  }

  return (
    <div className="p-6 space-y-6 overflow-y-auto max-h-[calc(100vh-4rem)]">
      <h1 className="text-2xl font-bold">AI Responses</h1>

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

      {responses.length === 0 && (
        <p className="text-zinc-500">No AI responses yet for this session.</p>
      )}
    </div>
  );
}
