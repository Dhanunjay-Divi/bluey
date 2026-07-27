// Collapse/expand the frameless overlay window between the full panel and a
// compact pill. Pure window-sizing via the Tauri window API (no Rust shell
// change needed). The expanded size is remembered so re-expanding restores
// whatever the user had resized to, falling back to the configured default.

import { useCallback, useEffect, useRef, useState } from "react";

const DEFAULT_EXPANDED = { width: 540, height: 760 };
const PILL = { width: 232, height: 60 };
// Dynamic-Island pill morph sizes. The collapsed pill is not one fixed shape:
// like the iOS Dynamic Island it GROWS to fit richer state (a detected question,
// a thinking spinner, an answer peek) and settles back to the compact caption
// bar when idle/listening. The pill component drives these via setPillSize; the
// window resize stays owned here so it never fights collapse()/expand().
export const PILL_SIZES = {
  // idle / listening — the ambient caption bar
  compact: { width: 232, height: 60 },
  // a detected for-me question OR a live "thinking" turn — one line taller/wider
  alert: { width: 320, height: 76 },
  // an answer just landed — a peek of the answer tail needs the most room
  peek: { width: 340, height: 96 },
} as const;
export type PillSize = keyof typeof PILL_SIZES;
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
    const { LogicalSize, PhysicalPosition } = await import(
      "@tauri-apps/api/dpi"
    );
    const { currentMonitor } = await import("@tauri-apps/api/window");
    await win.setSize(new LogicalSize(size.width, size.height));

    // A pill can be dragged right against any monitor edge. Growing that same
    // window back into the full panel in place used to leave most of it
    // off-screen. Clamp every morph/expand to the current monitor's work area
    // after resizing, preserving the user's position whenever it already fits.
    const monitor = await currentMonitor();
    if (!monitor) return;
    const position = await win.outerPosition();
    const width = Math.round(size.width * monitor.scaleFactor);
    const height = Math.round(size.height * monitor.scaleFactor);
    const minX = monitor.workArea.position.x;
    const minY = monitor.workArea.position.y;
    const maxX = Math.max(minX, minX + monitor.workArea.size.width - width);
    const maxY = Math.max(minY, minY + monitor.workArea.size.height - height);
    const x = Math.min(Math.max(position.x, minX), maxX);
    const y = Math.min(Math.max(position.y, minY), maxY);
    if (x !== position.x || y !== position.y) {
      await win.setPosition(new PhysicalPosition(x, y));
    }
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
    // Guard against reading a stale pill size (any morph variant) as the
    // "expanded" size. The pill can grow to PILL_SIZES.peek, so gate on the
    // tallest pill height, not the compact one.
    const maxPillH = Math.max(
      PILL_SIZES.compact.height,
      PILL_SIZES.alert.height,
      PILL_SIZES.peek.height,
    );
    if (h > maxPillH + 40) {
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
  // These values coordinate async Tauri calls without waiting for React state
  // to re-render. Window resizes are serialized and generation checked so a
  // slow pill resize can never finish after, and overwrite, a newer expand.
  const collapsedRef = useRef(false);
  const expandedSizeRef = useRef<Size>(DEFAULT_EXPANDED);
  const resizeRef = useRef({
    generation: 0,
    tail: Promise.resolve(),
  });

  const beginResize = useCallback(() => {
    resizeRef.current.generation += 1;
    return resizeRef.current.generation;
  }, []);

  const queueResize = useCallback((generation: number, size: Size) => {
    const run = resizeRef.current.tail
      .catch(() => undefined)
      .then(async () => {
        if (generation !== resizeRef.current.generation) return;
        await setWindowSize(size);
      });
    resizeRef.current.tail = run;
    return run;
  }, []);

  const collapse = useCallback(async () => {
    const generation = beginResize();
    const size = await currentExpandedSize();
    // expand() may have superseded this intent while the native size read was
    // in flight. In that case, neither state nor window size may move backward.
    if (generation !== resizeRef.current.generation) return;
    expandedSizeRef.current = size;
    collapsedRef.current = true;
    setCollapsed(true);
    await queueResize(generation, PILL);
  }, [beginResize, queueResize]);

  const expand = useCallback(async () => {
    const generation = beginResize();
    collapsedRef.current = false;
    setCollapsed(false);
    await queueResize(generation, expandedSizeRef.current);
  }, [beginResize, queueResize]);

  // Resize the collapsed pill window to one of its morph variants. No-op unless
  // collapsed (the pill only owns the window while collapsed). The pill component
  // calls this as its derived state changes (idle → detected → thinking → peek).
  const setPillSize = useCallback(
    async (size: PillSize) => {
      if (!collapsedRef.current) return;
      const generation = beginResize();
      await queueResize(generation, PILL_SIZES[size]);
    },
    [beginResize, queueResize],
  );

  return { collapsed, collapse, expand, setPillSize };
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
