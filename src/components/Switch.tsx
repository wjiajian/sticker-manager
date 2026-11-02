import type { ButtonHTMLAttributes } from "react";

export function Switch({ checked, ...props }: Omit<ButtonHTMLAttributes<HTMLButtonElement>, "role" | "aria-checked"> & { checked: boolean }) {
  return <button {...props} type="button" role="switch" aria-checked={checked}
    className={`interactive relative h-6 w-10 shrink-0 rounded-full disabled:cursor-default disabled:opacity-50 ${checked ? "bg-accent-fill ring-1 ring-accent" : "bg-switch-off"}`}>
    <span aria-hidden className={`interactive absolute top-1 left-1 h-4 w-4 rounded-full bg-on-accent ${checked ? "translate-x-4" : ""}`} />
  </button>;
}
