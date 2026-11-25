import { useState, type KeyboardEvent } from "react";
import type { UpdateStatus } from "../../lib/ipc";
import { navigation, selectedNavigation } from "./libraryViews";
import { SECTIONS, type SectionId } from "./preferencesSections";
import { syncTime } from "./useSyncStatus";

export type NavStatus = { text?: string | null; dot?: "bg-ink-secondary" | "bg-synced" | "bg-accent"; error?: boolean };
export const failure: NavStatus = { text: "出错", error: true };
export function percentage(value: number, total: number) {
  return `${Math.min(100, Math.floor(value * 100 / Math.max(1, total)))}%`;
}

export function PreferencesNav({ prefix, active, matched, statuses, onJump, update }: {
  prefix: string; active?: SectionId; matched: Set<SectionId>; statuses: Record<SectionId, NavStatus>;
  onJump: (id: SectionId) => void; update?: UpdateStatus;
}) {
  const [focused, setFocused] = useState<SectionId>();
  const tabStop = focused && matched.has(focused) ? focused : active;
  function navigate(event: KeyboardEvent<HTMLElement>) {
    const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>("button:enabled"));
    const index = buttons.indexOf(event.target as HTMLButtonElement);
    if (index < 0) return;
    let next: number;
    switch (event.key) {
      case "ArrowDown": next = (index + 1) % buttons.length; break;
      case "ArrowUp": next = (index + buttons.length - 1) % buttons.length; break;
      case "Home": next = 0; break;
      case "End": next = buttons.length - 1; break;
      default: return;
    }
    event.preventDefault();
    buttons[next].focus();
  }
  const summary = update?.phase === "checking" ? "正在检查…" : update?.error ? "检查更新出错"
    : update?.available ? `发现新版本 ${update.available.version}`
    : update?.lastChecked ? `已是最新 · ${syncTime(update.lastChecked)}` : "尚未检查";
  return <div className="flex min-h-0 flex-col bg-chrome px-3 pb-4">
    <nav aria-label="设置分区" onKeyDown={navigate}
      onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget)) setFocused(undefined); }} className="min-h-0 overflow-y-auto">
      <ul className="flex flex-col gap-0.5">
        {SECTIONS.map(({ id, label, icon: Icon }) => {
          const status = statuses[id];
          return <li key={id}>
            <button data-nav={id} type="button" aria-controls={`${prefix}-section-${id}`} aria-current={id === active ? "location" : undefined}
              disabled={!matched.has(id)} onFocus={() => setFocused(id)} tabIndex={id === tabStop ? 0 : -1} onClick={() => onJump(id)}
              className={`${navigation} disabled:opacity-50 ${active === id ? selectedNavigation : "font-medium"}`}>
              <Icon size={16} aria-hidden className="shrink-0" /><span className="min-w-0 flex-1 truncate">{label}</span>
              {status.text && <span title={status.text} className={`flex max-w-16 shrink-0 items-center gap-1 text-xs ${status.error ? "text-accent" : "text-ink-secondary"}`}>
                {status.dot && <span aria-hidden className={`size-2 shrink-0 rounded-full ${status.dot}`} />}
                <span className="truncate">{status.text}</span>
              </span>}
            </button>
          </li>;
        })}
      </ul>
    </nav>
    <div className="mt-auto px-2 pt-4 text-xs text-ink-secondary">{update && <p>版本 {update.currentVersion}</p>}<p>{summary}</p></div>
  </div>;
}
