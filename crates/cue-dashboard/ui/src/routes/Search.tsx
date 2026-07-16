import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { invoke } from "../lib/tauri";
import {
  ArrowRight,
  CircleAlert,
  Mic2,
  MonitorSpeaker,
  Search as SearchIcon,
} from "lucide-react";
import {
  shouldApplySearchResponse,
  type SearchRequestToken,
} from "./searchRequestState";

interface TranscriptHit {
  session_id: string;
  text: string;
  snippet: string;
  source: string;
  ts: number;
  rank: number;
}

/** Parse a snippet with «highlighted» markers into safe React elements. */
function renderSnippet(snippet: string) {
  const parts: { text: string; highlight: boolean }[] = [];
  let remaining = snippet;
  while (remaining.length > 0) {
    const start = remaining.indexOf("\u00AB");
    if (start === -1) {
      parts.push({ text: remaining, highlight: false });
      break;
    }
    if (start > 0) {
      parts.push({ text: remaining.slice(0, start), highlight: false });
    }
    const end = remaining.indexOf("\u00BB", start);
    if (end === -1) {
      parts.push({ text: remaining.slice(start), highlight: false });
      break;
    }
    parts.push({ text: remaining.slice(start + 1, end), highlight: true });
    remaining = remaining.slice(end + 1);
  }
  return parts.map((p, i) =>
    p.highlight ? (
      <mark key={i} className="bg-yellow-600/40 text-yellow-200 rounded px-0.5">
        {p.text}
      </mark>
    ) : (
      <span key={i}>{p.text}</span>
    )
  );
}

