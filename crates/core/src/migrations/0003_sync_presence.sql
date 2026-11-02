-- 本机确认信息；不参与元数据投影，也不随表情的临时重建级联删除。
CREATE TABLE sync_origins (
    library_id TEXT NOT NULL,
    sticker_id TEXT NOT NULL,
    device TEXT NOT NULL,
    seq INTEGER NOT NULL,
    PRIMARY KEY(library_id, sticker_id, device, seq)
);
CREATE TABLE sync_device_cache (
    library_id TEXT PRIMARY KEY,
    devices TEXT NOT NULL,
    last_sync_at INTEGER NOT NULL
);
-- 升级前的目标已完成首次加入；新加入的目标显式写 0。
ALTER TABLE sync_targets ADD COLUMN initial_pull_complete INTEGER NOT NULL DEFAULT 1;
