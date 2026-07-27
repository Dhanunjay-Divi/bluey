// Make a frameless NSPanel draggable by its header.
//
// A borderless transparent NSPanel has no titlebar. Tauri's native
// A small movement threshold keeps ordinary clicks working on collapsed pills.
// Once crossed, hand the pointer gesture to the native window and report that
// the eventual click came from a drag so click-to-expand surfaces can consume
// it.

import { getCurrentWindow } from "@tauri-apps/api/window";
import type { MutableRefObject, RefObject } from "react";
import { useEffect, useRef } from "react";

const DRAG_THRESHOLD = 4; // px

export function useDragHeader(
  ref: RefObject<HTMLElement | null>,
  active = true,
): MutableRefObject<boolean> {
  const didDrag = useRef(false);

  useEffect(() => {
    if (!active) return;
    const zone = ref.current;
    if (!zone) return;
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window))
      return;

    const onDown = (ev: MouseEvent) => {
      if (ev.button !== 0) return;
      didDrag.current = false;
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
          didDrag.current = true;
          cleanup();
          void getCurrentWindow()
            .startDragging()
            .catch(() => {});
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
  }, [active, ref]);

  return didDrag;
}
