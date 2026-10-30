use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::{Result, db};

/// 仓库：应用托管的、存放全部表情的唯一位置（ADR 0002）。
///
/// 目录结构：
/// - `library.db`：元数据
/// - `objects/`：表情原文件，按内容哈希命名
/// - `thumbs/`：本机生成的缩略图，不同步
pub struct Library {
    pub(crate) conn: Connection,
    pub(crate) root: PathBuf,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LibraryView {
    All,
    Recent,
    Favorites,
    Untagged,
    RecentlyDeleted,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sticker {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub byte_size: i64,
    pub animated: bool,
    pub added_at: i64,
    pub object_path: PathBuf,
    pub thumbnail_path: PathBuf,
}

/// 侧栏仓库各项的数量。
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
        Ok(Self {
            conn,
            root: root.canonicalize()?,
        })
    }

    pub fn list_stickers(&self, view: LibraryView) -> Result<Vec<Sticker>> {
        let filter = match view {
            LibraryView::All => "s.deleted_at IS NULL",
            LibraryView::Recent => {
                "s.deleted_at IS NULL AND EXISTS (SELECT 1 FROM usages u WHERE u.sticker_id = s.id)"
            }
            LibraryView::Favorites => "s.deleted_at IS NULL AND s.favorite = 1",
            LibraryView::Untagged => {
                "s.deleted_at IS NULL AND NOT EXISTS (SELECT 1 FROM sticker_tags st WHERE st.sticker_id = s.id)"
            }
            LibraryView::RecentlyDeleted => "s.deleted_at IS NOT NULL",
        };
        let order = if matches!(view, LibraryView::Recent) {
            "(SELECT MAX(u.last_used_at) FROM usages u WHERE u.sticker_id = s.id) DESC, s.added_at DESC, s.rowid DESC"
        } else {
            "s.added_at DESC, s.rowid DESC"
        };
        let mut statement = self.conn.prepare(&format!(
            "SELECT s.id, s.name, s.width, s.height, s.byte_size, s.animated, s.added_at, s.hash, s.ext
             FROM stickers s WHERE {filter} ORDER BY {order}"
        ))?;
        let stickers = statement.query_map([], |row| {
            let hash: String = row.get(7)?;
            let ext: String = row.get(8)?;
            Ok(Sticker {
                id: row.get(0)?,
                name: row.get(1)?,
                width: row.get(2)?,
                height: row.get(3)?,
                byte_size: row.get(4)?,
                animated: row.get(5)?,
                added_at: row.get(6)?,
                object_path: self.root.join("objects").join(format!("{hash}.{ext}")),
                thumbnail_path: self.root.join("thumbs").join(format!("{hash}.png")),
            })
        })?;
        Ok(stickers.collect::<rusqlite::Result<_>>()?)
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
    fn lists_views_in_order_and_uses_latest_usage_across_devices() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::open(dir.path()).unwrap();
        library.conn.execute_batch(
            "INSERT INTO stickers (id, hash, ext, name, favorite, width, height, byte_size, added_at, deleted_at) VALUES
                ('plain', 'h1', 'png', '收到', 0, 1, 1, 1, 30, NULL),
                ('fav', 'h2', 'gif', '晚安', 1, 1, 1, 1, 20, NULL),
                ('tagged', 'h3', 'png', '打工', 0, 1, 1, 1, 10, NULL),
                ('deleted', 'h4', 'png', '无语', 1, 1, 1, 1, 40, 50);
             INSERT INTO tags (id, name, color, position) VALUES ('work', '打工', '#3E7BFA', 0);
             INSERT INTO sticker_tags VALUES ('tagged', 'work');
             INSERT INTO usages VALUES
                ('plain', 'mac', 3, 10), ('plain', 'win', 2, 40),
                ('fav', 'mac', 1, 20), ('fav', 'win', 1, 30),
                ('deleted', 'mac', 1, 100);"
        ).unwrap();
        let ids = |view| {
            library
                .list_stickers(view)
                .unwrap()
                .into_iter()
                .map(|s| s.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(LibraryView::All), ["plain", "fav", "tagged"]);
        assert_eq!(ids(LibraryView::Recent), ["plain", "fav"]);
        assert_eq!(ids(LibraryView::Favorites), ["fav"]);
        assert_eq!(ids(LibraryView::Untagged), ["plain", "fav"]);
        assert_eq!(ids(LibraryView::RecentlyDeleted), ["deleted"]);
        let overview = library.overview().unwrap();
        for (view, count) in [
            (LibraryView::All, overview.all),
            (LibraryView::Recent, overview.recent),
            (LibraryView::Favorites, overview.favorites),
            (LibraryView::Untagged, overview.untagged),
            (LibraryView::RecentlyDeleted, overview.recently_deleted),
        ] {
            assert_eq!(ids(view).len(), count as usize);
        }
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
