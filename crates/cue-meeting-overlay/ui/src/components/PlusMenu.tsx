// The "+" context menu — opens from the composer's + button. Real daemon
// actions only; nothing here is a stub. Holds BOTH the context actions (attach /
// capture / screenshot) AND the audio controls (listen with mic/system, or pick
// a specific app to capture) — so audio lives in the quick "+" dialog rather
// than a separate tab.

import { useEffect, useRef, useState } from "react";
import { getClient } from "../lib";
import type { ListeningState } from "../lib/types";
import {
  AlertIcon,
  AttachIcon,
  GlobeIcon,
  ScreenIcon,
  StopIcon,
  SystemAudioIcon,
} from "./icons";

export function PlusMenu({ onClose }: { onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const client = getClient();
  const [listen, setListen] = useState<ListeningState>("idle");

  // Live listening state so the menu shows Start vs Stop + permission hints.
  useEffect(() => client.onListeningState(setListen), [client]);

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

  const active = listen === "listening" || listen === "connecting";
  const denied = listen === "permission_denied";

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
      {/* Audio — the listen controls live here now (no separate tab) */}
      <MenuLabel>AUDIO</MenuLabel>
      {/* One action: Listen / Stop. v1 captures SYSTEM audio (the other people
          in the call — the question trigger). No "pick app" — clicking Listen
          just works (whole-meeting audio). */}
      {active ? (
        <MenuItem
          icon={<StopIcon size={15} />}
          label="Stop listening"
          onClick={act(() => client.stopListening())}
        />
      ) : (
        <MenuItem
          icon={<SystemAudioIcon size={15} />}
          label="Listen"
          onClick={act(() => client.startListening({ microphone: false, system: true }))}
        />
      )}
      {denied && (
        <MenuItem
          icon={<AlertIcon size={15} />}
          label="Grant Screen Recording…"
          tone="warn"
          onClick={act(() => client.openPermissionSettings("screen_recording"))}
        />
      )}

      <Divider />

      {/* Context — the original "+" actions */}
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

function Divider() {
  return <div style={{ height: 1, background: "var(--line)", margin: "5px 0" }} />;
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
      onMouseEnter={(e) => (e.currentTarget.style.background = "var(--tint-wash)")}
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
        <span style={{ fontSize: 10, color: "var(--ink-4)", fontFamily: "var(--mono)" }}>
          {hint}
        </span>
      )}
    </button>
  );
}
