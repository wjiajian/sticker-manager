import { GearIcon, PlusIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { CategoryDot } from "../../components/CategoryDot";
import { ipc, type CategorySummary, type SyncStatus } from "../../lib/ipc";
import { useCategories, useLibraryMutation, useLibraryOverview } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";
import { LIBRARY_VIEWS } from "./libraryViews";
import { CategoryCreateDialog } from "./CategoryCreateDialog";
import { useCategoryMenu, type RenameTarget } from "./useCategoryMenu";
import { SyncCard } from "./SyncCard";
import { CategorySkeleton, useDelayedLoading } from "../../components/Skeleton";

const navigation = "interactive relative flex w-full min-w-0 items-center gap-2 rounded-control px-2 py-1 text-left hover:bg-hover active:scale-98";
const selectedNavigation = "bg-surface font-semibold windows:before:absolute windows:before:top-2 windows:before:left-0 windows:before:h-3 windows:before:w-0.75 windows:before:rounded-full windows:before:bg-accent";

export function Sidebar({ onPreferences, onSync, sync }: { onPreferences: () => void; onSync: (button: HTMLButtonElement) => void; sync?: SyncStatus }) {
  const { data: overview } = useLibraryOverview();
  const categories = useCategories();
  const { data: list, error, isPending } = categories;
  const skeleton = useDelayedLoading(isPending);
  const view = useUiStore((s) => s.view);
  const searching = useUiStore((s) => !!s.searchQuery);
  const setView = useUiStore((s) => s.setView);
  const [creating, setCreating] = useState(false);
  const [rename, setRename] = useState<RenameTarget | null>(null);
  const menus = useCategoryMenu(setRename);

  function finishRename(restoreFocus: boolean) {
    const target = rename;
    setRename(null);
    if (restoreFocus && target) requestAnimationFrame(() => document.getElementById(`sidebar-category-${target.id}`)?.focus());
  }

  function categoryRow(category: CategorySummary) {
    const selected = !searching && typeof view !== "string" && view.category === category.id;
    return rename?.id === category.id ? (
      <div key={category.id} className="px-2 py-1">
        <InlineName key={category.id} target={rename} onFinish={finishRename} />
      </div>
    ) : (
      <button
        key={category.id}
        id={`sidebar-category-${category.id}`}
        type="button"
        aria-current={selected ? "page" : undefined}
        aria-haspopup="menu"
        title={category.name}
        onClick={() => setView({ category: category.id })}
        {...menuEvents((x, y) => menus.categoryMenu(category, x, y))}
        className={`${navigation} ${selected ? selectedNavigation : ""}`}
      >
        <CategoryDot color={category.color} />
        <span className="min-w-0 flex-1 truncate">{category.name}</span>
        <span className="shrink-0 text-xs font-normal text-ink-secondary">{category.stickerCount}</span>
      </button>
    );
  }

  return (
    <nav aria-label="仓库导航" className="flex w-54 shrink-0 flex-col bg-chrome px-3 pt-2 pb-4">
      <div className="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto">
      <Section title="仓库">
        {LIBRARY_VIEWS.map(({ view: id, label, icon: Icon }) => {
          const selected = !searching && id === view;
          return (
            <button
              key={id}
              type="button"
              aria-current={selected ? "page" : undefined}
              onClick={() => setView(id)}
              className={`${navigation} ${selected ? selectedNavigation : "font-medium"}`}
            >
              <Icon size={16} aria-hidden />
              <span className="flex-1">{label}</span>
              <span className="text-xs font-normal text-ink-secondary">{overview?.[id]}</span>
            </button>
          );
        })}
      </Section>

      <section aria-label="分类" className="flex min-w-0 flex-col gap-0.5" aria-busy={isPending || menus.busy}>
        <div className="flex items-center justify-between px-2 py-1">
          <h2 className="text-xs font-semibold text-ink-secondary">分类</h2>
          <button
            type="button"
            aria-label="新建分类"
            onClick={() => setCreating(true)}
            className="interactive flex rounded-sm p-0.5 text-ink-secondary hover:opacity-82 active:scale-98"
          >
            <PlusIcon size={16} aria-hidden />
          </button>
        </div>
        {error ? <p role="alert" className="flex items-center gap-2 px-2 py-1 text-xs text-ink-secondary">读取分类失败<button type="button" disabled={categories.isFetching} onClick={() => { void categories.refetch(); }} className="rounded-sm underline underline-offset-4 hover:opacity-82 disabled:opacity-50">重试</button></p>
          : isPending ? skeleton ? <CategorySkeleton /> : null
          : <>
            {list.length === 0 && <p className="px-2 py-1 text-xs text-ink-secondary">还没有分类</p>}
            {list.map(categoryRow)}
          </>}
        {menus.error && <p role="alert" className="px-2 py-1 text-xs text-ink-secondary">{menus.error}</p>}
      </section>
      </div>
      <SyncCard status={sync} onClick={onSync} />
      <button type="button" onClick={onPreferences} className={`${navigation} mt-4 shrink-0 text-ink-tertiary`}><GearIcon size={16} aria-hidden />偏好设置</button>
      {creating && <CategoryCreateDialog onClose={() => setCreating(false)} />}
    </nav>
  );
}

function InlineName({ target, onFinish }: { target: RenameTarget; onFinish: (restoreFocus: boolean) => void }) {
  const [name, setName] = useState(target.name);
  const input = useRef<HTMLInputElement>(null);
  const working = useRef(false);
  const finished = useRef(false);
  const mutation = useLibraryMutation((value: string) => ipc.renameCategory(target.id, value));

  useEffect(() => { input.current?.focus(); input.current?.select(); }, []);

  async function save(restoreFocus: boolean) {
    if (working.current || finished.current) return;
    working.current = true;
    try {
      if (name.trim() !== target.name) await mutation.mutateAsync(name);
      finished.current = true;
      onFinish(restoreFocus);
    } catch {
      input.current?.focus();
    } finally {
      working.current = false;
    }
  }

  return (
    <div className="flex min-w-0 flex-col gap-1">
      <input
        ref={input}
        aria-label="分类名称"
        aria-invalid={!!mutation.error}
        value={name}
        readOnly={mutation.isPending}
        onChange={(event) => setName(event.target.value)}
        onBlur={() => { void save(false); }}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing || event.keyCode === 229) return;
          if (event.key === "Escape") {
            event.preventDefault();
            if (!working.current) { finished.current = true; onFinish(true); }
          } else if (event.key === "Enter") {
            event.preventDefault();
            void save(true);
          }
        }}
        className="w-full min-w-0 rounded-control border border-line bg-surface px-2 py-0.5"
      />
      {mutation.error && <p role="alert" className="text-xs text-ink-secondary">{String(mutation.error)}</p>}
    </div>
  );
}

function menuEvents(open: (x: number, y: number) => Promise<void>) {
  return {
    onContextMenu: (event: MouseEvent<HTMLElement>) => {
      event.preventDefault();
      event.stopPropagation();
      event.currentTarget.focus();
      void open(event.clientX, event.clientY);
    },
    onKeyDown: (event: KeyboardEvent<HTMLElement>) => {
      if (event.key !== "ContextMenu" && !(event.key === "F10" && event.shiftKey)) return;
      event.preventDefault();
      const rect = event.currentTarget.getBoundingClientRect();
      void open(rect.left, rect.bottom);
    },
  };
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-0.5">
      <h2 className="px-2 py-1 text-xs font-semibold text-ink-secondary">{title}</h2>
      {children}
    </section>
  );
}
