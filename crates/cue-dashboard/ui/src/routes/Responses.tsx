import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import {
  ArrowRight,
  Check,
  Clipboard,
  FileCode2,
  LoaderCircle,
  MessageCircle,
  Radio,
  Search,
  Sparkles,
} from "lucide-react";
import { invoke } from "../lib/tauri";
import {
  applyChunk,
  clearInflight,
  responseBelongsToSession,
  shouldApplyResponseLoad,
  type CueResponseChunk,
  type InflightResponse,
  type ResponseLoadToken,
} from "./responseReducer";

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

interface SessionSwitchedPayload {
  id: string | null;
}

type ResponseFilter = "all" | "answer" | "suggestion" | "recap";

export function Responses() {
  const navigate = useNavigate();
  const [responses, setResponses] = useState<CueResponse[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [inflight, setInflight] = useState<Map<string, InflightResponse>>(new Map());
  const [filter, setFilter] = useState<ResponseFilter>("all");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [copyStatus, setCopyStatus] = useState("");
  const sessionIdRef = useRef<string | null>(null);
  const selectionGenerationRef = useRef(0);
  const loadGenerationRef = useRef(0);
  const loadAbortRef = useRef<AbortController | null>(null);

  const loadSessionResponses = useCallback(async (nextSessionId: string | null) => {
    loadAbortRef.current?.abort();
    const controller = new AbortController();
    loadAbortRef.current = controller;
    const generation = loadGenerationRef.current + 1;
    loadGenerationRef.current = generation;
    sessionIdRef.current = nextSessionId;
    setSessionId(nextSessionId);
    setResponses([]);
    setInflight(new Map());
    setError("");
    if (!nextSessionId) {
      setLoading(false);
      return;
    }
    const token: ResponseLoadToken = { generation, sessionId: nextSessionId };
    setLoading(true);
    try {
      const loaded = await invoke<CueResponse[]>("list_responses", {
        sessionId: nextSessionId,
        limit: 100,
      });
      if (
        controller.signal.aborted ||
        !shouldApplyResponseLoad(loadGenerationRef.current, sessionIdRef.current, token)
      ) {
        return;
      }
      const accepted = loaded.filter((response) =>
        responseBelongsToSession(sessionIdRef.current, response.source_session_id),
      );
      setResponses((current) => {
        const merged = new Map(accepted.map((response) => [response.id, response]));
        for (const response of current) {
          if (responseBelongsToSession(sessionIdRef.current, response.source_session_id)) {
            merged.set(response.id, response);
          }
        }
        return Array.from(merged.values()).sort((a, b) => b.ts_ms - a.ts_ms);
      });
    } catch (nextError) {
      if (
        controller.signal.aborted ||
        !shouldApplyResponseLoad(loadGenerationRef.current, sessionIdRef.current, token)
      ) {
        return;
      }
      setError(String(nextError));
    } finally {
      if (
        !controller.signal.aborted &&
        shouldApplyResponseLoad(loadGenerationRef.current, sessionIdRef.current, token)
      ) {
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    let removeListener: (() => void) | null = null;
    void (async () => {
      removeListener = await listen<SessionSwitchedPayload>("session:switched", (event) => {
        selectionGenerationRef.current += 1;
        void loadSessionResponses(event.payload.id);
      });
      if (disposed) {
        removeListener();
        return;
      }
      const selectionGeneration = selectionGenerationRef.current;
      try {
        const activeSessionId = await invoke<string | null>("get_active_session");
        if (!disposed && selectionGenerationRef.current === selectionGeneration) {
          await loadSessionResponses(activeSessionId);
        }
      } catch (nextError) {
        if (!disposed && selectionGenerationRef.current === selectionGeneration) {
          setError(String(nextError));
          setLoading(false);
        }
      }
    })().catch((nextError) => {
      if (!disposed) {
        setError(String(nextError));
        setLoading(false);
      }
    });
    return () => {
      disposed = true;
      selectionGenerationRef.current += 1;
      loadGenerationRef.current += 1;
      loadAbortRef.current?.abort();
      removeListener?.();
    };
  }, [loadSessionResponses]);

  useEffect(() => {
    const unlisten = listen<CueResponse>("cue_response", (event) => {
      const response = event.payload;
      if (!responseBelongsToSession(sessionIdRef.current, response.source_session_id)) return;
      setInflight((current) => clearInflight(current, response.id));
      setResponses((current) => [
        response,
        ...current.filter((item) => item.id !== response.id),
      ]);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<CueResponseChunk>("cue_response_chunk", (event) => {
      if (
        !responseBelongsToSession(
          sessionIdRef.current,
          event.payload.source_session_id,
        )
      ) {
        return;
      }
      setInflight((current) => applyChunk(current, event.payload));
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  const visibleResponses = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return responses.filter((response) => {
      if (filter !== "all" && response.kind !== filter) return false;
      if (!normalized) return true;
      return (
        response.text.toLowerCase().includes(normalized) ||
        response.source_text?.toLowerCase().includes(normalized)
      );
    });
  }, [filter, query, responses]);

  async function copy(text: string) {
    setCopyStatus("");
    try {
      await navigator.clipboard.writeText(text);
      setCopyStatus("Answer copied.");
    } catch (nextError) {
      setError(String(nextError));
    }
  }

  if (!loading && !sessionId) {
    return (
      <div className="mx-auto flex min-h-[60vh] max-w-xl items-center justify-center text-center">
        <div>
          <MessageCircle aria-hidden="true" className="mx-auto text-zinc-600" size={34} />
          <h1 className="mt-4 text-xl font-semibold text-zinc-100">No active session</h1>
          <p className="mt-2 text-sm leading-6 text-zinc-500">
            Open or create a session, then use Live or the overlay to ask Bluey.
          </p>
          <button
            type="button"
            onClick={() => navigate("/")}
            className="mt-4 inline-flex min-h-10 items-center gap-2 rounded-md bg-cyan-400 px-4 text-sm font-semibold text-zinc-950 hover:bg-cyan-300"
          >
            Go home
            <ArrowRight aria-hidden="true" size={15} />
          </button>
        </div>
      </div>
    );
  }

  const inflightEntries = Array.from(inflight.entries());

  return (
    <div className="mx-auto max-w-5xl space-y-5">
      <header className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="text-xs font-semibold uppercase tracking-[0.16em] text-cyan-300">
            Current session
          </p>
          <h1 className="mt-1 text-3xl font-semibold tracking-tight text-zinc-50">Answers</h1>
          <p className="mt-2 max-w-2xl text-sm leading-6 text-zinc-400">
            Completed and streaming responses stay together without provider or routing internals.
          </p>
        </div>
        <button
          type="button"
          onClick={() => navigate("/live")}
          className="inline-flex min-h-10 items-center gap-2 rounded-md bg-cyan-400 px-4 text-sm font-semibold text-zinc-950 hover:bg-cyan-300"
        >
          <Radio aria-hidden="true" size={16} />
          Open live session
        </button>
      </header>

      {error ? (
        <p role="alert" className="rounded-lg border border-red-500/30 bg-red-950/30 px-4 py-3 text-sm text-red-100">
          Answers could not be updated: {error}
        </p>
      ) : null}
      {copyStatus ? (
        <p role="status" className="flex items-center gap-2 text-xs text-emerald-300">
          <Check aria-hidden="true" size={14} />
          {copyStatus}
        </p>
      ) : null}

      <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_auto]">
        <label className="relative">
          <span className="sr-only">Search answers</span>
          <Search
            aria-hidden="true"
            className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500"
            size={15}
          />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            className="min-h-11 w-full rounded-md border border-zinc-800 bg-zinc-900 pl-9 pr-3 text-sm text-zinc-100 outline-none placeholder:text-zinc-600 focus:border-cyan-400"
            placeholder="Search questions and answers..."
          />
        </label>
        <div className="flex gap-1 overflow-x-auto rounded-md border border-zinc-800 bg-zinc-900 p-1">
          {(["all", "answer", "suggestion", "recap"] as ResponseFilter[]).map((item) => (
            <button
              key={item}
              type="button"
              aria-pressed={filter === item}
              onClick={() => setFilter(item)}
              className={`min-h-8 whitespace-nowrap rounded px-3 text-xs font-semibold capitalize ${
                filter === item
                  ? "bg-cyan-400/10 text-cyan-200"
                  : "text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200"
              }`}
            >
              {item}
            </button>
          ))}
        </div>
      </div>

      {loading ? (
        <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-sm text-zinc-500">
          Loading saved answers...
        </div>
      ) : null}

      {inflightEntries.length > 0 ? (
        <section aria-labelledby="streaming-answers-title" aria-live="polite">
          <h2 id="streaming-answers-title" className="mb-2 flex items-center gap-2 text-sm font-semibold text-cyan-200">
            <LoaderCircle aria-hidden="true" className="animate-spin" size={15} />
            Answering now
          </h2>
          <div className="space-y-3">
            {inflightEntries.map(([id, response]) => (
              <InflightCard key={id} response={response} />
            ))}
          </div>
        </section>
      ) : null}

      {!loading && visibleResponses.length > 0 ? (
        <ol className="space-y-3">
          {visibleResponses.map((response) => (
            <li key={response.id}>
              <ResponseCard response={response} onCopy={copy} />
            </li>
          ))}
        </ol>
      ) : null}

      {!loading && responses.length === 0 && inflightEntries.length === 0 ? (
        <div className="rounded-xl border border-dashed border-zinc-700 bg-zinc-900/50 px-5 py-12 text-center">
          <Sparkles aria-hidden="true" className="mx-auto text-zinc-600" size={30} />
          <p className="mt-3 text-sm font-medium text-zinc-200">No answers yet</p>
          <p className="mt-1 text-xs text-zinc-500">
            Ask from Live or the overlay. Streaming output and completed answers will appear here.
          </p>
        </div>
      ) : null}

      {!loading && responses.length > 0 && visibleResponses.length === 0 ? (
        <p className="rounded-xl border border-zinc-800 bg-zinc-900 px-4 py-8 text-center text-sm text-zinc-500">
          No answers match this search and filter.
        </p>
      ) : null}
    </div>
  );
}

function ResponseCard({
  response,
  onCopy,
}: {
  response: CueResponse;
  onCopy: (text: string) => Promise<void>;
}) {
  return (
    <article className="overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900">
      <header className="flex flex-wrap items-center justify-between gap-2 border-b border-zinc-800 bg-zinc-950/50 px-4 py-3">
        <span className="flex items-center gap-2">
          <span className="rounded-full bg-cyan-400/10 px-2.5 py-1 text-[11px] font-semibold capitalize text-cyan-200">
            {response.kind}
          </span>
          <span className="text-xs text-zinc-600">{formatTime(response.ts_ms)}</span>
          {displayCost(response) ? (
            <span className="text-[11px] text-zinc-500">{displayCost(response)}</span>
          ) : null}
        </span>
        <button
          type="button"
          onClick={() => void onCopy(response.text)}
          className="inline-flex min-h-8 items-center gap-1.5 rounded-md px-2 text-xs font-semibold text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100"
        >
          <Clipboard aria-hidden="true" size={13} />
          Copy
        </button>
      </header>
      <div className="px-4 py-4">
        {response.source_text ? (
          <blockquote className="mb-3 border-l-2 border-zinc-700 pl-3 text-xs leading-5 text-zinc-500">
            {response.source_text}
          </blockquote>
        ) : null}
        <p className="whitespace-pre-wrap text-sm leading-6 text-zinc-200">{response.text}</p>
        <Artifact response={response} />
      </div>
    </article>
  );
}

function InflightCard({ response }: { response: InflightResponse }) {
  return (
    <article className="rounded-xl border border-cyan-500/35 bg-cyan-950/10 px-4 py-4">
      <div className="flex items-center gap-2 text-xs">
        <span className="font-semibold text-cyan-200">
          {response.refined ? "Refining answer" : "Streaming answer"}
        </span>
        <span className="capitalize text-zinc-500">{response.kind}</span>
        {displayCost(response) ? <span className="text-zinc-600">{displayCost(response)}</span> : null}
      </div>
      <p className="mt-3 whitespace-pre-wrap text-sm leading-6 text-zinc-200">
        {response.text || "Bluey is preparing the first useful words..."}
        {!response.done ? (
          <span aria-hidden="true" className="ml-1 inline-block h-4 w-1 animate-pulse bg-cyan-300" />
        ) : null}
      </p>
      <Artifact response={response} />
    </article>
  );
}

function Artifact({
  response,
}: {
  response: Pick<CueResponse, "artifact_type" | "artifact_body">;
}) {
  if (!response.artifact_type || !response.artifact_body) return null;
  return (
    <div className="mt-4 rounded-lg border border-violet-400/25 bg-violet-400/5 p-3">
      <div className="mb-2 flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.14em] text-violet-200">
        <FileCode2 aria-hidden="true" size={13} />
        {response.artifact_type.replace(/_/g, " ")} workbench
      </div>
      <pre className="max-h-80 overflow-auto whitespace-pre-wrap text-xs leading-5 text-zinc-300">
        {response.artifact_body}
      </pre>
    </div>
  );
}

function displayCost(
  response: Pick<CueResponse, "cost_label" | "cost_cents">,
): string | null {
  if (response.cost_label) return response.cost_label;
  if (response.cost_cents === null || response.cost_cents === undefined) return null;
  return `$${(response.cost_cents / 100).toFixed(2)}`;
}

function formatTime(value: number): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Unknown time";
  return date.toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}
