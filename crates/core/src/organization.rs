use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde::Serialize;
use uuid::Uuid;

use crate::{Error, Library, Result};

/// 顺序与 design/README.md 的分类圆点色一致。
pub const CATEGORY_COLORS: [&str; 8] = [
    "#F2B705", "#8A8A8A", "#E5484D", "#3E7BFA", "#2BA471", "#7A5AF8", "#E8572A", "#A0662F",
];

/// 没有分类的表情显示在这个名字下，所以不能用它给真正的分类命名。
pub const UNCATEGORIZED: &str = "未分类";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    pub color: String,
    pub position: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySummary {
    #[serde(flatten)]
    pub category: Category,
    pub sticker_count: u32,
    pub total_sticker_count: u32,
}

impl Category {
    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
            position: row.get(3)?,
        })
    }
}

impl Library {
    /// `category_id` 为空表示移到"未分类"。已删除的表情不动。
    pub fn set_stickers_category(
        &mut self,
        ids: &[String],
        category_id: Option<&str>,
    ) -> Result<usize> {
        let tx = self.conn.transaction()?;
        if let Some(category_id) = category_id {
            require_category(&tx, category_id)?;
        }
        let count = batch_set(&tx, ids, category_id)?;
        tx.commit()?;
        Ok(count)
    }

    pub fn set_sticker_category(
        &mut self,
        sticker_id: &str,
        category_id: Option<&str>,
    ) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_editable_sticker(&tx, sticker_id)?;
        if let Some(category_id) = category_id {
            require_category(&tx, category_id)?;
        }
        tx.execute(
            "UPDATE stickers SET category_id = ?2 WHERE id = ?1",
            params![sticker_id, category_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 重试同名分类时复用它；新建和整批设置在同一个事务内。
    pub fn create_category_and_set_stickers(
        &mut self,
        ids: &[String],
        name: &str,
    ) -> Result<(Category, usize)> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let category = tx
            .query_row(
                "SELECT id,name,color,position FROM categories WHERE name=?1 COLLATE NOCASE",
                [name.trim()],
                Category::from_row,
            )
            .optional()?;
        let category = match category {
            Some(category) => category,
            None => insert_category(&tx, name, None)?,
        };
        let count = batch_set(&tx, ids, Some(&category.id))?;
        tx.commit()?;
        Ok((category, count))
    }

    pub fn create_category(&mut self, name: &str, color: Option<&str>) -> Result<Category> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let category = insert_category(&tx, name, color)?;
        tx.commit()?;
        Ok(category)
    }

    pub fn rename_category(&mut self, id: &str, name: &str) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_category(&tx, id)?;
        let name = validate_name(&tx, name, Some(id))?;
        tx.execute(
            "UPDATE categories SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_category_color(&self, id: &str, color: &str) -> Result<()> {
        let color = validate_color(color)?;
        let changed = self.conn.execute(
            "UPDATE categories SET color = ?2 WHERE id = ?1",
            params![id, color],
        )?;
        if changed == 0 {
            return Err(Error::CategoryUnavailable);
        }
        Ok(())
    }

    pub fn delete_category(&self, id: &str) -> Result<()> {
        // 外键把这个分类下的表情（包括最近删除中的）移到"未分类"，不删表情。
        if self
            .conn
            .execute("DELETE FROM categories WHERE id = ?1", [id])?
            == 0
        {
            return Err(Error::CategoryUnavailable);
        }
        Ok(())
    }

