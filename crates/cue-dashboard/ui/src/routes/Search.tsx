import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { Search as SearchIcon } from "lucide-react";

interface TranscriptHit {
  session_id: string;
  text: string;
  snippet: string;
  source: string;
  ts: number;
  rank: number;
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
        <SearchIcon size={16} className="absolute left-3 top-2.5 text-zinc-500" />
        <input
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search transcripts..."
          className="w-full rounded-md border border-zinc-700 bg-zinc-900 pl-9 pr-3 py-2 text-sm"
        />
      </div>
      {loading && <p className="text-xs text-zinc-500">Searching...</p>}
      {results.length > 0 && (
        <ul className="space-y-2">
          {results.map((hit, i) => (
            <li
              key={`${hit.session_id}-${i}`}
              className="cursor-pointer rounded-md border border-zinc-800 bg-zinc-900 px-4 py-3 text-sm hover:border-zinc-600"
              onClick={() => navigate(`/session/${hit.session_id}`)}
            >
              <div className="flex items-center gap-2 text-xs text-zinc-500 mb-1">
                <span className={`rounded px-1.5 py-0.5 ${hit.source === "mic" ? "bg-blue-900/40 text-blue-300" : "bg-purple-900/40 text-purple-300"}`}>
                  {hit.source}
                </span>
                <span>{new Date(hit.ts).toLocaleTimeString()}</span>
              </div>
              <p className="text-zinc-300" dangerouslySetInnerHTML={{ __html: hit.snippet.replace(/\u00AB/g, '<mark class="bg-yellow-600/40 text-yellow-200 rounded px-0.5">').replace(/\u00BB/g, '</mark>') }} />
            </li>
          ))}
        </ul>
      )}
      {!loading && query.trim() && results.length === 0 && (
        <p className="text-sm text-zinc-500">No results found.</p>
      )}
    </div>
  );
}
