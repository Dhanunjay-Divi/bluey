import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Command } from "cmdk";
import { invoke } from "../lib/tauri";

interface Session {
  id: string;
  title: string;
}

/**
 * Palette command with a typed action so we can tell "plain navigation" from
 * "do a thing then navigate" at dispatch time.
 */
type PaletteCommand =
  | { label: string; kind: "navigate"; to: string }
  | { label: string; kind: "new-session" };

const commands: PaletteCommand[] = [
  { label: "New session", kind: "new-session" },
  { label: "Go to chats", kind: "navigate", to: "/chats" },
  { label: "Go to settings", kind: "navigate", to: "/settings" },
  { label: "Set up coach", kind: "navigate", to: "/coach" },
  { label: "Add screen context", kind: "navigate", to: "/screenshot" },
  { label: "Go to home", kind: "navigate", to: "/" },
];

export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const navigate = useNavigate();

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setOpen((o) => !o);
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  async function run(cmd: PaletteCommand) {
    if (busy) return;
    if (cmd.kind === "navigate") {
      navigate(cmd.to);
      setOpen(false);
      return;
    }
    // new-session: actually create the session, then navigate to it.
    setBusy(true);
    try {
      const s = await invoke<Session>("create_session", { title: null });
      navigate(`/session/${s.id}`);
      setOpen(false);
    } catch (e) {
      console.error("create_session via palette failed", e);
    } finally {
      setBusy(false);
    }
  }

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-[20vh] bg-black/50">
      <Command
        className="w-[500px] rounded-lg border border-zinc-700 bg-zinc-900 shadow-2xl"
        onKeyDown={(e: React.KeyboardEvent) => {
          if (e.key === "Escape") setOpen(false);
        }}
      >
        <Command.Input
          placeholder={busy ? "Creating session..." : "Type a command..."}
          className="w-full border-b border-zinc-700 bg-transparent px-4 py-3 text-sm text-zinc-100 outline-none placeholder:text-zinc-500"
          disabled={busy}
        />
        <Command.List className="max-h-[300px] overflow-auto p-2">
          <Command.Empty className="px-4 py-2 text-sm text-zinc-500">
            No results found.
          </Command.Empty>
          {commands.map((cmd) => (
            <Command.Item
              key={cmd.label}
              value={cmd.label}
              onSelect={() => run(cmd)}
              className="cursor-pointer rounded-md px-3 py-2 text-sm text-zinc-300 aria-selected:bg-blue-600/20 aria-selected:text-blue-400"
            >
              {cmd.label}
            </Command.Item>
          ))}
        </Command.List>
      </Command>
    </div>
  );
}
