import { useEffect, useState, type FormEvent } from "react";
import { Building2, CheckCircle2, LoaderCircle, Radar, Search } from "lucide-react";
import type {
  CareerTrack,
  DiscoverySource,
  DiscoverySourceCatalogEntry,
  DiscoverySourceCatalogResponse,
} from "../types";
import { titleCase } from "../lib/format";
import { Dialog } from "./Dialog";

const providerOptions = [
  ["all", "All supported systems"],
  ["greenhouse", "Greenhouse"],
  ["lever", "Lever"],
  ["ashby", "Ashby"],
  ["smartrecruiters", "SmartRecruiters"],
  ["workday", "Workday"],
] as const;

interface Props {
  open: boolean;
  tracks: CareerTrack[];
  initialTrackId: string;
  onClose(): void;
  onSearch(query: string, trackId: string, provider: string): Promise<DiscoverySourceCatalogResponse>;
  onConnect(trackId: string, entry: DiscoverySourceCatalogEntry): Promise<DiscoverySource>;
}

export function DiscoverySourceDialog({
  open,
  tracks,
  initialTrackId,
  onClose,
  onSearch,
  onConnect,
}: Props) {
  const activeTracks = tracks.filter((track) => track.active);
  const [trackId, setTrackId] = useState(initialTrackId);
  const [provider, setProvider] = useState("all");
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<DiscoverySourceCatalogEntry[]>([]);
  const [searched, setSearched] = useState(false);
  const [searching, setSearching] = useState(false);
  const [connectingId, setConnectingId] = useState("");
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");

  useEffect(() => {
    if (!open) return;
    setTrackId(initialTrackId || activeTracks[0]?.id || "");
    setProvider("all");
    setQuery("");
    setResults([]);
    setSearched(false);
    setSearching(false);
    setConnectingId("");
    setError("");
    setMessage("");
  }, [open, initialTrackId]);

  const search = async (event: FormEvent) => {
    event.preventDefault();
    const normalizedQuery = query.trim();
    if (!trackId || normalizedQuery.length < 2) return;
    setSearching(true);
    setSearched(false);
    setError("");
    setMessage("");
    try {
      const response = await onSearch(normalizedQuery, trackId, provider);
      setResults(response.entries);
      setSearched(true);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Bluey could not search company career pages.");
    } finally {
      setSearching(false);
    }
  };

  const connect = async (entry: DiscoverySourceCatalogEntry) => {
    setConnectingId(entry.id);
    setError("");
    setMessage("");
    try {
      await onConnect(trackId, entry);
      setResults((current) => current.map((item) => item.id === entry.id ? { ...item, connected: true } : item));
      setMessage(`${entry.company} is now watched for this Career Track.`);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Bluey could not connect that company.");
    } finally {
      setConnectingId("");
    }
  };

  return (
    <Dialog
      open={open}
      title="Watch company career pages"
      description="Find employers on supported application systems. Bluey checks each original posting before it becomes a match."
      onClose={onClose}
    >
      <form className="source-picker" onSubmit={(event) => void search(event)}>
        <div className="source-picker-controls">
          <label>
            <span>Career Track</span>
            <select value={trackId} onChange={(event) => setTrackId(event.target.value)}>
              {activeTracks.map((track) => <option key={track.id} value={track.id}>{track.name}</option>)}
            </select>
          </label>
          <label>
            <span>Application system</span>
            <select value={provider} onChange={(event) => setProvider(event.target.value)}>
              {providerOptions.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
            </select>
          </label>
        </div>
        <label className="source-picker-search">
          <span>Company</span>
          <div>
            <Search size={17} aria-hidden="true" />
            <input
              data-dialog-initial-focus
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search Apex Systems, Capital One, Stripe..."
              minLength={2}
              maxLength={80}
              autoComplete="off"
            />
            <button className="button primary compact" type="submit" disabled={searching || !trackId || query.trim().length < 2}>
              {searching ? <LoaderCircle className="spin" size={15} /> : <Radar size={15} />}
              {searching ? "Searching" : "Search"}
            </button>
          </div>
        </label>
      </form>

      <div className="source-picker-body" aria-live="polite">
        {error && <div className="inline-error" role="alert">{error}</div>}
        {message && <div className="inline-success" role="status">{message}</div>}
        {!searched && !searching && !error && (
          <div className="source-picker-prompt"><Building2 size={20} /><p>Search by employer name, then connect the public career page to one Career Track.</p></div>
        )}
        {searched && results.length === 0 && (
          <div className="source-picker-prompt"><Search size={20} /><p>No supported company source found. You can still add a direct job link from Matches.</p></div>
        )}
        {results.length > 0 && (
          <ul className="source-picker-results" aria-label="Company career pages">
            {results.map((entry) => (
              <li key={entry.id}>
                <span className="source-company-icon"><Building2 size={17} /></span>
                <span><strong>{entry.company}</strong><small>{titleCase(entry.provider)}</small></span>
                {entry.connected ? (
                  <span className="source-connected"><CheckCircle2 size={15} />Watching</span>
                ) : (
                  <button className="button secondary compact" type="button" disabled={Boolean(connectingId)} onClick={() => void connect(entry)}>
                    {connectingId === entry.id ? <LoaderCircle className="spin" size={15} /> : <Radar size={15} />}
                    {connectingId === entry.id ? "Connecting" : "Watch"}
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="dialog-actions">
        <button className="button secondary" type="button" onClick={onClose}>Done</button>
      </div>
    </Dialog>
  );
}
