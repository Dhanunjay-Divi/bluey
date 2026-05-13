import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

interface Session {
  id: string;
  title: string;
  status: string;
  created_at: number;
  updated_at: number;
  token_count: number;
}

/**
 * Listen for session events emitted from the Rust backend.
 * Phase 1: channel is wired but events are not yet emitted.
 * Phase 2+ will emit these events when sessions change.
 */
export function useSessionEvents(onSessionCreated?: (s: Session) => void) {
  useEffect(() => {
    const unlisten = listen<Session>("session:created", (event) => {
      onSessionCreated?.(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [onSessionCreated]);
}
