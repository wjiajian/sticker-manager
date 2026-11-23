-- 为未删除的收藏表情按分页顺序建部分索引，避免翻页时全表扫描和排序。
CREATE INDEX stickers_favorites_added_at ON stickers(added_at DESC, id DESC)
WHERE deleted_at IS NULL AND favorite = 1;