    pub fn set_sticker_name(&self, id: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::EmptyName("表情"));
        }
        crate::sync::validate_sticker_name(name)?;
        let changed = self.conn.execute(
            "UPDATE stickers SET name=?2 WHERE id=?1 AND deleted_at IS NULL",
            params![id, name],
        )?;
        if changed == 0 {
            return Err(Error::StickerUnavailable);
        }
        Ok(())
    }

    pub fn set_sticker_caption(&self, id: &str, caption: &str) -> Result<()> {
        let caption = caption.trim();
        let caption = if caption.is_empty() {
            None
        } else {
            Some(caption)
        };
        let changed = self.conn.execute(
            "UPDATE stickers SET caption = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            params![id, caption],
        )?;
        if changed == 0 {
            return Err(Error::StickerUnavailable);
        }
        Ok(())
    }

    pub fn categories(&self) -> Result<Vec<CategorySummary>> {
        let mut statement = self.conn.prepare(
            "SELECT c.id, c.name, c.color, c.position, COUNT(s.id), COUNT(a.id)
             FROM categories c
             LEFT JOIN stickers a ON a.category_id = c.id
             LEFT JOIN stickers s ON s.id = a.id AND s.deleted_at IS NULL
             GROUP BY c.id ORDER BY c.position, c.id",
        )?;
        let categories = statement
            .query_map([], |row| {
                Ok(CategorySummary {
                    category: Category::from_row(row)?,
                    sticker_count: row.get(4)?,
                    total_sticker_count: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(categories)
    }
}

fn batch_set(conn: &Connection, ids: &[String], category: Option<&str>) -> Result<usize> {
    let mut count = 0;
    for id in ids.iter().collect::<std::collections::BTreeSet<_>>() {
        count += conn.execute(
            "UPDATE stickers SET category_id = ?2 WHERE id = ?1 AND deleted_at IS NULL AND category_id IS NOT ?2",
            params![id, category],
        )?;
    }
    Ok(count)
}

fn insert_category(conn: &Connection, name: &str, color: Option<&str>) -> Result<Category> {
    let name = validate_name(conn, name, None)?;
    let position: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM categories",
        [],
        |row| row.get(0),
    )?;
    let color = match color {
        Some(color) => validate_color(color)?,
        None => CATEGORY_COLORS[position.rem_euclid(CATEGORY_COLORS.len() as i64) as usize],
    };
    let category = Category {
        id: Uuid::new_v4().to_string(),
        name: name.to_owned(),
        color: color.to_owned(),
        position,
    };
    conn.execute(
        "INSERT INTO categories (id, name, color, position) VALUES (?1, ?2, ?3, ?4)",
        params![
            category.id,
            category.name,
            category.color,
            category.position
        ],
    )?;
    Ok(category)
}

/// 分类名以后要直接用作文件夹名，所以不能带路径分隔符，也不能以点开头。
fn validate_name<'a>(
    conn: &Connection,
    name: &'a str,
    exclude_id: Option<&str>,
) -> Result<&'a str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::EmptyName("分类"));
    }
    if name == UNCATEGORIZED {
        return Err(Error::ReservedCategoryName);
    }
    if name.starts_with('.') || name.contains("..") || name.contains(['/', '\\']) {
        return Err(Error::InvalidCategoryName);
    }
    // 用户输入始终通过参数绑定。事务避免两个连接同时通过重名检查。
    let exists: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM categories WHERE name = ?1 COLLATE NOCASE AND (?2 IS NULL OR id != ?2))",
        params![name, exclude_id],
        |row| row.get(0),
    )?;
    if exists {
        return Err(Error::DuplicateName("分类"));
    }
    Ok(name)
}

fn validate_color(color: &str) -> Result<&'static str> {
    CATEGORY_COLORS
        .iter()
        .copied()
        .find(|candidate| candidate.eq_ignore_ascii_case(color))
        .ok_or(Error::InvalidCategoryColor)
}

fn require_category(conn: &Connection, id: &str) -> Result<()> {
    if !conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM categories WHERE id = ?1)",
        [id],
        |row| row.get::<_, bool>(0),
    )? {
        return Err(Error::CategoryUnavailable);
    }
    Ok(())
}

fn require_editable_sticker(conn: &Connection, id: &str) -> Result<()> {
    if !conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM stickers WHERE id = ?1 AND deleted_at IS NULL)",
        [id],
        |row| row.get::<_, bool>(0),
    )? {
        return Err(Error::StickerUnavailable);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
