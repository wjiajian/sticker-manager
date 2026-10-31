use rusqlite::{Connection, Row, TransactionBehavior, params};
use serde::Serialize;
use uuid::Uuid;

use crate::{Error, Library, Result};

/// 顺序与 design/README.md 的标签圆点色一致。
pub const TAG_COLORS: [&str; 8] = [
    "#F2B705", "#8A8A8A", "#E5484D", "#3E7BFA", "#2BA471", "#7A5AF8", "#E8572A", "#A0662F",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: String,
    pub group_id: Option<String>,
    pub position: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagSummary {
    #[serde(flatten)]
    pub tag: Tag,
    pub sticker_count: u32,
    pub total_sticker_count: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagGroup {
    pub id: String,
    pub name: String,
    pub position: i64,
    pub tags: Vec<TagSummary>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagSidebar {
    pub groups: Vec<TagGroup>,
    pub ungrouped: Vec<TagSummary>,
}

impl Tag {
    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
            group_id: row.get(3)?,
            position: row.get(4)?,
        })
    }
}

impl Library {
    pub fn create_tag(
        &mut self,
        name: &str,
        color: Option<&str>,
        group_id: Option<&str>,
    ) -> Result<Tag> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let tag = insert_tag(&tx, name, color, group_id)?;
        tx.commit()?;
        Ok(tag)
    }

    /// 新建并添加是一次操作，任一步失败都不保留改动。
    pub fn create_tag_and_add(&mut self, sticker_id: &str, name: &str) -> Result<Tag> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_editable_sticker(&tx, sticker_id)?;
        let tag = insert_tag(&tx, name, None, None)?;
        tx.execute(
            "INSERT INTO sticker_tags (sticker_id, tag_id) VALUES (?1, ?2)",
            params![sticker_id, tag.id],
        )?;
        tx.commit()?;
        Ok(tag)
    }

    pub fn rename_tag(&mut self, id: &str, name: &str) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_tag(&tx, id)?;
        let name = validate_name(&tx, "tags", "标签", name, Some(id))?;
        tx.execute("UPDATE tags SET name = ?2 WHERE id = ?1", params![id, name])?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_tag_color(&self, id: &str, color: &str) -> Result<()> {
        let color = validate_color(color)?;
        let changed = self.conn.execute(
            "UPDATE tags SET color = ?2 WHERE id = ?1",
            params![id, color],
        )?;
        if changed == 0 {
            return Err(Error::TagUnavailable);
        }
        Ok(())
    }

    pub fn move_tag(&mut self, id: &str, group_id: Option<&str>) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_tag(&tx, id)?;
        if let Some(group_id) = group_id {
            require_group(&tx, group_id)?;
        }
        tx.execute(
            "UPDATE tags SET group_id = ?2 WHERE id = ?1",
            params![id, group_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_tag(&self, id: &str) -> Result<()> {
        // 外键同时移除所有表情上的此标签，包括最近删除中的表情。
        if self.conn.execute("DELETE FROM tags WHERE id = ?1", [id])? == 0 {
            return Err(Error::TagUnavailable);
        }
        Ok(())
    }

    pub fn create_tag_group(&mut self, name: &str) -> Result<TagGroup> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let name = validate_name(&tx, "tag_groups", "标签分组", name, None)?;
        let position: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM tag_groups",
            [],
            |row| row.get(0),
        )?;
        let group = TagGroup {
            id: Uuid::new_v4().to_string(),
            name: name.to_owned(),
            position,
            tags: Vec::new(),
        };
        tx.execute(
            "INSERT INTO tag_groups (id, name, position) VALUES (?1, ?2, ?3)",
            params![group.id, group.name, group.position],
        )?;
        tx.commit()?;
        Ok(group)
    }

    pub fn rename_tag_group(&mut self, id: &str, name: &str) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_group(&tx, id)?;
        let name = validate_name(&tx, "tag_groups", "标签分组", name, Some(id))?;
        tx.execute(
            "UPDATE tag_groups SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_tag_group(&self, id: &str) -> Result<()> {
        // ON DELETE SET NULL 保留标签，将它们移到未分组。
        if self
            .conn
            .execute("DELETE FROM tag_groups WHERE id = ?1", [id])?
            == 0
        {
            return Err(Error::TagGroupUnavailable);
        }
        Ok(())
    }

    pub fn add_sticker_tag(&mut self, sticker_id: &str, tag_id: &str) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_editable_sticker(&tx, sticker_id)?;
        require_tag(&tx, tag_id)?;
        tx.execute(
            "INSERT INTO sticker_tags (sticker_id, tag_id) VALUES (?1, ?2)
             ON CONFLICT(sticker_id, tag_id) DO NOTHING",
            params![sticker_id, tag_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove_sticker_tag(&mut self, sticker_id: &str, tag_id: &str) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_editable_sticker(&tx, sticker_id)?;
        require_tag(&tx, tag_id)?;
        tx.execute(
            "DELETE FROM sticker_tags WHERE sticker_id = ?1 AND tag_id = ?2",
            params![sticker_id, tag_id],
        )?;
        tx.commit()?;
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

    pub fn tag_sidebar(&self) -> Result<TagSidebar> {
        let mut statement = self
            .conn
            .prepare("SELECT id, name, position FROM tag_groups ORDER BY position, id")?;
        let groups = statement
            .query_map([], |row| {
                Ok(TagGroup {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    position: row.get(2)?,
                    tags: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut sidebar = TagSidebar {
            groups,
            ungrouped: Vec::new(),
        };
        let mut statement = self.conn.prepare(
            "SELECT t.id, t.name, t.color, t.group_id, t.position, COUNT(s.id), COUNT(st.sticker_id)
             FROM tags t
             LEFT JOIN sticker_tags st ON st.tag_id = t.id
             LEFT JOIN stickers s ON s.id = st.sticker_id AND s.deleted_at IS NULL
             GROUP BY t.id ORDER BY t.position, t.id",
        )?;
        let tags = statement.query_map([], |row| {
            Ok(TagSummary {
                tag: Tag::from_row(row)?,
                sticker_count: row.get(5)?,
                total_sticker_count: row.get(6)?,
            })
        })?;
        for tag in tags {
            let tag = tag?;
            if let Some(group) = sidebar
                .groups
                .iter_mut()
                .find(|group| Some(&group.id) == tag.tag.group_id.as_ref())
            {
                group.tags.push(tag);
            } else {
                sidebar.ungrouped.push(tag);
            }
        }
        Ok(sidebar)
    }
}

fn insert_tag(
    conn: &Connection,
    name: &str,
    color: Option<&str>,
    group_id: Option<&str>,
) -> Result<Tag> {
    let name = validate_name(conn, "tags", "标签", name, None)?;
    if let Some(group_id) = group_id {
        require_group(conn, group_id)?;
    }
    let position: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM tags",
        [],
        |row| row.get(0),
    )?;
    let color = match color {
        Some(color) => validate_color(color)?,
        None => TAG_COLORS[position.rem_euclid(TAG_COLORS.len() as i64) as usize],
    };
    let tag = Tag {
        id: Uuid::new_v4().to_string(),
        name: name.to_owned(),
        color: color.to_owned(),
        group_id: group_id.map(str::to_owned),
        position,
    };
    conn.execute(
        "INSERT INTO tags (id, name, color, group_id, position) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![tag.id, tag.name, tag.color, tag.group_id, tag.position],
    )?;
    Ok(tag)
}

fn validate_name<'a>(
    conn: &Connection,
    table: &str,
    kind: &'static str,
    name: &'a str,
    exclude_id: Option<&str>,
) -> Result<&'a str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::EmptyName(kind));
    }
    // 表名来自本模块；用户输入始终通过参数绑定。事务避免两个连接同时通过重名检查。
    let exists: bool = conn.query_row(
        &format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE name = ?1 COLLATE NOCASE AND (?2 IS NULL OR id != ?2))"),
        params![name, exclude_id],
        |row| row.get(0),
    )?;
    if exists {
        return Err(Error::DuplicateName(kind));
    }
    Ok(name)
}

fn validate_color(color: &str) -> Result<&'static str> {
    TAG_COLORS
        .iter()
        .copied()
        .find(|candidate| candidate.eq_ignore_ascii_case(color))
        .ok_or(Error::InvalidTagColor)
}

fn require_tag(conn: &Connection, id: &str) -> Result<()> {
    if !conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM tags WHERE id = ?1)",
        [id],
        |row| row.get::<_, bool>(0),
    )? {
        return Err(Error::TagUnavailable);
    }
    Ok(())
}

fn require_group(conn: &Connection, id: &str) -> Result<()> {
    if !conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM tag_groups WHERE id = ?1)",
        [id],
        |row| row.get::<_, bool>(0),
    )? {
        return Err(Error::TagGroupUnavailable);
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
