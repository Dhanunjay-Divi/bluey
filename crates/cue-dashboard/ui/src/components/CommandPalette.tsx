import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Command } from "cmdk";

const commands = [
  { label: "New session", action: "/chats" },
  { label: "Go to chats", action: "/chats" },
  { label: "Go to settings", action: "/settings" },
  { label: "Go to prompts", action: "/prompts" },
  { label: "Go to home", action: "/" },
];

export function CommandPalette() {
  const [open, setOpen] = useState(false);
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
          placeholder="Type a command..."
          className="w-full border-b border-zinc-700 bg-transparent px-4 py-3 text-sm text-zinc-100 outline-none placeholder:text-zinc-500"
        />
        <Command.List className="max-h-[300px] overflow-auto p-2">
          <Command.Empty className="px-4 py-2 text-sm text-zinc-500">
            No results found.
          </Command.Empty>
          {commands.map((cmd) => (
            <Command.Item
              key={cmd.label}
              value={cmd.label}
              onSelect={() => {
                navigate(cmd.action);
                setOpen(false);
              }}
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
