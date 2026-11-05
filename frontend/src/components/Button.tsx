import type { ButtonHTMLAttributes } from "react";

const variants = {
  primary: "bg-ink text-surface",
  accent: "bg-accent-fill text-on-accent",
  secondary: "border border-line bg-surface text-ink",
};

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: keyof typeof variants;
}

export function Button({ variant = "secondary", className = "", ...props }: ButtonProps) {
  return (
    <button
      type="button"
      className={`interactive flex items-center gap-2 rounded-control px-3 py-2 font-semibold hover:opacity-82 active:scale-98 ${variants[variant]} ${className}`}
      {...props}
    />
  );
}
