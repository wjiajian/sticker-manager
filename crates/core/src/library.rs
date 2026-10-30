use std::fs;
use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::{Result, db};

/// 仓库：应用托管的、存放全部表情的唯一位置（ADR 0002）。
///
/// 目录结构：
/// - `library.db`：元数据
/// - `objects/`：表情原文件，按内容哈希命名
/// - `thumbs/`：本机生成的缩略图，不同步
pub struct Library {
    conn: Connection,
}

/// 侧栏"资料库"一节各项的数量。
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryOverview {
    pub all: u32,
    pub recent: u32,
    pub favorites: u32,
    pub untagged: u32,
    pub recently_deleted: u32,
}

impl Library {
    /// 打开 `root` 下的仓库，不存在就新建。
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root.join("objects"))?;
        fs::create_dir_all(root.join("thumbs"))?;
        let conn = db::open(&root.join("library.db"))?;
        Ok(Self { conn })
    }

    pub fn overview(&self) -> Result<LibraryOverview> {
        let count = |sql: &str| self.conn.query_row(sql, [], |row| row.get::<_, u32>(0));
        Ok(LibraryOverview {
            all: count("SELECT COUNT(*) FROM stickers WHERE deleted_at IS NULL")?,
            recent: count(
                "SELECT COUNT(DISTINCT s.id) FROM stickers s JOIN usages u ON u.sticker_id = s.id
                 WHERE s.deleted_at IS NULL",
            )?,
            favorites: count(
                "SELECT COUNT(*) FROM stickers WHERE deleted_at IS NULL AND favorite = 1",
            )?,
            untagged: count(
                "SELECT COUNT(*) FROM stickers s WHERE s.deleted_at IS NULL
                 AND NOT EXISTS (SELECT 1 FROM sticker_tags st WHERE st.sticker_id = s.id)",
            )?,
            recently_deleted: count("SELECT COUNT(*) FROM stickers WHERE deleted_at IS NOT NULL")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_layout_with_empty_overview() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::open(dir.path()).unwrap();

        assert!(dir.path().join("library.db").is_file());
        assert!(dir.path().join("objects").is_dir());
        assert!(dir.path().join("thumbs").is_dir());
        assert_eq!(library.overview().unwrap(), LibraryOverview::default());
    }

    #[test]
    fn overview_counts_each_section() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::open(dir.path()).unwrap();
        library
            .conn
            .execute_batch(
                "INSERT INTO stickers (id, hash, ext, name, favorite, width, height, byte_size, added_at, deleted_at) VALUES
                     ('plain',   'h1', 'png', '收到',   0, 1, 1, 1, 0, NULL),
                     ('fav',     'h2', 'gif', '晚安',   1, 1, 1, 1, 0, NULL),
                     ('tagged',  'h3', 'png', '打工',   0, 1, 1, 1, 0, NULL),
                     ('deleted', 'h4', 'png', '无语',   1, 1, 1, 1, 0, 1000);
                 INSERT INTO tags (id, name, color, position) VALUES ('work', '打工', '#3E7BFA', 0);
                 INSERT INTO sticker_tags VALUES ('tagged', 'work'), ('deleted', 'work');
                 INSERT INTO usages VALUES
                     ('plain',   'mac', 3, 10), ('plain', 'win', 2, 20),
                     ('deleted', 'mac', 1, 30);",
            )
            .unwrap();

        assert_eq!(
            library.overview().unwrap(),
            LibraryOverview {
                all: 3,
                recent: 1,
                favorites: 1,
                untagged: 2,
                recently_deleted: 1
            }
        );
    }
}
