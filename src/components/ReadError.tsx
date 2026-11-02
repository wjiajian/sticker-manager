import { WarningCircleIcon } from "@phosphor-icons/react";
import { Button } from "./Button";

export function ReadError({ error, retry, busy, icon = false }: { error: unknown; retry: () => void; busy: boolean; icon?: boolean }) {
  return <div className="flex min-w-0 flex-1 flex-col items-center justify-center gap-3 text-center">
    {icon && <WarningCircleIcon size={32} className="text-accent" aria-hidden />}
    <h2 className="text-base font-semibold text-ink">读取表情失败</h2>
    <p role="alert" className="max-h-32 w-full overflow-y-auto text-xs break-words whitespace-pre-wrap text-ink-secondary select-text">{String(error)}</p>
    <Button disabled={busy} onClick={retry} className="disabled:opacity-50">重试</Button>
  </div>;
}
