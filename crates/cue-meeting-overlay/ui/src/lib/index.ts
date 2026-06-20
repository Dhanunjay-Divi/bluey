// Picks the adapter: real Tauri client when running inside the Tauri shell,
// the mock otherwise (browser dev / preview / design review). The UI imports
// `getClient()` and never knows which one it got.

import type { MeetingClient } from "./client";
import { createMockClient } from "./mockClient";
import { createTauriClient } from "./tauriClient";

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

let client: MeetingClient | null = null;

export function getClient(): MeetingClient {
  if (!client) client = inTauri() ? createTauriClient() : createMockClient();
  return client;
}

export type { MeetingClient } from "./client";
export * from "./types";
