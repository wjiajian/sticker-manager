import { PlusIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { useLibraryOverview } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";
import { LIBRARY_VIEWS } from "./libraryViews";

export function Sidebar() {
  const { data: overview } = useLibraryOverview();
  const view = useUiStore((s) => s.view);
  const setView = useUiStore((s) => s.setView);

  return (
    <nav aria-label="仓库导航" className="flex w-54 shrink-0 flex-col gap-6 overflow-y-auto bg-chrome px-3 pt-2 pb-4">
      <Section title="仓库">
        {LIBRARY_VIEWS.map(({ view: id, label, icon: Icon }) => {
          const selected = id === view;
          return (
            <button
              key={id}
              type="button"
              aria-current={selected ? "page" : undefined}
              onClick={() => setView(id)}
              className={`interactive relative flex w-full items-center gap-2 rounded-control px-2 py-1 text-left hover:bg-hover active:scale-98 ${
                selected
                  ? "bg-surface font-semibold windows:before:absolute windows:before:top-2 windows:before:left-0 windows:before:h-3 windows:before:w-0.75 windows:before:rounded-full windows:before:bg-accent"
                  : "font-medium"
              }`}
            >
              <Icon size={16} />
              <span className="flex-1">{label}</span>
              <span className="text-xs font-normal text-ink-secondary">{overview?.[id]}</span>
            </button>
          );
        })}
      </Section>

      <Section
        title="标签分组"
        action={
          <button
            type="button"
            aria-label="新建标签"
            className="interactive flex rounded-sm p-0.5 text-ink-secondary hover:opacity-82 active:scale-98"
          >
            <PlusIcon size={16} />
          </button>
        }
      >
        <p className="px-2 py-1 text-xs text-ink-secondary">还没有标签</p>
      </Section>
    </nav>
  );
}

function Section({ title, action, children }: { title: string; action?: ReactNode; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-0.5">
      <div className="flex items-center justify-between px-2 py-1">
        <h2 className="text-xs font-semibold text-ink-secondary">{title}</h2>
        {action}
      </div>
      {children}
    </section>
  );
}
