import { CheckIcon, CopyIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";

export function PathSlot({ value, label, copyLabel, onError }: {
  value: string; label: string; copyLabel: string; onError: (message: string | null) => void;
}) {
  const path = useRef<HTMLSpanElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [copied, setCopied] = useState(false);
  useEffect(() => () => clearTimeout(timer.current), []);
  async function copy() {
    try {
      await navigator.clipboard.writeText(value);
    } catch {
      if (!path.current) return;
      const range = document.createRange();
      range.selectNodeContents(path.current);
      const selection = window.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
      try {
        if (!document.execCommand("copy")) throw Error();
      } catch {
        onError("无法复制路径，请选中后手动复制");
        return;
      }
    }
    if (!path.current) return;
    onError(null);
    clearTimeout(timer.current);
    setCopied(true);
    timer.current = setTimeout(() => setCopied(false), 1500);
  }
  const split = Math.max(0, value.length - 28);
  const Icon = copied ? CheckIcon : CopyIcon;
  return <div role="group" aria-label={label} className="flex min-w-0 items-center rounded-control bg-muted p-1">
    <span ref={path} title={value} tabIndex={0} className="flex min-w-0 flex-1 select-text px-2 py-1 font-mono text-xs"
      onCopy={event => { event.clipboardData.setData("text/plain", value); event.preventDefault(); }}>
      <span className="min-w-0 truncate">{value.slice(0, split)}</span><span className="shrink-0">{value.slice(split)}</span>
    </span>
    <button type="button" aria-label={copyLabel} title={copyLabel} onClick={() => void copy()}
      className="interactive shrink-0 rounded-inset p-1 hover:bg-hover active:scale-98"><Icon size={16} aria-hidden /></button>
    <span className="sr-only" role="status">{copied ? "已复制路径" : ""}</span>
  </div>;
}
