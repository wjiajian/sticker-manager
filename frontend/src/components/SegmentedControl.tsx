import { useRef } from "react";

type Props<T extends string | number> = {
  label?: string; labelledBy?: string; value: T | undefined;
  options: readonly { value: T; label: string }[]; onChange: (value: T) => void;
  disabled?: boolean; className?: string;
};

export function SegmentedControl<T extends string | number>({ label, labelledBy, value, options, onChange, disabled, className = "flex" }: Props<T>) {
  const group = useRef<HTMLDivElement>(null);
  return <div ref={group} role="radiogroup" aria-label={label} aria-labelledby={labelledBy}
    className={`${className} gap-1 rounded-control bg-muted p-1`}>
    {options.map((option, index) => <button key={option.value} type="button" role="radio" disabled={disabled}
      aria-checked={option.value === value} tabIndex={option.value === value || (value === undefined && index === 0) ? 0 : -1}
      className="interactive min-w-0 whitespace-nowrap rounded-inset px-3 py-1 text-xs font-medium text-ink-secondary aria-checked:bg-surface aria-checked:font-semibold aria-checked:text-ink hover:opacity-82 active:scale-98 disabled:opacity-50"
      onClick={() => { if (option.value !== value) onChange(option.value); }}
      onKeyDown={event => {
        const count = options.length;
        let next: number;
        switch (event.key) {
          case "ArrowLeft": case "ArrowUp": next = (index + count - 1) % count; break;
          case "ArrowRight": case "ArrowDown": next = (index + 1) % count; break;
          case "Home": next = 0; break;
          case "End": next = count - 1; break;
          default: return;
        }
        event.preventDefault();
        group.current?.querySelectorAll("button")[next]?.focus();
        if (options[next].value !== value) onChange(options[next].value);
      }}>{option.label}</button>)}
  </div>;
}
