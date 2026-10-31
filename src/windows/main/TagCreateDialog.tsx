import { useEffect, useId, useRef, useState } from "react";
import { Button } from "../../components/Button";
import { TagDot } from "../../components/TagDot";
import { ipc, type TagGroup } from "../../lib/ipc";
import { useLibraryMutation } from "../../lib/queries";
import { TAG_COLORS } from "../../lib/tagColors";

export type TagCreation = { kind: "tag"; groupId: string | null } | { kind: "group" };

export function TagCreateDialog({ creation, groups, onClose, onCreated }: {
  creation: TagCreation;
  groups: TagGroup[];
  onClose: () => void;
  onCreated: (groupId: string | null) => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const titleId = useId();
  const [name, setName] = useState("");
  const [color, setColor] = useState<string | null>(null);
  const [groupId, setGroupId] = useState(creation.kind === "tag" ? creation.groupId : null);
  const working = useRef(false);
  const mutation = useLibraryMutation(async () => {
    if (creation.kind === "tag") {
      await ipc.createTag(name, color, groupId);
      return groupId;
    }
    return (await ipc.createTagGroup(name)).id;
  });
  const title = creation.kind === "tag" ? "新建标签" : "新建标签分组";

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
      const createdGroupId = await mutation.mutateAsync();
      onCreated(createdGroupId);
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
      className="m-auto w-80 max-w-full rounded-card border border-line bg-surface p-6 text-ink backdrop:bg-ink/20"
    >
      <form
        className="flex flex-col gap-4"
        aria-busy={mutation.isPending}
        onSubmit={(event) => { event.preventDefault(); void save(); }}
        onKeyDown={(event) => {
          if (event.key === "Enter" && (event.nativeEvent.isComposing || event.keyCode === 229)) event.preventDefault();
        }}
      >
        <h2 id={titleId} className="text-lg font-semibold">{title}</h2>
        <label className="flex flex-col gap-2">
          <span className="text-xs font-semibold text-ink-secondary">名称</span>
          <input ref={nameInput} value={name} readOnly={mutation.isPending} onChange={(event) => setName(event.target.value)} className="min-w-0 rounded-control border border-line bg-surface px-3 py-2" />
        </label>
        {creation.kind === "tag" && <>
          <label className="flex flex-col gap-2">
            <span className="text-xs font-semibold text-ink-secondary">标签分组</span>
            <select value={groupId ?? ""} disabled={mutation.isPending} onChange={(event) => setGroupId(event.target.value || null)} className="min-w-0 rounded-control border border-line bg-surface px-3 py-2">
              <option value="">未分组</option>
              {groups.map((group) => <option key={group.id} value={group.id}>{group.name}</option>)}
            </select>
          </label>
          <fieldset disabled={mutation.isPending}>
            <legend className="mb-2 text-xs font-semibold text-ink-secondary">颜色</legend>
            <div className="flex flex-wrap gap-2">
              {[{ value: null, name: "自动" }, ...TAG_COLORS].map((item) => (
                <label key={item.value ?? "auto"} className="interactive flex items-center gap-1 rounded-control border border-line px-2 py-1 text-xs hover:bg-hover">
                  <input type="radio" name="color" value={item.value ?? ""} checked={color === item.value} onChange={() => setColor(item.value)} className="accent-accent" />
                  {item.value && <TagDot color={item.value} />}
                  {item.name}
                </label>
              ))}
            </div>
          </fieldset>
        </>}
        {mutation.error && <p role="alert" className="text-xs text-ink-secondary">{String(mutation.error)}</p>}
        <div className="flex justify-end gap-2">
          <Button disabled={mutation.isPending} onClick={onClose}>取消</Button>
          <Button type="submit" variant="primary" disabled={mutation.isPending}>{mutation.isPending ? "正在新建…" : "新建"}</Button>
        </div>
      </form>
    </dialog>
  );
}
