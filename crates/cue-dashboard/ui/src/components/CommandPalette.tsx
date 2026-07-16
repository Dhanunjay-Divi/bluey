import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Command } from "cmdk";
import {
  newSessionActionErrorMessage,
  startNewSession,
} from "../lib/sessionActions";

/**
 * Palette command with a typed action so we can tell "plain navigation" from
 * "do a thing then navigate" at dispatch time.
 */
type PaletteCommand =
  | { label: string; kind: "navigate"; to: string }
  | { label: string; kind: "new-session" };

const commands: PaletteCommand[] = [
  { label: "New session", kind: "new-session" },
  { label: "Go to live transcript", kind: "navigate", to: "/live" },
  { label: "Go to context and memory", kind: "navigate", to: "/context" },
  { label: "Go to saved sessions", kind: "navigate", to: "/chats" },
  { label: "Go to answers", kind: "navigate", to: "/responses" },
  { label: "Search session memory", kind: "navigate", to: "/search" },
  { label: "Go to settings", kind: "navigate", to: "/settings" },
  { label: "Go to home", kind: "navigate", to: "/" },
];

export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const navigate = useNavigate();

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        if (busy) return;
        setError(null);
        setOpen((o) => !o);
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [busy]);

  async function run(cmd: PaletteCommand) {
    if (busy) return;
    if (cmd.kind === "navigate") {
      setError(null);
      navigate(cmd.to);
      setOpen(false);
      return;
    }
    // Create, activate, and only then open Live.
    setBusy(true);
    setError(null);
    try {
      await startNewSession({ navigate });
      setOpen(false);
    } catch (e) {
      setError(newSessionActionErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/65 px-4 pt-[14vh] backdrop-blur-sm"
      role="presentation"
      onMouseDown={(event) => {
        if (!busy && event.target === event.currentTarget) setOpen(false);
      }}
    >
      <Command
        label="Bluey command palette"
        aria-busy={busy}
        className="w-full max-w-xl overflow-hidden rounded-xl border border-zinc-700 bg-zinc-900 shadow-2xl"
        onKeyDown={(e: React.KeyboardEvent) => {
          if (e.key === "Escape" && !busy) setOpen(false);
        }}
      >
        <Command.Input
          placeholder={busy ? "Creating session..." : "Type a command..."}
          className="w-full border-b border-zinc-700 bg-transparent px-4 py-3 text-sm text-zinc-100 outline-none placeholder:text-zinc-500"
          disabled={busy}
        />
        {error ? (
          <p role="alert" className="border-b border-red-500/25 bg-red-950/30 px-4 py-3 text-xs leading-5 text-red-100">
            {error}
          </p>
        ) : null}
        <Command.List className="max-h-[min(420px,60vh)] overflow-auto p-2">
          <Command.Empty className="px-4 py-2 text-sm text-zinc-500">
            No results found.
          </Command.Empty>
          {commands.map((cmd) => (
            <Command.Item
              key={cmd.label}
              value={cmd.label}
              disabled={busy}
              onSelect={() => void run(cmd)}
              className="cursor-pointer rounded-md px-3 py-2.5 text-sm text-zinc-300 aria-disabled:cursor-not-allowed aria-disabled:opacity-50 aria-selected:bg-cyan-400/10 aria-selected:text-cyan-200"
            >
              {busy && cmd.kind === "new-session" ? "Starting new session..." : cmd.label}
            </Command.Item>
          ))}
        </Command.List>
      </Command>
    </div>
  );
}
