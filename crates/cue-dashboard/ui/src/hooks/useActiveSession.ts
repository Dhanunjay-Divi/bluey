import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "../lib/tauri";

interface SessionSwitchedPayload {
  id: string | null;
}

/**
 * Hook exposing the currently-active session id.
 *
 * Pulls the initial value from the daemon on mount, then updates whenever the
 * daemon emits `session:switched`. Call `setActive(id | null)` to change the
 * active session — the daemon is the source of truth, so this round-trips
 * through the `set_active_session` Tauri command before local state updates.
 */
export function useActiveSession() {
  const [activeId, setActiveId] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    invoke<string | null>("get_active_session")
      .then((id) => {
        if (alive) setActiveId(id);
      })
      .catch(() => {
        /* best-effort initial load */
      });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<SessionSwitchedPayload>("session:switched", (event) => {
      setActiveId(event.payload.id);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const setActive = useCallback(async (id: string | null) => {
    await invoke("set_active_session", { id });
  }, []);

  return { activeId, setActive };
}
