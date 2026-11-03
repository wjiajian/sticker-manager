-- 标签改为分类：每张表情最多属于一个分类，没有分类的显示在"未分类"下；标签分组取消。
-- 旧的同步状态不再兼容（操作类型变了），一并清掉，下次启用同步时从当前表重建。
DROP TRIGGER sync_link_insert;
DROP TRIGGER sync_link_delete;
DROP TRIGGER sync_tags_create;
DROP TRIGGER sync_tags_name;
DROP TRIGGER sync_tags_color;
DROP TRIGGER sync_tags_group_id;
DROP TRIGGER sync_tags_position;
DROP TRIGGER sync_tags_delete;
DROP TRIGGER sync_tag_groups_create;
DROP TRIGGER sync_tag_groups_name;
DROP TRIGGER sync_tag_groups_position;
DROP TRIGGER sync_tag_groups_delete;
DROP TRIGGER sync_stickers_create;

CREATE TABLE categories (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    color       TEXT NOT NULL,
    position    INTEGER NOT NULL
);
INSERT INTO categories (id, name, color, position) SELECT id, name, color, position FROM tags;

ALTER TABLE stickers ADD COLUMN category_id TEXT REFERENCES categories(id) ON DELETE SET NULL;
-- 原来带多个标签的表情，只保留 position 最靠前的那个。
UPDATE stickers SET category_id = (
    SELECT t.id FROM sticker_tags st JOIN tags t ON t.id = st.tag_id
    WHERE st.sticker_id = stickers.id ORDER BY t.position, t.id LIMIT 1
);
CREATE INDEX stickers_category ON stickers(category_id);

DROP TABLE sticker_tags;
DROP TABLE tags;
DROP TABLE tag_groups;

DELETE FROM sync_model;
DELETE FROM sync_outbox;
DELETE FROM sync_stream;
DELETE FROM sync_pending;
DELETE FROM sync_progress;
DELETE FROM sync_origins;
DELETE FROM sync_targets;

CREATE TRIGGER sync_stickers_create AFTER INSERT ON stickers
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (1)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'create', 'entity', 'sticker', 'id', NEW.id, 'data', json_object('hash', NEW.hash, 'ext', NEW.ext, 'animated', NEW.animated, 'width', NEW.width, 'height', NEW.height, 'byte_size', NEW.byte_size, 'added_at', NEW.added_at), 'fields', json_object('name', NEW.name, 'caption', NEW.caption, 'favorite', NEW.favorite, 'deleted_at', NEW.deleted_at, 'category_id', NEW.category_id)) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_stickers_category_id AFTER UPDATE OF category_id ON stickers
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.category_id IS NOT OLD.category_id)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'sticker', 'id', NEW.id, 'field', 'category_id', 'value', NEW.category_id) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_categories_create AFTER INSERT ON categories
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (1)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'create', 'entity', 'category', 'id', NEW.id, 'data', NULL, 'fields', json_object('name', NEW.name, 'color', NEW.color, 'position', NEW.position)) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_categories_name AFTER UPDATE OF name ON categories
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.name IS NOT OLD.name)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'category', 'id', NEW.id, 'field', 'name', 'value', NEW.name) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_categories_color AFTER UPDATE OF color ON categories
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.color IS NOT OLD.color)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'category', 'id', NEW.id, 'field', 'color', 'value', NEW.color) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_categories_position AFTER UPDATE OF position ON categories
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (NEW.position IS NOT OLD.position)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'set', 'entity', 'category', 'id', NEW.id, 'field', 'position', 'value', NEW.position) FROM sync_meta WHERE singleton = 1;
END;

CREATE TRIGGER sync_categories_delete AFTER DELETE ON categories
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND (1)
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'delete', 'entity', 'category', 'ids', json_array(OLD.id)) FROM sync_meta WHERE singleton = 1;
END;
