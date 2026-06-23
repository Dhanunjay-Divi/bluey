// The "+" context menu — opens from the composer's + button. Matches the
// designed menu (docs/design mockup + interview overlay): Capture page / Attach
// files / Take a screenshot. Each item maps to a REAL daemon action via the
// client; nothing here is a stub.
//
// "Attach files" opens the native file picker (Tauri dialog plugin) and hands
// the chosen paths to the daemon, which attaches them as context artifacts.

import { useEffect, useRef } from "react";
import { getClient } from "../lib";

export function PlusMenu({ onClose }: { onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const client = getClient();

  // Dismiss on outside-click / Escape — standard popover behavior.
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", onDown, true);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onDown, true);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [onClose]);

  const pickFiles = async () => {
    onClose();
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({ multiple: true, directory: false });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      client.attachFiles(paths);
    } catch (e) {
      console.error("[PlusMenu] file pick failed", e);
    }
  };

  const capturePage = () => {
    onClose();
    client.capturePage();
  };

  const screenshot = () => {
    onClose();
    client.captureScreenshot();
  };

  return (
    <div
      ref={ref}
      role="menu"
      style={{
        position: "absolute",
        left: 14,
        bottom: 60,
        width: 208,
        zIndex: 20,
        borderRadius: "var(--r-lg)",
        background: "var(--glass-solid)",
        border: "1px solid var(--line-2)",
        boxShadow: "0 16px 40px -12px rgba(40,40,90,.35), inset 0 0 0 1px rgba(255,255,255,.5)",
        overflow: "hidden",
        animation: "aurora-fade-in .16s ease both",
      }}
    >
      <MenuItem glyph="📎" label="Attach files" onClick={pickFiles} />
      <MenuItem glyph="🌐" label="Capture page" hint="⌥S" onClick={capturePage} />
      <MenuItem glyph="🖥" label="Take a screenshot" onClick={screenshot} />
    </div>
  );
}

function MenuItem({
  glyph,
  label,
  hint,
  onClick,
}: {
  glyph: string;
  label: string;
  hint?: string;
  onClick: () => void;
}) {
  return (
    <button
      role="menuitem"
      onClick={onClick}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        width: "100%",
        border: "none",
        background: "transparent",
        cursor: "pointer",
        padding: "9px 12px",
        fontSize: 12.5,
        color: "var(--ink)",
        textAlign: "left",
      }}
      onMouseEnter={(e) => (e.currentTarget.style.background = "var(--tint-wash)")}
      onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
    >
      <span aria-hidden style={{ fontSize: 14, width: 18, textAlign: "center" }}>
        {glyph}
      </span>
      <span style={{ flex: 1 }}>{label}</span>
      {hint && (
        <span style={{ fontSize: 10, color: "var(--ink-4)", fontFamily: "var(--mono)" }}>{hint}</span>
      )}
    </button>
  );
}
