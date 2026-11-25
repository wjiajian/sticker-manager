import { WarningCircleIcon } from "@phosphor-icons/react";
import { createContext, useContext, type ReactNode } from "react";
import { useDelayedLoading } from "../../components/Skeleton";
import { matchPreferences, SECTIONS, type ItemId, type SectionId } from "./preferencesSections";

export const PreferencesFilterContext = createContext(matchPreferences(""));

export function PrefSection({ prefix, id, error, children }: { prefix: string; id: SectionId; error?: string; children: ReactNode }) {
  const heading = `${prefix}-heading-${id}`;
  return <section id={`${prefix}-section-${id}`} data-section={id} aria-labelledby={heading} hidden={!useContext(PreferencesFilterContext).sections.has(id)}
    className="flex shrink-0 scroll-mt-6 flex-col gap-4">
    <h3 id={heading} tabIndex={-1} className="text-base font-semibold">{SECTIONS.find(section => section.id === id)?.label}</h3>
    {children}
    <InlineError message={error} />
  </section>;
}

export function SettingRow({ item, label, description, descriptionId, children, dim, stacked }: {
  item: ItemId; label?: ReactNode; description?: string; descriptionId?: string; children: ReactNode; dim?: boolean; stacked?: boolean;
}) {
  const visible = useContext(PreferencesFilterContext).items.has(item);
  return <div data-item={item} hidden={!visible}
    className={`flex min-h-9 ${stacked ? "flex-col gap-3" : "items-center justify-between gap-4"}`}>
    {label && <div className={dim ? "text-ink-secondary" : ""}>{label}
      {description && <p id={descriptionId} className="mt-1 text-xs text-ink-secondary">{description}</p>}
    </div>}
    {children}
  </div>;
}

export function InlineError({ id, message }: { id?: string; message?: string | null }) {
  return message ? <p id={id} role="alert" className="flex select-text items-start gap-2 break-all text-xs text-accent">
    <WarningCircleIcon size={16} aria-hidden className="shrink-0" />{message}
  </p> : null;
}

const shapes = {
  segmented: "h-8 w-48 rounded-control", toggle: "h-6 w-10 rounded-full", slider: "h-1 flex-1 rounded-full",
  preview: "size-24 rounded-card", version: "h-5 w-28 rounded-control",
};
export function ControlSkeleton({ shape }: { shape: keyof typeof shapes }) {
  const show = useDelayedLoading(true);
  return <span aria-hidden className={`block ${shapes[shape]} ${show ? "skeleton-pulse bg-muted" : "invisible"}`} />;
}
