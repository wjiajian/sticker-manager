import { ArrowsClockwiseIcon, CloudCheckIcon, CloudSlashIcon, CloudWarningIcon } from "@phosphor-icons/react";
import type { SyncStatus } from "../../lib/ipc";

export function SyncIcon({ status }: { status?: SyncStatus }) {
  const Icon = status?.phase === "synced" ? CloudCheckIcon : status?.phase === "syncing" ? ArrowsClockwiseIcon : status?.phase === "error" ? CloudWarningIcon : CloudSlashIcon;
  return <Icon size={16} aria-hidden className={`shrink-0 ${status?.phase === "synced" ? "text-synced" : status?.phase === "error" ? "text-accent" : "text-ink-secondary"}`} />;
}
