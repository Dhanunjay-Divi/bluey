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
    <div className="fixed bottom-4 right-4 z-50 rounded-lg bg-blue-600 px-4 py-3 text-sm text-white shadow-lg">
      <p>
        Update <strong>v{version}</strong> available.
      </p>
      <button
        onClick={() => setVersion(null)}
        className="mt-1 text-xs underline opacity-80 hover:opacity-100"
      >
        Dismiss
      </button>
    </div>
  );
}
