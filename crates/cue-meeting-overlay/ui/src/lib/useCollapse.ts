// Collapse/expand the frameless overlay window between the full panel and a
// compact pill. Pure window-sizing via the Tauri window API (no Rust shell
// change needed). The expanded size is remembered so re-expanding restores
// whatever the user had resized to, falling back to the configured default.

import { useCallback, useEffect, useState } from "react";

const DEFAULT_EXPANDED = { width: 540, height: 760 };
const PILL = { width: 232, height: 60 };
// Onboarding is a small centered card, NOT the full Ask panel — sizing the window
// to the full panel made the card float in a large empty box. This is just big
// enough for the tallest step (attach: up to 4 agent rows).
const ONBOARDING = { width: 420, height: 520 };

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

/**
 * While `active` (first-run onboarding), shrink the OS window to a compact
 * card-sized frame so the onboarding card isn't a small panel floating in a big
 * empty window. When onboarding ends, restore the full expanded panel size so
 * the Ask screen fills the window as before. No-op in the collapsed (pill) state
 * — the pill owns the window size then.
 */
export function useOnboardingWindowSize(active: boolean, collapsed: boolean) {
  useEffect(() => {
    if (collapsed) return;
    let cancelled = false;
    (async () => {
      if (active) {
        await setWindowSize(ONBOARDING);
      } else {
        // Restore to whatever the panel should be (the remembered/expanded size).
        const size = await currentExpandedSize();
        // If we're coming straight out of onboarding the current window IS the
        // compact onboarding size, so currentExpandedSize() falls back to the
        // default — which is the correct full-panel size to restore to.
        const restore =
          size.width <= ONBOARDING.width + 40 &&
          size.height <= ONBOARDING.height + 40
            ? DEFAULT_EXPANDED
            : size;
        if (!cancelled) await setWindowSize(restore);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [active, collapsed]);
}
