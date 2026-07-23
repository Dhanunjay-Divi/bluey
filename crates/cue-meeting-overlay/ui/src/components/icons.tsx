// Crisp inline-SVG icon set in the Aurora design language: stroke="currentColor",
// strokeWidth 1.5, round caps/joins, sized ~16px by default. These replace the
// emoji glyphs (🎙 ⏸ …) that read as cheap on a premium glass surface — every
// icon inherits its color from the parent (so a token-driven color flows in via
// `color`/`currentColor`) and scales by a single `size` prop.

import type { SVGProps } from "react";

/** Shared frame for every icon — a 24-unit viewBox so the 1.5 stroke reads
 *  consistently at any rendered `size`. */
function Icon({
  size = 16,
  children,
  ...rest
}: {
  size?: number;
  children: React.ReactNode;
} & SVGProps<SVGSVGElement>) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...rest}
    >
      {children}
    </svg>
  );
}

/** Microphone — the "you" source / start-listening glyph. */
export function MicIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <rect x="9" y="3" width="6" height="11" rx="3" />
      <path d="M5.5 11a6.5 6.5 0 0 0 13 0" />
      <path d="M12 17.5V21" />
      <path d="M9 21h6" />
    </Icon>
  );
}

/** System audio — a speaker emitting waves; the "the call / others" source. */
export function SystemAudioIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M4 9.5h3l4.5-3.5v12L7 14.5H4z" />
      <path d="M16 9a4 4 0 0 1 0 6" />
      <path d="M18.5 6.5a8 8 0 0 1 0 11" />
    </Icon>
  );
}

/** Stop — a rounded square; the "stop listening" glyph. */
export function StopIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <rect x="6" y="6" width="12" height="12" rx="2.5" />
    </Icon>
  );
}

/** Pause — two rounded bars; an alternate "pause capture" glyph. */
export function PauseIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M9 5v14" />
      <path d="M15 5v14" />
    </Icon>
  );
}

/** A small spinner ring — the "connecting…" affordance (animate via CSS). */
export function SpinnerIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M12 3a9 9 0 1 0 9 9" />
    </Icon>
  );
}

/** A warning triangle — the "audio failed" affordance. */
export function AlertIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M12 4 2.8 20h18.4z" />
      <path d="M12 10v4" />
      <path d="M12 17.2v.1" />
    </Icon>
  );
}

/** Paperclip — "attach files". */
export function AttachIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M21 11.5l-8.5 8.5a5 5 0 0 1-7-7l8.5-8.5a3.3 3.3 0 0 1 4.7 4.7l-8.5 8.5a1.6 1.6 0 0 1-2.3-2.3l7.8-7.8" />
    </Icon>
  );
}

/** Globe — "capture page". */
export function GlobeIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18" />
      <path d="M12 3c2.5 2.5 3.8 5.7 3.8 9s-1.3 6.5-3.8 9c-2.5-2.5-3.8-5.7-3.8-9S9.5 5.5 12 3z" />
    </Icon>
  );
}

/** Eye — "hide / minimise to the pill" (the panel stays running, just unseen). */
export function EyeIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z" />
      <circle cx="12" cy="12" r="3" />
    </Icon>
  );
}

/** Chevron — expand/collapse affordance (rotate 180° for the open state). */
export function ChevronIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M6 9l6 6 6-6" />
    </Icon>
  );
}

/** Display — "take a screenshot". */
export function ScreenIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <rect x="3" y="4" width="18" height="12" rx="2" />
      <path d="M8 20h8" />
      <path d="M12 16v4" />
    </Icon>
  );
}

/** User — speaker identification and assignment glyph. */
export function UserIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
      <circle cx="12" cy="7" r="4" />
    </Icon>
  );
}

/** Merge — combine two speaker profiles. */
export function MergeIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M18 18L12 12L6 18" />
      <path d="M6 6L12 12L18 6" />
    </Icon>
  );
}

/** Check mark — selected state glyph. */
export function CheckIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M20 6L9 17l-5-5" />
    </Icon>
  );
}
