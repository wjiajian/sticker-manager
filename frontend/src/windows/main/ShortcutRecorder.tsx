import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ipc, type ShortcutCandidate } from "../../lib/ipc";
import { mergePanelShortcut, usePanelShortcut } from "../../lib/panelShortcut";
import { platform } from "../../lib/platform";

// 跨卸载/重新打开也串行：旧会话的 end 必须先于新会话的 begin。
let queue: Promise<unknown> = Promise.resolve();
function enqueue<T>(operation: () => Promise<T>): Promise<T> {
  const task = queue.then(operation);
  queue = task.catch(() => {});
  return task;
}
type Phase = "idle" | "starting" | "recording" | "saving-recording" | "saving";
const MODIFIER_CODES = new Set(["ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight", "OSLeft", "OSRight", "Fn", "FnLock"]);
const MODIFIER_KEYS = new Set(["Control", "Alt", "AltGraph", "Shift", "Meta", "OS", "Fn"]);
const link = "text-xs text-ink-secondary underline underline-offset-4 hover:text-ink disabled:opacity-50";
export function ShortcutRecorder({ active, onRecordingChange }: { active: boolean; onRecordingChange: (recording: boolean) => void }) {
  const client = useQueryClient();
  const query = usePanelShortcut(); const state = query.data;
  const button = useRef<HTMLButtonElement>(null);
  const id = useId();
  const [phase, setPhase] = useState<Phase>("idle"); const phaseRef = useRef<Phase>("idle");
  const session = useRef<{ id: number; revision: number } | null>(null);
  const generation = useRef(0);
  const pending = useRef<Promise<ShortcutCandidate> | null>(null);
  const checkSeq = useRef(0);
  // 跟踪主键是否仍处于按下态；macOS 按住 ⌘ 时不会派发主键的 keyup，因此标记 withMeta 免于永久等待。
  const heldMainKey = useRef<{ code: string; withMeta: boolean } | null>(null);
  const [candidate, setCandidate] = useState<ShortcutCandidate | null>(null);
  const [held, setHeld] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const mounted = useRef(true); const cancelRequested = useRef(false);
  const ignoreClickUntil = useRef(0);
  const notify = useRef(onRecordingChange); notify.current = onRecordingChange;
  function announce(message: string) {
    if (!mounted.current) return;
    setAnnouncement((previous) => !message ? "" : previous === message ? `${message}\u200B` : message);
  }
  function restoreFocus(focusButton: boolean) {
    if (!focusButton) return;
    button.current?.focus();
    requestAnimationFrame(() => {
      if (mounted.current) button.current?.focus();
    });
  }
  function transition(next: Phase) {
    phaseRef.current = next;
    if (mounted.current) { setPhase(next); notify.current(next === "starting" || next === "recording" || next === "saving-recording"); }
  }
  function reset(message = "已退出录制", focusButton = false) {
    generation.current += 1;
    session.current = null; pending.current = null; heldMainKey.current = null; checkSeq.current += 1; cancelRequested.current = false;
    transition("idle");
    if (mounted.current) {
      setHeld(""); setCandidate(null); setError(null);
      if (message) announce(message);
      restoreFocus(focusButton);
    }
  }
  async function end(focusButton = false) {
    const current = session.current;
    if (current) mergePanelShortcut(client, await ipc.endPanelShortcutRecording(current.id));
    reset("已退出录制", focusButton);
  }
  function cancel(focusButton = false) {
    if (phaseRef.current === "idle" || phaseRef.current === "saving") return;
    cancelRequested.current = true; pending.current = null;
    if (phaseRef.current === "recording") {
      transition("saving-recording");
      void enqueue(() => end(focusButton)).catch((reason: unknown) => { reset("", focusButton); if (mounted.current) setError(String(reason)); });
    }
  }
  function begin() {
    button.current?.focus();
    if (!state || phaseRef.current !== "idle" || performance.now() < ignoreClickUntil.current) return;
    const gen = ++generation.current;
    cancelRequested.current = false; setError(null); setCandidate(null); setHeld(""); transition("starting");
    void enqueue(async () => {
      try {
        const next = await ipc.beginPanelShortcutRecording(); mergePanelShortcut(client, next);
        if (gen !== generation.current) {
          if (next.recordingId !== null) mergePanelShortcut(client, await ipc.endPanelShortcutRecording(next.recordingId));
          return;
        }
        session.current = next.recordingId === null ? null : { id: next.recordingId, revision: next.revision };
        if (cancelRequested.current || !mounted.current) await end();
        else if (phaseRef.current === "starting") transition("recording");
      } catch (reason) { reset(""); if (mounted.current) setError(String(reason)); }
    });
  }
  function save(shortcut: string | null, formatAnnouncement?: (accessibleLabel: string | null) => string, checked?: Promise<ShortcutCandidate>, focusButton = false) {
    const fromRecording = checked !== undefined || session.current !== null || phaseRef.current === "starting" || phaseRef.current === "recording";
    const expectedGen = fromRecording ? generation.current : ++generation.current;
    transition(fromRecording ? "saving-recording" : "saving");
    void enqueue(async () => {
      try {
        if (expectedGen !== generation.current) return;
        if (checked) {
          const value = await checked;
          if (expectedGen !== generation.current) return;
          if (cancelRequested.current || !mounted.current) { await end(); return; }
          if (value.error || !value.shortcut) {
            if (mounted.current) { setCandidate(value); setError(value.error); }
            transition(session.current ? "recording" : "idle");
            return;
          }
          shortcut = value.shortcut;
        }
        const next = await ipc.setPanelShortcut(shortcut); mergePanelShortcut(client, next);
        ignoreClickUntil.current = performance.now() + 400;
        const message = formatAnnouncement
          ? formatAnnouncement(next.accessibleLabel)
          : next.shortcut ? `呼出快捷键已改为 ${next.accessibleLabel}` : "已停用呼出快捷键";
        reset(message, focusButton);
      } catch (reason) {
        if (expectedGen !== generation.current) return;
        if (mounted.current) setError(String(reason));
        if (cancelRequested.current || !mounted.current) await end();
        else transition(session.current ? "recording" : "idle");
      }
    }).catch((reason: unknown) => { reset("", focusButton); if (mounted.current) setError(String(reason)); });
  }
  function heldLabel(event: KeyboardEvent<HTMLButtonElement>) {
    const names = platform === "macos" ? ["⌃", "⌥", "⇧", "⌘"] : ["Ctrl", "Alt", "Shift", "Win"];
    const parts = [event.ctrlKey, event.altKey, event.shiftKey, event.metaKey].flatMap((yes, i) => yes ? [names[i]] : []);
    return parts.length ? parts.join(platform === "macos" ? "" : "+") + (platform === "macos" ? "…" : "+…") : "";
  }
  function keyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (phaseRef.current === "idle") {
      if (event.repeat && (event.key === "Enter" || event.key === " ")) {
        event.preventDefault();
        ignoreClickUntil.current = Math.max(ignoreClickUntil.current, performance.now() + 400);
      }
      return;
    }
    if (phaseRef.current === "saving") return;
    if (event.key === "Tab") { cancel(); return; }
    event.preventDefault(); event.stopPropagation();
    if (event.key === "Escape") { cancel(true); return; }
    if (phaseRef.current !== "recording" && phaseRef.current !== "starting") return;
    if (event.nativeEvent.isComposing || event.keyCode === 229) {
      pending.current = null; heldMainKey.current = null; setCandidate(null); setError("请先切换到英文输入状态再录制"); return;
    }
    if (event.repeat) return;
    setHeld(heldLabel(event));
    if (MODIFIER_KEYS.has(event.key) || MODIFIER_CODES.has(event.code)) { checkSeq.current += 1; pending.current = null; heldMainKey.current = null; setCandidate(null); setError(null); return; }
    heldMainKey.current = { code: event.code, withMeta: platform === "macos" && event.metaKey };
    const input = { code: event.code, keyCode: event.keyCode, ctrl: event.ctrlKey, alt: event.altKey, shift: event.shiftKey, meta: event.metaKey };
    const seq = ++checkSeq.current;
    const task = enqueue(() => ipc.checkPanelShortcut(input)); pending.current = task;
    void task.then((value) => {
      if (mounted.current && checkSeq.current === seq && phaseRef.current !== "idle") {
        setCandidate(value); setError(value.error);
        if (!value.error && value.accessibleLabel) announce(`${value.accessibleLabel}，松开按键后保存`);
      }
    }).catch((reason: unknown) => {
      if (mounted.current && checkSeq.current === seq && phaseRef.current !== "idle") { pending.current = null; setError(String(reason)); }
    });
  }
  function keyUp(event: KeyboardEvent<HTMLButtonElement>) {
    if (heldMainKey.current?.code === event.code) heldMainKey.current = null;
    if (phaseRef.current !== "recording" && phaseRef.current !== "starting") return;
    if (event.key !== "Tab") { event.preventDefault(); event.stopPropagation(); }
    setHeld(heldLabel(event));
    const mainKeyReleased = !heldMainKey.current || heldMainKey.current.withMeta;
    if (!event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey && mainKeyReleased && pending.current) {
      const checked = pending.current;
      pending.current = null;
      save(null, undefined, checked);
    }
  }
  useEffect(() => {
    mounted.current = true;
    const hide = () => { if (document.visibilityState === "hidden") cancel(); };
    document.addEventListener("visibilitychange", hide);
    return () => {
      mounted.current = false; document.removeEventListener("visibilitychange", hide);
      if (phaseRef.current === "starting" || phaseRef.current === "recording" || phaseRef.current === "saving-recording") {
        cancelRequested.current = true; void enqueue(() => end()).catch(() => {});
      }
      notify.current(false);
    };
  }, []);
  useEffect(() => { if (!active) cancel(); }, [active]);
  useEffect(() => {
    // 保存会先广播 recordingId:null，再 resolve；仅 recording 阶段处理主动结束。
    if (phase === "recording" && state && session.current !== null && state.revision > session.current.revision && state.recordingId !== session.current.id) reset();
  }, [phase, state]);
  const recording = phase === "starting" || phase === "recording" || phase === "saving-recording";
  const busy = phase !== "idle";
  const note = candidate && !candidate.error && recording ? "松开按键后保存"
    : recording ? "按 Esc 取消。按下后没反应，说明这个组合已被系统或其他应用占用"
    : state?.status === "disabled" ? "已停用，可以从悬浮球或托盘打开表情面板" : null;
  const failure = error ?? (state?.status === "failed" ? state.error : null) ?? (query.error ? String(query.error) : null);
  const describedBy = [note && `${id}-note`, failure && `${id}-error`].filter(Boolean).join(" ") || undefined;
  const buttonAriaLabel = recording
    ? candidate && !candidate.error && candidate.accessibleLabel
      ? `正在录制呼出快捷键，已按下 ${candidate.accessibleLabel}，松开按键后保存`
      : "正在录制呼出快捷键，按下新的组合键，Esc 取消"
    : !state ? (query.error ? "呼出快捷键读取失败" : "呼出快捷键正在读取")
    : state.accessibleLabel ? `呼出快捷键 ${state.accessibleLabel}，按下可修改`
    : "呼出快捷键已停用，按下可录制新的组合";
  return <>
    <div className="flex items-center justify-between gap-4">
      <span id={`${id}-label`}>呼出快捷键</span>
      <div className="flex items-center gap-3">
        {recording ? <button type="button" tabIndex={-1} disabled={phase === "saving-recording"} className={link}
          onMouseDown={(event) => { event.preventDefault(); cancel(true); }} onClick={() => cancel(true)}>取消</button> : <>
          {state && state.shortcut !== state.defaultShortcut && <button type="button" disabled={busy} className={link}
            onClick={() => save(state.defaultShortcut, (label) => `已恢复默认的 ${label ?? state.defaultLabel}`, undefined, true)}>恢复默认</button>}
          {state?.shortcut && <button type="button" disabled={busy} className={link} onClick={() => save(null, undefined, undefined, true)}>停用</button>}
        </>}
        <button ref={button} type="button" aria-label={buttonAriaLabel} aria-describedby={describedBy}
          aria-busy={busy} disabled={!state} onClick={begin} onKeyDown={keyDown} onKeyUp={keyUp} onBlur={() => cancel()}
          className={`interactive inline-flex min-w-24 items-center justify-center rounded-control bg-muted px-3 py-1 font-semibold tabular-nums ${recording ? "outline-2 outline-offset-2 outline-accent" : "hover:opacity-82"}`}>
          <kbd className={`font-sans ${recording && !candidate?.label && !held ? "font-normal text-ink-secondary" : !recording && !state?.label ? "font-normal text-ink-secondary" : ""}`}>
            {recording ? candidate?.label || held || "按下组合键" : state?.label ?? (state ? "已停用" : query.error ? "读取失败" : "正在读取…")}
          </kbd>
        </button>
      </div>
    </div>
    {note && <p id={`${id}-note`} className="text-xs text-ink-secondary">{note}</p>}
    {failure && <div className="flex flex-wrap items-baseline gap-2 text-xs">
      <p id={`${id}-error`} role="alert" className="text-accent">{failure}</p>
      {state?.status === "failed" && !recording && <button type="button" disabled={busy} className={link} onClick={() => save(state.shortcut, undefined, undefined, true)}>重试</button>}
    </div>}
    <span className="sr-only" role="status">{announcement}</span>
  </>;
}
