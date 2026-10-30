import { useLibraryOverview } from "../../lib/queries";
import { useUiStore } from "../../stores/ui";
import { LIBRARY_VIEWS } from "./libraryViews";

export function StickerGrid() {
  const view = useUiStore((s) => s.view);
  const { data: overview, error } = useLibraryOverview();
  const label = LIBRARY_VIEWS.find((v) => v.view === view)?.label;
  const count = overview?.[view];

  return (
    <main className="flex min-w-0 flex-1 flex-col gap-6 overflow-y-auto px-8 py-6">
      <div className="flex items-baseline gap-3">
        <h1 className="text-2xl font-semibold">{label}</h1>
        {count !== undefined && <span className="text-ink-secondary">{count} 张</span>}
      </div>

      {error ? (
        <p className="text-ink-secondary">读取仓库失败：{String(error)}</p>
      ) : (
        count === 0 && (
          <div className="flex flex-1 flex-col items-center justify-center gap-1 pb-16 text-center">
            <p className="text-base font-semibold">这里还没有表情</p>
            <p className="text-ink-secondary">导入后会出现在这里</p>
          </div>
        )
      )}
    </main>
  );
}
