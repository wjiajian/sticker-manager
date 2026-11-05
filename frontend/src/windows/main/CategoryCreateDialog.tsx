import { useEffect, useId, useRef, useState } from "react";
import { Button } from "../../components/Button";
import { CategoryDot } from "../../components/CategoryDot";
import { ipc } from "../../lib/ipc";
import { useLibraryMutation } from "../../lib/queries";
import { CATEGORY_COLORS } from "../../lib/categoryColors";

export function CategoryCreateDialog({ onClose }: { onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const titleId = useId();
  const [name, setName] = useState("");
  const [color, setColor] = useState<string | null>(null);
  const working = useRef(false);
  const mutation = useLibraryMutation(() => ipc.createCategory(name, color));

  useEffect(() => {
    const element = dialog.current!;
    element.showModal();
    nameInput.current?.focus();
    return () => element.close();
  }, []);

  async function save() {
    if (working.current) return;
    working.current = true;
    try {
      await mutation.mutateAsync();
      onClose();
    } catch {
      nameInput.current?.focus();
    } finally {
      working.current = false;
    }
  }

  return (
    <dialog
      ref={dialog}
      aria-labelledby={titleId}
      onCancel={(event) => { event.preventDefault(); if (!working.current) onClose(); }}
      className="m-auto w-80 max-w-full rounded-card border border-line bg-surface p-6 text-ink backdrop:bg-overlay"
    >
      <form
        className="flex flex-col gap-4"
        aria-busy={mutation.isPending}
        onSubmit={(event) => { event.preventDefault(); void save(); }}
        onKeyDown={(event) => {
          if (event.key === "Enter" && (event.nativeEvent.isComposing || event.keyCode === 229)) event.preventDefault();
        }}
      >
        <h2 id={titleId} className="text-lg font-semibold">新建分类</h2>
        <label className="flex flex-col gap-2">
          <span className="text-xs font-semibold text-ink-secondary">名称</span>
          <input ref={nameInput} value={name} readOnly={mutation.isPending} onChange={(event) => setName(event.target.value)} className="min-w-0 rounded-control border border-line bg-surface px-3 py-2" />
        </label>
        <fieldset disabled={mutation.isPending}>
          <legend className="mb-2 text-xs font-semibold text-ink-secondary">颜色</legend>
          <div className="flex flex-wrap gap-2">
            {[{ value: null, name: "自动" }, ...CATEGORY_COLORS].map((item) => (
              <label key={item.value ?? "auto"} className="interactive flex items-center gap-1 rounded-control border border-line px-2 py-1 text-xs hover:bg-hover">
                <input type="radio" name="color" value={item.value ?? ""} checked={color === item.value} onChange={() => setColor(item.value)} className="accent-accent" />
                {item.value && <CategoryDot color={item.value} />}
                {item.name}
              </label>
            ))}
          </div>
        </fieldset>
        {mutation.error && <p role="alert" className="text-xs text-ink-secondary">{String(mutation.error)}</p>}
        <div className="flex justify-end gap-2">
          <Button disabled={mutation.isPending} onClick={onClose}>取消</Button>
          <Button type="submit" variant="primary" disabled={mutation.isPending}>{mutation.isPending ? "正在新建…" : "新建"}</Button>
        </div>
      </form>
    </dialog>
  );
}
