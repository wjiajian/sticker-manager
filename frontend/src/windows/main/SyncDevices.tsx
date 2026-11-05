import { DesktopIcon, LaptopIcon } from "@phosphor-icons/react";
import type { SyncStatus } from "../../lib/ipc";
import { platform } from "../../lib/platform";
import { deviceTime } from "./useSyncStatus";

export function SyncDevices({ status }: { status: SyncStatus }) {
  const devices = [{ id: status.deviceId, name: status.deviceName, platform, lastSyncAt: status.lastSyncAt }, ...status.devices.filter((d) => d.id !== status.deviceId).sort((a, b) => b.lastSyncAt - a.lastSyncAt)];
  return <ul aria-label="同步设备" className="flex min-w-0 flex-col gap-3">
    {devices.map((device) => {
      const Icon = device.platform === "macos" ? LaptopIcon : DesktopIcon;
      const name = `${device.name}${device.id === status.deviceId ? " · 本机" : ""}`;
      return <li key={device.id} className="flex min-w-0 items-center gap-2 text-xs">
        <Icon size={16} aria-hidden className="shrink-0 text-ink-secondary" />
        <div className="min-w-0"><p className="truncate" title={name}>{name}</p><p className="text-ink-secondary">{deviceTime(device.lastSyncAt)}</p></div>
      </li>;
    })}
  </ul>;
}
