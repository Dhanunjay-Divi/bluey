// Collapse/expand the frameless overlay window between the full panel and a
// compact pill. Pure window-sizing via the Tauri window API (no Rust shell
// change needed). The expanded size is remembered so re-expanding restores
// whatever the user had resized to, falling back to the configured default.

import { useCallback, useState } from "react";

const DEFAULT_EXPANDED = { width: 540, height: 760 };
const PILL = { width: 232, height: 60 };

type Size = { width: number; height: number };

async function tauriWindow() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

async function setWindowSize(size: Size) {
  try {
    const win = await tauriWindow();
    const { LogicalSize } = await import("@tauri-apps/api/dpi");
    await win.setSize(new LogicalSize(size.width, size.height));
  } catch {
    // Non-Tauri (browser dev) — no-op.
  }
}

async function currentExpandedSize(): Promise<Size> {
  try {
    const win = await tauriWindow();
    const inner = await win.innerSize();
    const factor = await win.scaleFactor();
    const w = Math.round(inner.width / factor);
    const h = Math.round(inner.height / factor);
    // Guard against reading a stale pill size as the "expanded" size.
    if (w > PILL.width + 40 && h > PILL.height + 40) {
      return { width: w, height: h };
    }
  } catch {
    // fall through to default
  }
  return DEFAULT_EXPANDED;
}

/** `collapsed` state + toggles that also resize the OS window. */
export function useCollapse() {
  const [collapsed, setCollapsed] = useState(false);
  // Remember the expanded size across a collapse so re-expand restores it.
  const [expandedSize, setExpandedSize] = useState<Size>(DEFAULT_EXPANDED);

  const collapse = useCallback(async () => {
    const size = await currentExpandedSize();
    setExpandedSize(size);
    setCollapsed(true);
    await setWindowSize(PILL);
  }, []);

  const expand = useCallback(async () => {
    setCollapsed(false);
    await setWindowSize(expandedSize);
  }, [expandedSize]);

  return { collapsed, collapse, expand };
}
