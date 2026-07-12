import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Eye, EyeOff } from "lucide-react";

/**
 * Brief in-dashboard toast that confirms an overlay visibility change.
 *
 * Subscribes to the legacy-named "invisibility_changed" event emitted by
 * the show/hide command. Auto-dismisses after 1.6s.
 */
export function InvisibilityToast() {
  const [state, setState] = useState<"hidden" | "visible" | null>(null);

  useEffect(() => {
    const unlisten = listen<boolean>("invisibility_changed", (event) => {
      setState(event.payload ? "hidden" : "visible");
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (!state) return;
    const t = setTimeout(() => setState(null), 1600);
    return () => clearTimeout(t);
  }, [state]);

  if (!state) return null;

  const isHidden = state === "hidden";
  return (
    <div
      className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 inline-flex items-center gap-2 rounded-full bg-zinc-900/95 border border-zinc-700 px-4 py-2 shadow-lg shadow-black/30 backdrop-blur animate-in fade-in slide-in-from-bottom-2 duration-150"
      role="status"
      aria-live="polite"
    >
      {isHidden ? (
        <EyeOff className="h-4 w-4 text-zinc-300" />
      ) : (
        <Eye className="h-4 w-4 text-blue-400" />
      )}
      <span className="text-xs font-medium text-zinc-100">
        {isHidden ? "Overlay hidden. Press F19 to restore." : "Overlay visible"}
      </span>
    </div>
  );
}
