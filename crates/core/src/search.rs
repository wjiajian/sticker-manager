use crate::{Library, Result, Sticker};

impl Library {
    /// 每个词都要匹配名称、配文或分类；完整搜索词的完全匹配和前缀匹配优先。
    pub fn search_stickers(&self, query: &str) -> Result<Vec<Sticker>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let mut filter = String::from("s.deleted_at IS NULL");
        let mut patterns = Vec::new();
        for word in query.split_whitespace() {
            let escaped = word
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            patterns.push(format!("%{escaped}%"));
            let parameter = patterns.len();
            // 只拼接占位符编号，用户输入始终通过参数绑定。SQLite LIKE 默认忽略英文大小写。
            filter.push_str(&format!(
                " AND (s.name LIKE ?{parameter} ESCAPE '\\'
                    OR s.caption LIKE ?{parameter} ESCAPE '\\'
                    OR EXISTS (SELECT 1 FROM categories search_c
                        WHERE search_c.id = s.category_id
                            AND search_c.name LIKE ?{parameter} ESCAPE '\\'))"
            ));
        }
        let parameters: Vec<_> = patterns.iter().map(String::as_str).collect();
        let mut stickers = self.query_stickers(
            &filter,
            "(SELECT MAX(u.last_used_at) FROM usages u WHERE u.sticker_id = s.id) DESC,
             s.added_at DESC, s.rowid DESC",
            &parameters,
        )?;
        // 分类已经批量读回；稳定排序只调整匹配档位，保留同档内的使用和添加时间顺序。
        stickers.sort_by_cached_key(|sticker| match_rank(sticker, query));
        Ok(stickers)
    }
}

fn match_rank(sticker: &Sticker, query: &str) -> u8 {
    std::iter::once(sticker.name.as_str())
        .chain(sticker.caption.as_deref())
        .chain(
            sticker
                .category
                .as_ref()
                .map(|category| category.name.as_str()),
        )
        .map(|value| {
            if value.eq_ignore_ascii_case(query) {
                0
            } else if value
                .get(..query.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(query))
            {
                1
            } else {
                2
            }
        })
        .min()
        .unwrap_or(2)
}

#[cfg(test)]
mod tests;
