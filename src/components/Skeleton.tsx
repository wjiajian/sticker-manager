import { useEffect, useRef, useState, type ReactNode } from "react";

export function useDelayedLoading(loading: boolean, identity: unknown = loading) {
  const [ready, setReady] = useState<{ identity: unknown } | null>(null);
  useEffect(() => {
    setReady(null);
    if (!loading) return;
    const timer = setTimeout(() => setReady({ identity }), 200);
    return () => clearTimeout(timer);
  }, [loading, identity]);
  return loading && ready?.identity === identity;
}

export function Skeleton({ label, className = "", children }: { label: string; className?: string; children: ReactNode }) {
  return <div aria-busy="true" className={className}>
    <span className="sr-only" role="status">{label}</span>
    <div aria-hidden="true" className="skeleton-pulse h-full">{children}</div>
  </div>;
}

export function GridSkeleton({ panel = false }: { panel?: boolean }) {
  const container = useRef<HTMLDivElement>(null);
  const [count, setCount] = useState(0);
  useEffect(() => {
    if (panel || !container.current) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      const columns = Math.max(1, Math.floor((width + 12) / 116));
      const tile = (width - (columns - 1) * 12) / columns;
      setCount(columns * Math.ceil((height + 12) / (tile + 12)));
    });
    observer.observe(container.current);
    return () => observer.disconnect();
  }, [panel]);
  return <div ref={container} className={panel ? "h-41" : "min-h-0 flex-1 overflow-hidden"}>
    <Skeleton label="正在读取表情" className="h-full">
      <div className={panel ? "grid grid-cols-4 gap-2" : "grid grid-cols-[repeat(auto-fill,minmax(104px,1fr))] gap-3"}>
        {Array.from({ length: panel ? 8 : count }, (_, index) => <div key={index} className={`aspect-square bg-muted ${panel ? "rounded-lg" : "rounded-card"}`} />)}
      </div>
    </Skeleton>
  </div>;
}

export function CategorySkeleton({ panel = false }: { panel?: boolean }) {
  return <Skeleton label="正在读取分类" className={panel ? "h-8 shrink-0" : "px-2 py-1"}>
    <div className={panel ? "flex gap-2" : "flex flex-col gap-3"}>
      {[0, 1, 2].map((index) => panel ? <div key={index} className="h-8 w-16 rounded-full bg-muted" />
        : <div key={index} className="flex h-4 items-center gap-2"><div className="size-2 rounded-full bg-muted" /><div className="h-3 w-24 rounded-sm bg-muted" /></div>)}
    </div>
  </Skeleton>;
}

export function DetailSkeleton() {
  return <Skeleton label="正在读取表情详情" className="w-full">
    <div className="flex flex-col gap-6"><div className="aspect-square rounded-card bg-muted" /><div className="h-4 w-3/4 rounded-sm bg-muted" /><div className="h-3 w-1/2 rounded-sm bg-muted" /></div>
  </Skeleton>;
}
