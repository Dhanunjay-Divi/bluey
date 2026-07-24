import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { MeetingProvider } from "./lib/meetingState";
import { DataProvider } from "./lib/dataStore";
import "./styles/aurora.css";
import "./styles/floorplan.css";

const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// Render first, so the window is never blank even if the env probes below race.
// MeetingProvider sits ABOVE <App/> so the meeting's session state (transcript,
// history, Q&A, detected question) is never unmounted by collapse/onboarding/tab
// switching — the meeting is the source of truth and this is its durable view.
// DataProvider sits alongside it (SWR store for agents / meetings / sessions) so
// discovery data is cached across tab switches and revalidated in the background
// instead of being thrown away and refetched on every mount.
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <MeetingProvider>
      <DataProvider>
        <App />
      </DataProvider>
    </MeetingProvider>
  </StrictMode>,
);

// In the browser (dev/preview) there is no native window, so paint the aurora
// field as a backdrop. In Tauri the window is transparent + frameless and the
// glass panel paints itself (the panel is the whole UI — same in real and
// capture-visible modes), so no body backdrop.
if (!inTauri) {
  document.body.classList.add("dev-backdrop");
}

// No JS auto-resize: the panel FILLS the fixed-size window (pinned to all edges,
// content scrolls inside), the same approach as the interview overlay — so there
// is never empty space around the panel.
