import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "../lib/tauri";
import { Search as SearchIcon } from "lucide-react";

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
      <mark key={i} className="bg-warning/10 text-warning rounded px-0.5">
        {p.text}
      </mark>
    ) : (
      <span key={i}>{p.text}</span>
    )
  );
}

export function Search() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<TranscriptHit[]>([]);
  const [loading, setLoading] = useState(false);
  const navigate = useNavigate();
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const doSearch = useCallback(async (q: string) => {
    if (!q.trim()) { setResults([]); return; }
    setLoading(true);
    try {
      const hits = await invoke<TranscriptHit[]>("search_transcripts", { query: q, limit: 50 });
      setResults(hits);
    } catch { setResults([]); }
    finally { setLoading(false); }
  }, []);

  useEffect(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => doSearch(query), 200);
    return () => { if (timerRef.current) clearTimeout(timerRef.current); };
  }, [query, doSearch]);

  return (
    <div className="space-y-4">
      <div className="relative">
        <SearchIcon size={16} className="absolute left-3 top-2.5 text-text-tertiary" />
        <input
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search transcripts..."
          className="w-full rounded-md border border-hairline bg-bg-input pl-9 pr-3 py-2 text-callout"
        />
      </div>
      {loading && <p className="text-footnote text-text-tertiary">Searching...</p>}
      {results.length > 0 && (
        <ul className="space-y-2">
          {results.map((hit, i) => (
            <li
              key={`${hit.session_id}-${i}`}
              className="glass cursor-pointer rounded-md px-4 py-3 text-callout hover:border-hairline-strong"
              onClick={() => navigate(`/session/${hit.session_id}`)}
            >
              <div className="flex items-center gap-2 text-footnote text-text-tertiary mb-1">
                <span className={`rounded px-1.5 py-0.5 ${hit.source === "mic" ? "bg-accent-subtle text-accent-subtle-text" : "bg-bg-raised-2 text-text-tertiary"}`}>
                  {hit.source}
                </span>
                <span>{new Date(hit.ts).toLocaleTimeString()}</span>
              </div>
              <p className="text-text-secondary">{renderSnippet(hit.snippet)}</p>
            </li>
          ))}
        </ul>
      )}
      {!loading && query.trim() && results.length === 0 && (
        <p className="text-callout text-text-tertiary">No results found.</p>
      )}
    </div>
  );
}
