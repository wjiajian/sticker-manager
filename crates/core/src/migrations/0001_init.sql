-- 表情。原文件按内容哈希存在仓库的 objects/ 目录里（ADR 0002）。
-- 主键不用哈希：彻底删除后再导入同一张图算作新表情（ADR 0004）。
CREATE TABLE stickers (
    id          TEXT PRIMARY KEY,
    hash        TEXT NOT NULL UNIQUE,
    ext         TEXT NOT NULL,
    name        TEXT NOT NULL,
    caption     TEXT,                       -- 配文，最多一段
    favorite    INTEGER NOT NULL DEFAULT 0, -- 收藏
    animated    INTEGER NOT NULL DEFAULT 0,
    width       INTEGER NOT NULL,
    height      INTEGER NOT NULL,
    byte_size   INTEGER NOT NULL,
    added_at    INTEGER NOT NULL,           -- Unix 毫秒
    deleted_at  INTEGER                     -- 进入最近删除的时间；NULL 表示没有删除
);

CREATE TABLE tag_groups (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    position    INTEGER NOT NULL
);

-- group_id 为 NULL 的标签显示在"未分组"下
CREATE TABLE tags (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    color       TEXT NOT NULL,
    group_id    TEXT REFERENCES tag_groups(id) ON DELETE SET NULL,
    position    INTEGER NOT NULL
);

CREATE TABLE sticker_tags (
    sticker_id  TEXT NOT NULL REFERENCES stickers(id) ON DELETE CASCADE,
    tag_id      TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (sticker_id, tag_id)
);
CREATE INDEX sticker_tags_tag ON sticker_tags(tag_id);

-- 每台设备各自计数；合并时次数相加，最近一次使用时间取最晚的（ADR 0003）
CREATE TABLE usages (
    sticker_id   TEXT NOT NULL REFERENCES stickers(id) ON DELETE CASCADE,
    device_id    TEXT NOT NULL,
    count        INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL,
    PRIMARY KEY (sticker_id, device_id)
);
