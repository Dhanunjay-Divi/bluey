// @ts-nocheck
// The single client boundary. The overlay ALWAYS talks to the real daemon
// through the Tauri shell — there is no mock. (A mock adapter used to exist for
// browser design-preview, but it was removed so there is zero chance of mock
// data ever appearing in the product; everything shown is real daemon data.)
//
// Outside the Tauri shell (a plain browser) there is no daemon to talk to, so
// the client's requests simply never resolve and the UI shows its empty/loading
// states. That's intentional: the overlay is only ever run inside the shell.

import type { MeetingClient } from "./client";
import { createTauriClient } from "./tauriClient";
import { createMockClient } from "./mockClient";

/** True when running inside the Tauri shell (i.e. wired to the real daemon). */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

let client: MeetingClient | null = null;

export function getClient(): MeetingClient {
  if (!client) client = createTauriClient();
  return client;
}

export type { MeetingClient } from "./client";
export * from "./types";
