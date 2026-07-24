// Aurora Glass primitives — the small, reusable building blocks every screen
// composes. Styles live in aurora.css (tokens) + inline style for layout. No
// component invents a color; everything reads from the design tokens.

import type { CSSProperties, MouseEvent as ReactMouseEvent, ReactNode } from "react";
import { useEffect, useRef, useState } from "react";

/** A frosted glass surface with the aurora wash refracting inside it. */
export function Glass({
  children,
  radius = "var(--r-xl)",
  style,
  className = "",
}: {
  children: ReactNode;
  radius?: string;
  style?: CSSProperties;
  className?: string;
}) {
  return (
    // `position: relative` is LOAD-BEARING: it makes this the containing block
    // for the absolutely-positioned `ResizeGrip` (bottom-right). Without it the
    // grip escaped to the nearest positioned ancestor and landed off-target, so
    // the corner drag silently did nothing. Callers may override via `style`,
    // but a positioned Glass is the contract the grip depends on.
    <div
      className={`glass ${className}`}
      style={{ position: "relative", borderRadius: radius, ...style }}
    >
      <div className="aurora" />
      {children}
    </div>
  );
}

/** The Bluey gradient mark. */
export function Mark({ size = 22 }: { size?: number }) {
  return (
    <span
      style={{
        width: size,
        height: size,
        borderRadius: size * 0.32,
        background:
          "linear-gradient(140deg, var(--violet), var(--blue) 55%, var(--mint))",
        boxShadow: "inset 0 0 0 1px rgba(255,255,255,.5)",
        display: "inline-block",
        flex: "none",
      }}
    />
  );
}

/** The listening waveform (3 bars). */
export function Waveform() {
  return (
    <span style={{ display: "inline-flex", alignItems: "flex-end", gap: 2.5, height: 13 }}>
      {[5, 12, 8].map((h, i) => (
        <span
          key={i}
          style={{
            width: 2.5,
            height: h,
            borderRadius: 2,
            background: "linear-gradient(var(--violet), var(--blue))",
            animation: "aurora-wave 1.1s ease-in-out infinite",
            animationDelay: `${i * 0.17}s`,
          }}
        />
      ))}
    </span>
  );
}

/** A status dot with a soft halo. */
export function Dot({ color = "var(--mint)", halo = true }: { color?: string; halo?: boolean }) {
  return (
    <span
      style={{
        width: 7,
        height: 7,
        borderRadius: "50%",
        background: color,
        boxShadow: halo ? `0 0 0 4px ${color}38` : "none",
        flex: "none",
      }}
    />
  );
}

/** Segmented control — Ask / History / Agents. */
export function SegmentedTabs<T extends string>({
  tabs,
  value,
  onChange,
}: {
  tabs: readonly T[];
  value: T;
  onChange: (t: T) => void;
}) {
  return (
    <div style={{ display: "flex", gap: 2, background: "rgba(20,22,28,.05)", borderRadius: 11, padding: 3 }}>
      {tabs.map((t) => {
        const on = t === value;
        return (
          <button
            key={t}
            onClick={() => onChange(t)}
            style={{
              border: "none",
              cursor: "pointer",
              background: on ? "var(--glass-solid)" : "transparent",
              color: on ? "var(--ink)" : "var(--ink-2)",
              boxShadow: on ? "0 1px 3px rgba(30,30,60,.12)" : "none",
              fontSize: 12,
              fontWeight: 540,
              letterSpacing: "-.01em",
              padding: "5px 13px",
              borderRadius: 8,
            }}
          >
            {t}
          </button>
        );
      })}
    </div>
  );
}

/** A soft pill toggle. */
export function Toggle({ on, onClick }: { on: boolean; onClick?: () => void }) {
  return (
    <button
      onClick={onClick}
      aria-pressed={on}
      style={{
        width: 38,
        height: 23,
        borderRadius: "var(--r-pill)",
        border: "none",
        cursor: "pointer",
        position: "relative",
        flex: "none",
        background: on ? "var(--ink)" : "rgba(20,22,28,.12)",
        boxShadow: on ? "0 2px 6px -2px rgba(28,26,25,.35)" : "none",
        transition: ".18s",
      }}
    >
      <span
        style={{
          position: "absolute",
          top: 2.5,
          left: on ? 17.5 : 2.5,
          width: 18,
          height: 18,
          borderRadius: "50%",
          background: "#fff",
          boxShadow: "0 1px 3px rgba(0,0,0,.25)",
          transition: ".18s",
        }}
      />
    </button>
  );
}

