import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles/aurora.css";

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// Render first, so the window is never blank even if the env probes below race.
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
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
