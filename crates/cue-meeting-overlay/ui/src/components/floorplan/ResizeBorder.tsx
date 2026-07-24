// All-edge / all-corner resize for the frameless overlay window.
//
// The overlay is a borderless macOS NSPanel (for screen-share invisibility), so
// the OS `startResizeDragging` silently no-ops — the same reason ResizeGrip does
// manual `setSize`. This extends that approach to the whole perimeter: eight
// invisible drag zones (4 edges + 4 corners). Resizing from the top or left
// edge also moves the window origin (so the opposite edge stays put), so those
// handles set BOTH position and size each move. No-op outside Tauri.

import { useRef } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";

const MIN_W = 320;
const MIN_H = 240;

// The eight resize directions, each a mix of horizontal/vertical edges.
type Dir = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

interface Handle {
  dir: Dir;
  cursor: string;
  style: React.CSSProperties;
}

// Perimeter geometry: a THICK px band on each edge + a slightly larger square at
// each corner, all absolutely positioned inside the panel. Edges leave room for
// the corners so a corner drag resizes on both axes.
const THICK = 6; // edge grab thickness
const CORNER = 14; // corner grab square

const HANDLES: Handle[] = [
  // edges
  { dir: "n", cursor: "ns-resize", style: { top: 0, left: CORNER, right: CORNER, height: THICK } },
  { dir: "s", cursor: "ns-resize", style: { bottom: 0, left: CORNER, right: CORNER, height: THICK } },
  { dir: "w", cursor: "ew-resize", style: { left: 0, top: CORNER, bottom: CORNER, width: THICK } },
  { dir: "e", cursor: "ew-resize", style: { right: 0, top: CORNER, bottom: CORNER, width: THICK } },
  // corners
  { dir: "nw", cursor: "nwse-resize", style: { top: 0, left: 0, width: CORNER, height: CORNER } },
  { dir: "ne", cursor: "nesw-resize", style: { top: 0, right: 0, width: CORNER, height: CORNER } },
  { dir: "sw", cursor: "nesw-resize", style: { bottom: 0, left: 0, width: CORNER, height: CORNER } },
  { dir: "se", cursor: "nwse-resize", style: { bottom: 0, right: 0, width: CORNER, height: CORNER } },
];

export function ResizeBorder() {
  // Live drag state captured at press: the window's logical size + origin, the
  // mouse anchor (screen coords), the direction, and the API closures.
  const drag = useRef<{
    dir: Dir;
    startX: number;
    startY: number;
    startW: number;
    startH: number;
    startPosX: number;
    startPosY: number;
    setSize: (w: number, h: number) => void;
    setPosition: (x: number, y: number) => void;
  } | null>(null);

  const begin = (dir: Dir) => async (ev: ReactMouseEvent) => {
    if (ev.button !== 0) return;
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
    ev.preventDefault();
    ev.stopPropagation();
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const { LogicalSize, LogicalPosition } = await import("@tauri-apps/api/dpi");
      const win = getCurrentWindow();
      const factor = await win.scaleFactor();
      const inner = await win.innerSize();
      const pos = await win.outerPosition();
      drag.current = {
        dir,
        startX: ev.screenX,
        startY: ev.screenY,
        startW: Math.round(inner.width / factor),
        startH: Math.round(inner.height / factor),
        startPosX: Math.round(pos.x / factor),
        startPosY: Math.round(pos.y / factor),
        setSize: (w, h) => void win.setSize(new LogicalSize(w, h)),
        setPosition: (x, y) => void win.setPosition(new LogicalPosition(x, y)),
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp, { once: true });
    } catch {
      // Non-Tauri / API unavailable — inert.
    }
  };

  const onMove = (ev: MouseEvent) => {
    const d = drag.current;
    if (!d) return;
    const dx = ev.screenX - d.startX;
    const dy = ev.screenY - d.startY;
    const east = d.dir.includes("e");
    const west = d.dir.includes("w");
    const north = d.dir.includes("n");
    const south = d.dir.includes("s");

    let w = d.startW;
    let h = d.startH;
    let x = d.startPosX;
    let y = d.startPosY;

    if (east) w = d.startW + dx;
    if (west) w = d.startW - dx;
    if (south) h = d.startH + dy;
    if (north) h = d.startH - dy;

    // Clamp to the minimum; when dragging a top/left edge, the origin shifts by
    // the ACTUAL size change (after clamping) so the opposite edge stays fixed.
    w = Math.max(MIN_W, w);
    h = Math.max(MIN_H, h);
    if (west) x = d.startPosX + (d.startW - w);
    if (north) y = d.startPosY + (d.startH - h);

    if (west || north) d.setPosition(x, y);
    d.setSize(w, h);
  };

  const onUp = () => {
    drag.current = null;
    window.removeEventListener("mousemove", onMove);
  };

  return (
    <>
      {HANDLES.map((h) => (
        <div
          key={h.dir}
          onMouseDown={begin(h.dir)}
          aria-hidden
          style={{
            position: "absolute",
            zIndex: 60,
            cursor: h.cursor,
            // -webkit-app-region:no-drag so these win over any drag region.
            ...({ WebkitAppRegion: "no-drag" } as React.CSSProperties),
            ...h.style,
          }}
        />
      ))}
    </>
  );
}