const SOURCE_COLORS: Record<string, string> = {
  jira: "var(--blue)",
  github: "var(--violet)",
  supabase: "var(--mint)",
  perplexity: "var(--tint)",
  default: "var(--ink-3)",
};

/** A grounding-source row shown under an answer. */
export function SourceRow({ kind, label, note }: { kind: string; label: string; note?: string }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 9,
        fontSize: 12,
        color: "var(--ink-2)",
        background: "var(--glass-solid)",
        border: "1px solid var(--line)",
        borderRadius: 10,
        padding: "7px 10px",
      }}
    >
      <span style={{ width: 7, height: 7, borderRadius: "50%", flex: "none", background: SOURCE_COLORS[kind] ?? SOURCE_COLORS.default }} />
      <b style={{ color: "var(--ink)", fontWeight: 560 }}>{label}</b>
      {note && <span style={{ marginLeft: "auto", color: "var(--ok)" }}>{note}</span>}
    </div>
  );
}

/** The honest slow-answer state — the agent is driving + doing MCP round-trips.
 *
 *  Mirrors what production AI chat UIs (ChatGPT, Claude) show before the first
 *  token: a LIVE ELAPSED TIMER (the one genuinely-true signal — time really did
 *  pass) plus a short, gently-rotating honest hint. The UX guidance is explicit
 *  — animate + hint + timer, never an indefinite loader, and NEVER invent fake
 *  tool names you can't substantiate. Our daemon emits no real tool-step events
 *  during an ask, so we show truthful generic phases, not fabricated steps. */
export function ThinkingState({ detail }: { detail: string }) {
  // Honest, generic phases — these describe what is genuinely happening (the
  // agent is driving its session + connectors), not specific tool calls we'd
  // be making up. They rotate to feel alive, like "Planning steps…".
  const phases = [detail, "Working in your session…", "Pulling context…", "Composing the answer…"];
  const [elapsed, setElapsed] = useState(0);
  const [phaseIdx, setPhaseIdx] = useState(0);
  const startRef = useRef<number | null>(null);

  useEffect(() => {
    startRef.current = performance.now();
    const tick = window.setInterval(() => {
      if (startRef.current != null) {
        setElapsed((performance.now() - startRef.current) / 1000);
      }
    }, 100);
    // Advance the hint every ~2.2s, capped at the last (most honest) phase.
    const rot = window.setInterval(() => {
      setPhaseIdx((i) => Math.min(i + 1, phases.length - 1));
    }, 2200);
    return () => {
      window.clearInterval(tick);
      window.clearInterval(rot);
    };
    // phases is derived from `detail`; re-run only when detail changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detail]);

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 11,
        margin: "6px 12px",
        padding: "12px 14px",
        borderRadius: "var(--r-lg)",
        background: "var(--glass-2)",
        boxShadow: "inset 0 0 0 1px rgba(255,255,255,.5)",
      }}
    >
      <span
        style={{
          width: 18,
          height: 18,
          borderRadius: "50%",
          flex: "none",
          background: "conic-gradient(var(--violet),var(--blue),var(--mint),var(--violet))",
          animation: "aurora-spin 1.4s linear infinite",
        }}
      />
      <span style={{ fontSize: 12.5, color: "var(--ink-2)", lineHeight: 1.4, flex: 1 }}>
        <b style={{ color: "var(--ink)", fontWeight: 540 }}>Thinking</b>
        <span style={{ color: "var(--ink-4)", fontVariantNumeric: "tabular-nums" }}>
          {" · "}
          {elapsed.toFixed(1)}s
        </span>
        <br />
        <span
          key={phaseIdx}
          style={{ fontSize: 11, animation: "aurora-fade-in .3s ease both", display: "inline-block" }}
        >
          {phases[phaseIdx]}
        </span>
      </span>
    </div>
  );
}

