import { useId } from "react";

export function BallFace({ size, eyes = "M21 31q4 -5 8 0M35 31q4 -5 8 0", mouth = "M24 41q8 8 16 0", className = "", panelVisible }: {
  size: number; eyes?: string; mouth?: string; className?: string; panelVisible?: boolean;
}) {
  const body = useId(), gloss = useId();
  return <span aria-hidden className={`block rounded-full shadow-ball ${className}`} style={{
    width: size, height: size, padding: Math.max(3, Math.round(size * 0.08)),
    backgroundImage: "linear-gradient(to bottom, var(--color-surface), color-mix(in srgb, var(--color-surface) 90%, var(--color-line)))",
    boxShadow: panelVisible ? "var(--shadow-ball), inset 0 0 0 3px var(--color-accent)" : undefined,
  }}>
    <svg viewBox="5.5 7.5 53 53" className="block h-full w-full" aria-hidden>
      <defs>
        <radialGradient id={body} cx="36%" cy="28%" r="80%">
          <stop offset="0" style={{ stopColor: "color-mix(in srgb, var(--color-brand) 65%, white)" }} />
          <stop offset="0.5" style={{ stopColor: "var(--color-brand)" }} />
          <stop offset="1" style={{ stopColor: "color-mix(in srgb, var(--color-brand) 80%, #d97706)" }} />
        </radialGradient>
        <linearGradient id={gloss} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#fff" stopOpacity="0.55" />
          <stop offset="1" stopColor="#fff" stopOpacity="0" />
        </linearGradient>
      </defs>
      <circle cx="32" cy="34" r="24" fill={`url(#${body})`} stroke="var(--color-brand-ink)" strokeWidth="2.5" />
      <ellipse cx="32" cy="20.5" rx="15" ry="7.5" fill={`url(#${gloss})`} />
      <path d={eyes} fill="none" stroke="var(--color-brand-ink)" strokeWidth="3" strokeLinecap="round" />
      <path d={mouth} fill="none" stroke="var(--color-brand-ink)" strokeWidth="3" strokeLinecap="round" />
    </svg>
  </span>;
}
