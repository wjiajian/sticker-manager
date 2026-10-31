import { CaretDownIcon, CaretRightIcon, PlusIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { TagDot } from "../../components/TagDot";
import { ipc, type TagGroup, type TagSummary } from "../../lib/ipc";
import { useLibraryMutation, useLibraryOverview, useTagSidebar } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";
import { LIBRARY_VIEWS } from "./libraryViews";
import { TagCreateDialog, type TagCreation } from "./TagCreateDialog";
import { useTagMenu, type RenameTarget } from "./useTagMenu";

const navigation = "interactive relative flex w-full min-w-0 items-center gap-2 rounded-control px-2 py-1 text-left hover:bg-hover active:scale-98";
const selectedNavigation = "bg-surface font-semibold windows:before:absolute windows:before:top-2 windows:before:left-0 windows:before:h-3 windows:before:w-0.75 windows:before:rounded-full windows:before:bg-accent";

export function Sidebar() {
  const { data: overview } = useLibraryOverview();
  const { data: sidebar, error, isPending } = useTagSidebar();
  const view = useUiStore((s) => s.view);
  const setView = useUiStore((s) => s.setView);
  const [collapsed, setCollapsed] = useState<Set<string | null>>(() => new Set());
  const [creation, setCreation] = useState<TagCreation | null>(null);
  const [rename, setRename] = useState<RenameTarget | null>(null);
  const menus = useTagMenu(sidebar?.groups ?? [], setCreation, setRename);

  function toggleGroup(id: string | null) {
    setCollapsed((previous) => {
      const next = new Set(previous);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function finishRename(restoreFocus: boolean) {
    const target = rename;
    setRename(null);
    if (restoreFocus && target) requestAnimationFrame(() => document.getElementById(`sidebar-${target.kind}-${target.id}`)?.focus());
  }

  function tagRow(tag: TagSummary) {
    const selected = typeof view !== "string" && view.tag === tag.id;
    return rename?.kind === "tag" && rename.id === tag.id ? (
      <div key={tag.id} className="py-1 pr-2 pl-6">
        <InlineName key={tag.id} target={rename} onFinish={finishRename} />
      </div>
    ) : (
      <button
        key={tag.id}
        id={`sidebar-tag-${tag.id}`}
        type="button"
        aria-current={selected ? "page" : undefined}
        aria-haspopup="menu"
        title={tag.name}
        onClick={() => setView({ tag: tag.id })}
        {...menuEvents((x, y) => menus.tagMenu(tag, x, y))}
        className={`${navigation} pl-6 ${selected ? selectedNavigation : ""}`}
      >
        <TagDot color={tag.color} />
        <span className="min-w-0 flex-1 truncate">{tag.name}</span>
        <span className="shrink-0 text-xs font-normal text-ink-secondary">{tag.stickerCount}</span>
      </button>
    );
  }

  function groupSection(group: TagGroup | null, tags: TagSummary[]) {
    const id = group?.id ?? null;
    const expanded = !collapsed.has(id);
    const name = group?.name ?? "未分组";
    const contentId = `tags-${id ?? "ungrouped"}`;
    return (
      <div key={id ?? "ungrouped"} className="flex min-w-0 flex-col gap-0.5">
        {group && rename?.kind === "group" && rename.id === group.id ? (
          <div className="px-2 py-1"><InlineName key={group.id} target={rename} onFinish={finishRename} /></div>
        ) : (
          <button
            id={`sidebar-group-${id ?? "ungrouped"}`}
            type="button"
            title={name}
            aria-expanded={expanded}
            aria-controls={contentId}
            aria-haspopup="menu"
            onClick={() => toggleGroup(id)}
            {...menuEvents((x, y) => menus.groupMenu(group, x, y))}
            className={`${navigation} gap-1 text-xs font-semibold text-ink-tertiary`}
          >
            {expanded ? <CaretDownIcon size={12} className="shrink-0" aria-hidden /> : <CaretRightIcon size={12} className="shrink-0" aria-hidden />}
            <span className="truncate">{name}</span>
          </button>
        )}
        <div id={contentId} hidden={!expanded} className="min-w-0">
          {tags.map(tagRow)}
        </div>
      </div>
    );
  }

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
              className={`${navigation} ${selected ? selectedNavigation : "font-medium"}`}
            >
              <Icon size={16} aria-hidden />
              <span className="flex-1">{label}</span>
              <span className="text-xs font-normal text-ink-secondary">{overview?.[id]}</span>
            </button>
          );
        })}
      </Section>

      <section aria-label="标签分组" className="flex min-w-0 flex-col gap-0.5" aria-busy={isPending || menus.busy}>
        <div className="flex items-center justify-between px-2 py-1">
          <h2 tabIndex={0} aria-haspopup="menu" {...menuEvents((x, y) => menus.groupMenu(null, x, y))} className="rounded-control text-xs font-semibold text-ink-secondary">标签分组</h2>
          <button
            type="button"
            aria-label="新建标签"
            onClick={() => setCreation({ kind: "tag", groupId: null })}
            className="interactive flex rounded-sm p-0.5 text-ink-secondary hover:opacity-82 active:scale-98"
          >
            <PlusIcon size={16} aria-hidden />
          </button>
        </div>
        {error ? <p role="alert" className="px-2 py-1 text-xs text-ink-secondary">{String(error)}</p>
          : isPending ? <p role="status" className="px-2 py-1 text-xs text-ink-secondary">正在读取标签…</p>
          : <>
            {sidebar.groups.length === 0 && sidebar.ungrouped.length === 0 && <p className="px-2 py-1 text-xs text-ink-secondary">还没有标签</p>}
            {sidebar.groups.map((group) => groupSection(group, group.tags))}
            {sidebar.ungrouped.length > 0 && groupSection(null, sidebar.ungrouped)}
          </>}
        {menus.error && <p role="alert" className="px-2 py-1 text-xs text-ink-secondary">{menus.error}</p>}
      </section>
      {creation && <TagCreateDialog
        creation={creation}
        groups={sidebar?.groups ?? []}
        onClose={() => setCreation(null)}
        onCreated={(groupId) => setCollapsed((previous) => { const next = new Set(previous); next.delete(groupId); return next; })}
      />}
    </nav>
  );
}

function InlineName({ target, onFinish }: { target: RenameTarget; onFinish: (restoreFocus: boolean) => void }) {
  const [name, setName] = useState(target.name);
  const input = useRef<HTMLInputElement>(null);
  const working = useRef(false);
  const finished = useRef(false);
  const mutation = useLibraryMutation((value: string) => target.kind === "tag" ? ipc.renameTag(target.id, value) : ipc.renameTagGroup(target.id, value));

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
        aria-label={target.kind === "tag" ? "标签名称" : "标签分组名称"}
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
