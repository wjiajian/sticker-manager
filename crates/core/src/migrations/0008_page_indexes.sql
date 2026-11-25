-- 为面板未删除表情的「全部表情」「未分类」和「分类」分页补齐按添加时间倒序的部分索引。
CREATE INDEX stickers_active_added_at ON stickers(added_at DESC, id DESC)
WHERE deleted_at IS NULL;

CREATE INDEX stickers_category_added_at ON stickers(category_id, added_at DESC, id DESC)
WHERE deleted_at IS NULL;
