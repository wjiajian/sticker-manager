-- 本地改动与操作日志在同一 SQLite 事务内提交。远端投影时暂时关闭捕获。
CREATE TABLE sync_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    device TEXT NOT NULL DEFAULT '',
    max_at INTEGER NOT NULL DEFAULT 0,
    recording INTEGER NOT NULL DEFAULT 0
);
INSERT INTO sync_meta(singleton) VALUES(1);
CREATE TABLE sync_outbox (
    revision INTEGER PRIMARY KEY AUTOINCREMENT,
    at INTEGER NOT NULL,
    device TEXT NOT NULL,
    action TEXT NOT NULL
);
CREATE TABLE sync_model (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    revision INTEGER NOT NULL,
    state TEXT NOT NULL
);
CREATE TABLE sync_targets (
    library_id TEXT PRIMARY KEY,
    revision INTEGER NOT NULL,
    next_seq INTEGER NOT NULL DEFAULT 0,
    pushed_seq INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE sync_stream (
    library_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    operation TEXT NOT NULL,
    PRIMARY KEY(library_id, seq)
);
CREATE TABLE sync_progress (
    library_id TEXT NOT NULL,
    device TEXT NOT NULL,
    seq INTEGER NOT NULL,
    PRIMARY KEY(library_id, device)
);
CREATE TABLE sync_pending (
    library_id TEXT NOT NULL,
    device TEXT NOT NULL,
    seq INTEGER NOT NULL,
    change TEXT NOT NULL,
    PRIMARY KEY(library_id, device, seq)
);
-- 合并前同一设备可能使用过两个不同 id；保留分量，不能让绝对计数被重复相加。
CREATE TABLE sync_usage_offsets (
    sticker_id TEXT NOT NULL REFERENCES stickers(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL,
    count INTEGER NOT NULL,
    PRIMARY KEY(sticker_id, device_id)
);

CREATE TRIGGER sync_stickers_create AFTER INSERT ON stickers
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (1)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'create', 'entity', 'sticker', 'id', NEW.id, 'data', json_object('hash', NEW.hash, 'ext', NEW.ext, 'animated', NEW.animated, 'width', NEW.width, 'height', NEW.height, 'byte_size', NEW.byte_size, 'added_at', NEW.added_at), 'fields', json_object('name', NEW.name, 'caption', NEW.caption, 'favorite', NEW.favorite, 'deleted_at', NEW.deleted_at)) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_stickers_name AFTER UPDATE OF name ON stickers
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.name IS NOT OLD.name)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'sticker', 'id', NEW.id, 'field', 'name', 'value', NEW.name) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_stickers_caption AFTER UPDATE OF caption ON stickers
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.caption IS NOT OLD.caption)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'sticker', 'id', NEW.id, 'field', 'caption', 'value', NEW.caption) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_stickers_favorite AFTER UPDATE OF favorite ON stickers
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.favorite IS NOT OLD.favorite)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'sticker', 'id', NEW.id, 'field', 'favorite', 'value', NEW.favorite) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_stickers_deleted_at AFTER UPDATE OF deleted_at ON stickers
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.deleted_at IS NOT OLD.deleted_at)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'sticker', 'id', NEW.id, 'field', 'deleted_at', 'value', NEW.deleted_at) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_stickers_delete AFTER DELETE ON stickers
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (1)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'purge', 'entity', 'sticker', 'ids', json_array(OLD.id)) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_tags_create AFTER INSERT ON tags
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (1)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'create', 'entity', 'tag', 'id', NEW.id, 'data', NULL, 'fields', json_object('name', NEW.name, 'color', NEW.color, 'group_id', NEW.group_id, 'position', NEW.position)) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_tags_name AFTER UPDATE OF name ON tags
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.name IS NOT OLD.name)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'tag', 'id', NEW.id, 'field', 'name', 'value', NEW.name) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_tags_color AFTER UPDATE OF color ON tags
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.color IS NOT OLD.color)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'tag', 'id', NEW.id, 'field', 'color', 'value', NEW.color) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_tags_group_id AFTER UPDATE OF group_id ON tags
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.group_id IS NOT OLD.group_id)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'tag', 'id', NEW.id, 'field', 'group_id', 'value', NEW.group_id) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_tags_position AFTER UPDATE OF position ON tags
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.position IS NOT OLD.position)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'tag', 'id', NEW.id, 'field', 'position', 'value', NEW.position) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_tags_delete AFTER DELETE ON tags
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (1)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'delete', 'entity', 'tag', 'ids', json_array(OLD.id)) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_tag_groups_create AFTER INSERT ON tag_groups
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (1)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'create', 'entity', 'group', 'id', NEW.id, 'data', NULL, 'fields', json_object('name', NEW.name, 'position', NEW.position)) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_tag_groups_name AFTER UPDATE OF name ON tag_groups
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.name IS NOT OLD.name)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'group', 'id', NEW.id, 'field', 'name', 'value', NEW.name) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_tag_groups_position AFTER UPDATE OF position ON tag_groups
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.position IS NOT OLD.position)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'group', 'id', NEW.id, 'field', 'position', 'value', NEW.position) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_tag_groups_delete AFTER DELETE ON tag_groups
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (1)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'delete', 'entity', 'group', 'ids', json_array(OLD.id)) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_link_insert AFTER INSERT ON sticker_tags
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (1)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'link', 'sticker', NEW.sticker_id, 'tag', NEW.tag_id, 'present', 1) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_link_delete AFTER DELETE ON sticker_tags
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (1)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'link', 'sticker', OLD.sticker_id, 'tag', OLD.tag_id, 'present', 0) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_usage_insert AFTER INSERT ON usages
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.device_id = (SELECT device FROM sync_meta WHERE singleton = 1))
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'usage', 'sticker', NEW.sticker_id, 'owner', NEW.device_id, 'count', NEW.count - COALESCE((SELECT count FROM sync_usage_offsets WHERE sticker_id = NEW.sticker_id AND device_id = NEW.device_id), 0), 'last_used_at', NEW.last_used_at) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_usage_update AFTER UPDATE ON usages
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.device_id = (SELECT device FROM sync_meta WHERE singleton = 1))
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'usage', 'sticker', NEW.sticker_id, 'owner', NEW.device_id, 'count', NEW.count - COALESCE((SELECT count FROM sync_usage_offsets WHERE sticker_id = NEW.sticker_id AND device_id = NEW.device_id), 0), 'last_used_at', NEW.last_used_at) FROM sync_meta WHERE singleton = 1;
END;

CREATE TABLE sync_deleted_objects (
    library_id TEXT NOT NULL,
    key TEXT NOT NULL,
    PRIMARY KEY(library_id, key)
);
