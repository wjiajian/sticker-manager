export function ProgressBar({ label, value, max, valueText, className = "" }: {
  label: string; value?: number; max: number; valueText?: string; className?: string;
}) {
  const limit = Math.max(1, max);
  const current = value === undefined ? undefined : Math.max(0, Math.min(limit, value));
  return <div role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={limit} aria-valuenow={current}
    aria-valuetext={valueText} className={`h-1 overflow-hidden rounded-full bg-muted ${className}`}>
    <div className={`h-full origin-left rounded-full bg-accent ${current === undefined ? "progress-indeterminate" : "w-full motion-safe:interactive"}`}
      style={current === undefined ? undefined : { transform: `scaleX(${current / limit})` }} />
  </div>;
}