export function Search() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [query, setQuery] = useState(() => searchParams.get("q") ?? "");
  const [results, setResults] = useState<TranscriptHit[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const navigate = useNavigate();
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const requestGenerationRef = useRef(0);
  const requestAbortRef = useRef<AbortController | null>(null);
  const activeQueryRef = useRef(query.trim());

  const doSearch = useCallback(async (q: string) => {
    const normalized = q.trim();
    requestAbortRef.current?.abort();
    const controller = new AbortController();
    requestAbortRef.current = controller;
    const generation = requestGenerationRef.current + 1;
    requestGenerationRef.current = generation;
    activeQueryRef.current = normalized;
    const token: SearchRequestToken = { generation, query: normalized };

    if (!normalized) {
      setResults([]);
      setError("");
      setLoading(false);
      return;
    }
    setLoading(true);
    setError("");
    try {
      const hits = await invoke<TranscriptHit[]>("search_transcripts", {
        query: normalized,
        limit: 50,
      });
      if (
        !shouldApplySearchResponse(
          requestGenerationRef.current,
          activeQueryRef.current,
          token,
          controller.signal.aborted,
        )
      ) {
        return;
      }
      setResults(hits);
    } catch (nextError) {
      if (
        !shouldApplySearchResponse(
          requestGenerationRef.current,
          activeQueryRef.current,
          token,
          controller.signal.aborted,
        )
      ) {
        return;
      }
      setResults([]);
      setError(String(nextError));
    } finally {
      if (
        shouldApplySearchResponse(
          requestGenerationRef.current,
          activeQueryRef.current,
          token,
          controller.signal.aborted,
        )
      ) {
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    requestAbortRef.current?.abort();
    requestGenerationRef.current += 1;
    const trimmed = query.trim();
    activeQueryRef.current = trimmed;
    setResults([]);
    setError("");
    setLoading(Boolean(trimmed));
    timerRef.current = setTimeout(() => {
      setSearchParams(trimmed ? { q: trimmed } : {}, { replace: true });
      void doSearch(trimmed);
    }, 200);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [query, doSearch, setSearchParams]);

  useEffect(
    () => () => {
      requestGenerationRef.current += 1;
      requestAbortRef.current?.abort();
    },
    [],
  );

  return (
    <div className="mx-auto max-w-5xl space-y-5">
      <header>
        <p className="text-xs font-semibold uppercase tracking-[0.16em] text-cyan-300">
          Search memory
        </p>
        <h1 className="mt-1 text-3xl font-semibold tracking-tight text-zinc-50">
          Find it across sessions.
        </h1>
        <p className="mt-2 max-w-2xl text-sm leading-6 text-zinc-400">
          Search locally saved transcript text. Results stay labeled by microphone or system
          audio so you can tell where the evidence came from.
        </p>
      </header>
      <div className="relative">
        <SearchIcon
          aria-hidden="true"
          size={17}
          className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500"
        />
        <label htmlFor="memory-search" className="sr-only">
          Search saved transcripts
        </label>
        <input
          id="memory-search"
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Decision, technology, customer, action item..."
          className="min-h-12 w-full rounded-lg border border-zinc-700 bg-zinc-900 pl-10 pr-3 text-sm text-zinc-100 outline-none placeholder:text-zinc-600 focus:border-cyan-400"
        />
      </div>
      {loading && (
        <p role="status" className="text-xs text-zinc-500">
          Searching saved transcript memory...
        </p>
      )}
      {error ? (
        <div role="alert" className="flex items-start gap-2 rounded-lg border border-red-500/30 bg-red-950/30 px-4 py-3 text-sm text-red-100">
          <CircleAlert aria-hidden="true" className="mt-0.5 shrink-0" size={16} />
          Search could not be completed: {error}
        </div>
      ) : null}
      {results.length > 0 && (
        <section aria-labelledby="search-results-title">
          <div className="mb-2 flex items-center justify-between">
            <h2 id="search-results-title" className="text-sm font-semibold text-zinc-200">
              {results.length} {results.length === 1 ? "result" : "results"}
            </h2>
            <span className="text-xs text-zinc-600">Best matches first</span>
          </div>
          <ul className="space-y-2">
          {results.map((hit, i) => (
            <li key={`${hit.session_id}-${i}`}>
              <button
                type="button"
                className="group grid w-full gap-3 rounded-xl border border-zinc-800 bg-zinc-900 px-4 py-4 text-left hover:border-cyan-400/35 hover:bg-zinc-900/75 sm:grid-cols-[minmax(0,1fr)_auto]"
                onClick={() => navigate(`/session/${hit.session_id}`)}
              >
                <span className="min-w-0">
                  <span className="mb-2 flex flex-wrap items-center gap-2 text-xs text-zinc-500">
                    <span
                      className={`inline-flex items-center gap-1.5 rounded-full px-2 py-1 ${
                        isMicrophoneSource(hit.source)
                          ? "bg-cyan-400/10 text-cyan-200"
                          : "bg-violet-400/10 text-violet-200"
                      }`}
                    >
                      {isMicrophoneSource(hit.source) ? (
                        <Mic2 aria-hidden="true" size={12} />
                      ) : (
                        <MonitorSpeaker aria-hidden="true" size={12} />
                      )}
                      {sourceLabel(hit.source)}
                    </span>
                    <span>{formatTimestamp(hit.ts)}</span>
                  </span>
                  <span className="block text-sm leading-6 text-zinc-300">
                    {renderSnippet(hit.snippet)}
                  </span>
                </span>
                <ArrowRight
                  aria-hidden="true"
                  className="hidden self-center text-zinc-600 group-hover:text-cyan-300 sm:block"
                  size={16}
                />
              </button>
            </li>
          ))}
          </ul>
        </section>
      )}
      {!loading && !error && query.trim() && results.length === 0 && (
        <div className="rounded-xl border border-dashed border-zinc-700 bg-zinc-900/50 px-5 py-10 text-center">
          <SearchIcon aria-hidden="true" className="mx-auto text-zinc-600" size={28} />
          <p className="mt-3 text-sm font-medium text-zinc-200">No saved transcript matches</p>
          <p className="mt-1 text-xs text-zinc-500">
            Try a person, decision, technology, or a shorter phrase.
          </p>
        </div>
      )}
      {!query.trim() ? (
        <div className="grid gap-3 sm:grid-cols-3">
          {["architecture decision", "follow-up action", "customer feedback"].map((suggestion) => (
            <button
              key={suggestion}
              type="button"
              onClick={() => setQuery(suggestion)}
              className="rounded-lg border border-zinc-800 bg-zinc-900 px-4 py-3 text-left text-sm text-zinc-400 hover:border-zinc-600 hover:text-zinc-100"
            >
              Search “{suggestion}”
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function isMicrophoneSource(source: string): boolean {
  return source === "mic" || source === "microphone" || source === "user";
}

function sourceLabel(source: string): string {
  if (isMicrophoneSource(source)) return "Microphone";
  if (source === "system") return "System audio";
  return "Transcript";
}

function formatTimestamp(value: number): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Unknown time";
  return date.toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}
