import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

export function UpdateToast() {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = listen<string>("update_available", (event) => {
      setVersion(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  if (!version) return null;

  return (
    <div className="glass-strong fixed bottom-4 right-4 z-50 rounded-lg px-4 py-3 text-callout text-text-primary">
      <p>
        Update <strong className="text-accent-subtle-text">v{version}</strong> available.
      </p>
      <button
        onClick={() => setVersion(null)}
        className="mt-1 text-footnote text-text-tertiary underline transition-colors duration-200 hover:text-text-primary"
      >
        Dismiss
      </button>
    </div>
  );
}
