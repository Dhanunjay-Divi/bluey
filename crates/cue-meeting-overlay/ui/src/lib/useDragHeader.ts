// Make a frameless NSPanel draggable by its header.
//
// A borderless transparent NSPanel has no titlebar, and CSS
// `-webkit-app-region: drag` is NOT honored on it — so we replicate the proven
// interview-overlay approach: watch for a left-press that MOVES past a small
// threshold on the header, then hand off to the OS via `window.startDragging()`.
// A press without movement still fires the underlying control's click (we never
// preventDefault), so buttons in the header keep working.

import type { RefObject } from "react";
import { useEffect } from "react";

const DRAG_THRESHOLD = 4; // px

export function useDragHeader(ref: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const zone = ref.current;
    if (!zone) return;
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;

    const onDown = (ev: MouseEvent) => {
      if (ev.button !== 0) return;
      const startX = ev.clientX;
      const startY = ev.clientY;
      let dragging = false;
      const onMove = (mv: MouseEvent) => {
        if (dragging) return;
        if (
          Math.abs(mv.clientX - startX) > DRAG_THRESHOLD ||
          Math.abs(mv.clientY - startY) > DRAG_THRESHOLD
        ) {
          dragging = true;
          cleanup();
          void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
            void getCurrentWindow().startDragging().catch(() => {});
          });
        }
      };
      const onUp = () => cleanup();
      const cleanup = () => {
        document.removeEventListener("mousemove", onMove, true);
        document.removeEventListener("mouseup", onUp, true);
      };
      document.addEventListener("mousemove", onMove, true);
      document.addEventListener("mouseup", onUp, true);
    };

    zone.addEventListener("mousedown", onDown);
    return () => zone.removeEventListener("mousedown", onDown);
  }, [ref]);
}