/** Bottom-right resize grip for the frameless panel.
 *
 *  MANUAL resize via `window.setSize`, NOT the OS `startResizeDragging`. The
 *  overlay window is a borderless macOS **NSPanel** (converted in the Rust shell
 *  for screen-share invisibility + float-over behavior). A borderless NSPanel has
 *  no resizable frame, so `startResizeDragging` — which asks the window server to
 *  begin an edge-resize — silently no-ops on it (the long-standing "corner drag
 *  does nothing" bug). `setSize` works on ANY window type (it's how `useCollapse`
 *  already switches pill↔panel), so we track the pointer ourselves and set the
 *  new size each move. No-op in the browser (no Tauri). */
const MIN_W = 320;
const MIN_H = 240;

export function ResizeGrip() {
  // Live drag state: the window's logical size + the mouse anchor at press.
  const drag = useRef<{
    startX: number;
    startY: number;
    startW: number;
    startH: number;
    setSize: (w: number, h: number) => void;
  } | null>(null);

  const onDown = async (ev: ReactMouseEvent) => {
    if (ev.button !== 0) return;
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
    ev.preventDefault();
    ev.stopPropagation();
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const { LogicalSize } = await import("@tauri-apps/api/dpi");
      const win = getCurrentWindow();
      const inner = await win.innerSize();
      const factor = await win.scaleFactor();
      drag.current = {
        startX: ev.screenX,
        startY: ev.screenY,
        startW: Math.round(inner.width / factor),
        startH: Math.round(inner.height / factor),
        setSize: (w, h) => void win.setSize(new LogicalSize(w, h)),
      };
      // Capture the pointer on the window so the drag keeps working even if the
      // cursor leaves the tiny grip mid-move.
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp, { once: true });
    } catch {
      // Non-Tauri / API unavailable — leave the grip inert.
    }
  };

  const onMove = (ev: MouseEvent) => {
    const d = drag.current;
    if (!d) return;
    // SouthEast resize: new size = start size + mouse delta (screen coords, so
    // window movement never confuses the delta). Clamp to a usable minimum.
    const w = Math.max(MIN_W, d.startW + (ev.screenX - d.startX));
    const h = Math.max(MIN_H, d.startH + (ev.screenY - d.startY));
    d.setSize(w, h);
  };

  const onUp = () => {
    drag.current = null;
    window.removeEventListener("mousemove", onMove);
  };

  return (
    <div
      onMouseDown={onDown}
      aria-label="Resize"
      style={{
        position: "absolute",
        right: 0,
        bottom: 0,
        // A SMALL 16px corner target: the transcript bar's expand chevron lives
        // in the bottom row's right edge, so a large grip here swallowed the
        // chevron's clicks (expand did nothing). Keeping the grip to just the
        // true corner — and the caption row reserving ~26px right-padding —
        // leaves the chevron (which sits higher/left of this) fully clickable.
        width: 16,
        height: 16,
        padding: "0 2px 2px 0",
        boxSizing: "border-box",
        cursor: "nwse-resize",
        color: "var(--ink-4)",
        display: "flex",
        alignItems: "flex-end",
        justifyContent: "flex-end",
        zIndex: 50,
      }}
    >
      <svg viewBox="0 0 14 14" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
        <path d="M13 4L4 13M13 8L8 13M13 12L12 13" />
      </svg>
    </div>
  );
}

/** An icon-ish glyph button (text glyph; the Tauri build can swap for SF Symbols). */
export function GlyphButton({
  glyph,
  label,
  accent = false,
  onClick,
  size = 28,
}: {
  glyph: string;
  label: string;
  accent?: boolean;
  onClick?: () => void;
  size?: number;
}) {
  return (
    <button
      onClick={onClick}
      aria-label={label}
      title={label}
      style={{
        width: size,
        height: size,
        borderRadius: size >= 30 ? 9 : "50%",
        border: "none",
        cursor: "pointer",
        background: accent ? "var(--tint-wash)" : "transparent",
        color: accent ? "var(--tint-ink)" : "var(--ink-2)",
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        fontSize: 14,
        transition: ".15s",
      }}
    >
      {glyph}
    </button>
  );
}
