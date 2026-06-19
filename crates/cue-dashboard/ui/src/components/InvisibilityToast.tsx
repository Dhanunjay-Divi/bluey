import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Eye, EyeOff } from "lucide-react";

/**
 * Codex Stage 18 follow-up: brief in-dashboard toast that confirms
 * an invisibility toggle actually took effect. The overlay shows
 * its own "Bluey hidden — press F19 to restore" toast on hide;
 * this is the symmetric feedback in the dashboard window itself.
 *
 * Subscribes to "invisibility_changed" event the daemon emits from
 * the invisibility_toggle command. Auto-dismisses after 1.6s.
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
      className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 inline-flex items-center gap-2 rounded-full border border-hairline bg-bg-raised/95 px-4 py-2 shadow-lg shadow-black/30 backdrop-blur animate-in fade-in slide-in-from-bottom-2 duration-150"
      role="status"
      aria-live="polite"
    >
      {isHidden ? (
        <EyeOff className="h-4 w-4 text-text-secondary" />
      ) : (
        <Eye className="h-4 w-4 text-accent-subtle-text" />
      )}
      <span className="text-footnote font-medium text-text-primary">
        {isHidden ? "Bluey hidden — press F19 to restore" : "Bluey visible"}
      </span>
    </div>
  );
}
