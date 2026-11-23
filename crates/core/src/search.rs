use std::collections::HashMap;

use crate::{Library, Result, Sticker};
use matcher::Query;

mod matcher;

impl Library {
    /// 扫描未删除表情的名称、配文和分类并在内存中完成原文/拼音匹配，
    /// 再按 `rowid` 批量取回完整数据并按九档相关度稳定排序（规则见 `docs/tech-stack.md`）。
    pub fn search_stickers(&self, query: &str) -> Result<Vec<Sticker>> {
        let Some(mut query) = Query::new(query) else {
            return Ok(Vec::new());
        };
        let mut rowids = Vec::new();
        let mut ranks = HashMap::new();
        let mut statement = self.conn.prepare(
            "SELECT s.rowid, s.id, s.name, s.caption, c.name FROM stickers s
             LEFT JOIN categories c ON c.id = s.category_id WHERE s.deleted_at IS NULL",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let field = |column| -> rusqlite::Result<&str> {
                Ok(row.get_ref(column)?.as_str_or_null()?.unwrap_or(""))
            };
            if let Some(rank) = query.rank([field(2)?, field(3)?, field(4)?]) {
                rowids.push(row.get::<_, i64>(0)?);
                ranks.insert(row.get::<_, String>(1)?, rank);
            }
        }
        if rowids.is_empty() {
            return Ok(Vec::new());
        }
        let rowids = serde_json::to_string(&rowids)?;
        let mut stickers = self.query_stickers(
            "s.deleted_at IS NULL AND s.rowid IN (SELECT value FROM json_each(?1))",
            "(SELECT MAX(u.last_used_at) FROM usages u WHERE u.sticker_id = s.id) DESC,
             s.added_at DESC, s.rowid DESC",
            &[&rowids],
        )?;
        stickers.sort_by_cached_key(|sticker| ranks[&sticker.id]);
        Ok(stickers)
    }
}

#[cfg(test)]
mod tests;
