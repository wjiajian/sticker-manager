-- 格式 2 的路径缓存和未完成移动只保存在本机，不进入快照。
CREATE TABLE sync_path_cache (
    library_id TEXT NOT NULL, path TEXT NOT NULL, size INTEGER NOT NULL,
    version TEXT, hash TEXT NOT NULL, sticker_id TEXT NOT NULL, object TEXT NOT NULL, managed INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(library_id, path)
);
CREATE TABLE sync_directory_cache (library_id TEXT NOT NULL, path TEXT NOT NULL, category_id TEXT, PRIMARY KEY(library_id,path));
CREATE TABLE sync_file_moves (
    library_id TEXT NOT NULL, sticker_id TEXT NOT NULL, source TEXT NOT NULL,
    destination TEXT NOT NULL, object TEXT NOT NULL, revision INTEGER NOT NULL,
    PRIMARY KEY(library_id, sticker_id)
);
CREATE TABLE sync_file_revision (singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL);
INSERT INTO sync_file_revision VALUES(1,0);
CREATE TABLE sync_file_dirty (sticker_id TEXT PRIMARY KEY, revision INTEGER NOT NULL);
DROP TRIGGER sync_stickers_category_id;
DROP TRIGGER sync_stickers_create;
CREATE TRIGGER sync_stickers_create AFTER INSERT ON stickers
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1
BEGIN
    UPDATE sync_meta SET max_at = MAX(CAST(unixepoch('subsec') * 1000 AS INTEGER), max_at + 1) WHERE singleton = 1;
    INSERT INTO sync_outbox(at, device, action)
    SELECT max_at, device, json_object('type', 'create', 'entity', 'sticker', 'id', NEW.id, 'data', json_object('hash', NEW.hash, 'ext', NEW.ext, 'animated', NEW.animated, 'width', NEW.width, 'height', NEW.height, 'byte_size', NEW.byte_size, 'added_at', NEW.added_at), 'fields', json_object('name', NEW.name, 'caption', NEW.caption, 'favorite', NEW.favorite, 'deleted_at', NEW.deleted_at)) FROM sync_meta WHERE singleton = 1;
    UPDATE sync_file_revision SET revision=revision+1;
    INSERT INTO sync_file_dirty SELECT NEW.id,revision FROM sync_file_revision WHERE singleton=1 ON CONFLICT(sticker_id) DO UPDATE SET revision=excluded.revision;
END;
CREATE TRIGGER sync_stickers_file AFTER UPDATE OF category_id,name,deleted_at ON stickers
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1
    AND (NEW.category_id IS NOT OLD.category_id OR NEW.name IS NOT OLD.name OR NEW.deleted_at IS NOT OLD.deleted_at)
BEGIN
    UPDATE sync_file_revision SET revision=revision+1;
    INSERT INTO sync_file_dirty SELECT NEW.id,revision FROM sync_file_revision WHERE singleton=1 ON CONFLICT(sticker_id) DO UPDATE SET revision=excluded.revision;
END;
CREATE TRIGGER sync_categories_file AFTER UPDATE OF name ON categories
WHEN (SELECT recording FROM sync_meta WHERE singleton = 1) = 1 AND NEW.name IS NOT OLD.name
BEGIN
    UPDATE sync_file_revision SET revision=revision+1;
    INSERT INTO sync_file_dirty SELECT id,(SELECT revision FROM sync_file_revision) FROM stickers WHERE category_id=NEW.id ON CONFLICT(sticker_id) DO UPDATE SET revision=excluded.revision;
END;
-- 旧格式的传输队列不能发布到格式 2；永久删除、合并映射与字段时钟仍保留。
DELETE FROM sync_stream;
DELETE FROM sync_progress;
DELETE FROM sync_origins;
DELETE FROM sync_targets;
DELETE FROM sync_device_cache;
DELETE FROM sync_deleted_objects;
DELETE FROM sync_outbox WHERE json_extract(action,'$.entity')='sticker' AND json_extract(action,'$.field')='category_id';
UPDATE sync_outbox SET action=json_remove(action,'$.fields.category_id') WHERE json_extract(action,'$.entity')='sticker';
INSERT OR IGNORE INTO sync_file_dirty SELECT id,0 FROM stickers;

UPDATE sync_pending SET change = COALESCE((
    SELECT json_group_array(json(CASE WHEN json_extract(value,'$.entity')='sticker'
        THEN json_remove(value,'$.fields.category_id') ELSE value END))
    FROM json_each(sync_pending.change)
    WHERE NOT (COALESCE(json_extract(value,'$.entity'),'')='sticker'
        AND COALESCE(json_extract(value,'$.field'),'')='category_id')
), '[]');
DELETE FROM sync_pending WHERE change='[]';
