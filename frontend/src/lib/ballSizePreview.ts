/** 合并尚未发送的预览；保存和取消排在已发送的预览后，防止迟到的 IPC 恢复旧尺寸。 */
export function ballSizePreview(
  preview: (size: number | null) => Promise<void>,
) {
  type Request = { size: number | null; onError: (reason: unknown) => void };
  let batch: { latest?: Request } | undefined;
  let tail = Promise.resolve();

  function schedule(size: number | null, onError: (reason: unknown) => void) {
    if (batch) {
      batch.latest = { size, onError };
    } else {
      const current = { latest: { size, onError } as Request | undefined };
      batch = current;
      tail = tail.then(async () => {
        try {
          while (current.latest) {
            const request = current.latest;
            current.latest = undefined;
            try { await preview(request.size); }
            catch (reason) { request.onError(reason); }
          }
        } finally { if (batch === current) batch = undefined; }
      });
    }
    return tail;
  }

  return {
    preview: schedule,
    commit<T>(save: () => Promise<T>): Promise<T> {
      // 已松手，无需再发送队列里的中间尺寸；在途预览仍须完成后才能保存。
      if (batch) batch.latest = undefined;
      // 后续预览属于新的批次，必须排在这次保存之后（含对话框重新打开的情况）。
      batch = undefined;
      const result = tail.then(async () => {
        try { return await save(); }
        finally { await preview(null); }
      });
      tail = result.then(() => {}, () => {});
      return result;
    },
  };
}
