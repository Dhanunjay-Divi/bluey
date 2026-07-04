// The "+" context menu — opens from the composer's + button. Real daemon
// actions only; nothing here is a stub. Holds the CONTEXT actions (attach /
// capture / screenshot). The listen control lives OUTSIDE the menu now — as the
// mic button directly on the composer row — so it is one click, not buried in a
// dialog.

import { useEffect, useRef } from "react";
import { getClient } from "../lib";
import { AttachIcon, GlobeIcon, ScreenIcon } from "./icons";

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

  const act = (fn: () => void) => () => {
    onClose();
    fn();
  };

  return (
    <div
      ref={ref}
      role="menu"
      style={{
        position: "absolute",
        left: 14,
        bottom: 60,
        width: 224,
        zIndex: 20,
        borderRadius: "var(--r-lg)",
        background: "var(--glass-solid)",
        border: "1px solid var(--line-2)",
        boxShadow:
          "0 16px 40px -12px rgba(40,40,90,.35), inset 0 0 0 1px rgba(255,255,255,.5)",
        overflow: "hidden",
        animation: "aurora-fade-in .16s ease both",
        padding: "5px 0",
      }}
    >
      {/* Context — the "+" actions. Listen moved out to the composer's mic. */}
      <MenuLabel>CONTEXT</MenuLabel>
      <MenuItem
        icon={<AttachIcon size={15} />}
        label="Attach files"
        onClick={act(() => client.openAttachPicker())}
      />
      <MenuItem
        icon={<GlobeIcon size={15} />}
        label="Capture page"
        hint="⌥S"
        onClick={act(() => client.capturePage())}
      />
      <MenuItem
        icon={<ScreenIcon size={15} />}
        label="Take a screenshot"
        onClick={act(() => client.captureScreenshot())}
      />
    </div>
  );
}

function MenuLabel({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        fontSize: 9.5,
        fontWeight: 680,
        letterSpacing: ".12em",
        color: "var(--ink-4)",
        padding: "5px 13px 3px",
      }}
    >
      {children}
    </div>
  );
}

function MenuItem({
  icon,
  label,
  hint,
  tone,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  hint?: string;
  tone?: "warn";
  onClick: () => void;
}) {
  const color = tone === "warn" ? "#b87503" : "var(--ink)";
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
        padding: "9px 13px",
        fontSize: 12.5,
        color,
        textAlign: "left",
      }}
      onMouseEnter={(e) =>
        (e.currentTarget.style.background = "var(--tint-wash)")
      }
      onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
    >
      <span
        aria-hidden
        style={{
          width: 18,
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          color: tone === "warn" ? "#b87503" : "var(--ink-3)",
        }}
      >
        {icon}
      </span>
      <span style={{ flex: 1 }}>{label}</span>
      {hint && (
        <span
          style={{
            fontSize: 10,
            color: "var(--ink-4)",
            fontFamily: "var(--mono)",
          }}
        >
          {hint}
        </span>
      )}
    </button>
  );
}
