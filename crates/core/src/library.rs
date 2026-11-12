use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, Row, ToSql, params_from_iter};
use serde::{Deserialize, Serialize};

use crate::{Category, Device, Error, Result, db, deletion::RETENTION_MS};

const STICKER_COLUMNS: &str = "s.id, s.name, s.width, s.height, s.byte_size, s.animated,
    s.added_at, s.hash, s.ext, s.favorite, s.deleted_at,
    COALESCE((SELECT SUM(u.count) FROM usages u WHERE u.sticker_id = s.id), 0),
    (SELECT MAX(u.last_used_at) FROM usages u WHERE u.sticker_id = s.id), s.caption,
    (SELECT COUNT(*) FROM usages u WHERE u.sticker_id = s.id AND u.count > 0)";

/// 仓库：应用托管的、存放全部表情的唯一位置（ADR 0002）。
///
/// 目录结构：
/// - `library.db`：元数据
/// - `objects/`：表情原文件，按内容哈希命名
/// - `thumbs/`：本机生成的缩略图，不同步
/// - `local/device.json`：本机设备身份，不同步
pub struct Library {
    pub(crate) conn: Connection,
    pub(crate) root: PathBuf,
    pub(crate) device: Device,
    pub(crate) files: Arc<Mutex<()>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LibraryView {
    All,
    Recent,
    Favorites,
    Uncategorized,
    RecentlyDeleted,
    Category(String),
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
    pub favorite: bool,
    pub deleted_at: Option<i64>,
    pub purge_at: Option<i64>,
    pub usage_count: i64,
    pub usage_devices: usize,
    pub last_used_at: Option<i64>,
    pub caption: Option<String>,
    pub category: Option<Category>,
    pub object_path: PathBuf,
    pub thumbnail_path: PathBuf,
}

/// 分页位置：上一页最后一张表情的添加时间和 id。
///
/// 不用 rowid：同步投影会清空并重写表情表，rowid 随之改变。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickerCursor {
    pub added_at: i64,
    pub id: String,
}

/// 分页读取时能用的筛选。这几种都按添加时间倒序，分页位置才通用。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StickerPageFilter {
    All,
    Uncategorized,
    Category(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StickerPage {
    pub stickers: Vec<Sticker>,
    /// 还有下一页时，读下一页要传的位置。
    pub next: Option<StickerCursor>,
}

/// 侧栏仓库各项的数量。
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryOverview {
    pub all: u32,
    pub recent: u32,
    pub favorites: u32,
    pub uncategorized: u32,
    pub recently_deleted: u32,
}

impl Library {
    /// 打开 `root` 下的仓库，不存在就新建。
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let conn = db::open(&root.join("library.db"))?;
        fs::create_dir_all(root.join("objects"))?;
        fs::create_dir_all(root.join("thumbs"))?;
        let device = Device::load_or_create(root)?;
        let library = Self {
            conn,
            root: root.canonicalize()?,
            device,
            files: Arc::new(Mutex::new(())),
        };
        library.initialize_sync()?;
        Ok(library)
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn sticker(&self, id: &str) -> Result<Sticker> {
        self.query_stickers("s.id = ?1", "s.id", &[id])?
            .pop()
            .ok_or(Error::StickerUnavailable)
    }

    pub fn sticker_for_use(&self, id: &str) -> Result<Sticker> {
        let sticker = self.sticker(id)?;
        if sticker.deleted_at.is_some() {
            return Err(Error::StickerUnavailable);
        }
        Ok(sticker)
    }

    pub fn list_stickers(&self, view: LibraryView) -> Result<Vec<Sticker>> {
        let filter = match &view {
            LibraryView::All => "s.deleted_at IS NULL",
            LibraryView::Recent => {
                "s.deleted_at IS NULL AND EXISTS (SELECT 1 FROM usages u WHERE u.sticker_id = s.id)"
            }
            LibraryView::Favorites => "s.deleted_at IS NULL AND s.favorite = 1",
            LibraryView::Uncategorized => "s.deleted_at IS NULL AND s.category_id IS NULL",
            LibraryView::RecentlyDeleted => "s.deleted_at IS NOT NULL",
            LibraryView::Category(_) => "s.deleted_at IS NULL AND s.category_id = ?1",
        };
        let order = match view {
            LibraryView::Recent => {
                "(SELECT MAX(u.last_used_at) FROM usages u WHERE u.sticker_id = s.id) DESC, s.added_at DESC, s.rowid DESC"
            }
            LibraryView::RecentlyDeleted => "s.deleted_at DESC, s.added_at DESC, s.rowid DESC",
            _ => "s.added_at DESC, s.rowid DESC",
        };
        let category_id = match &view {
            LibraryView::Category(id) => Some(id.as_str()),
            _ => None,
        };
        self.query_stickers(filter, order, category_id.as_slice())
    }

    /// 按添加时间倒序分页列出符合筛选的未删除表情，`after` 为空时从第一页开始。
    /// 读取期间新导入的表情排在最前面，不会让后面的页重复出现已读过的表情。
    pub fn list_stickers_page(
        &self,
        filter: &StickerPageFilter,
        after: Option<&StickerCursor>,
        limit: u32,
    ) -> Result<StickerPage> {
        let limit = limit.max(1) as usize;
        let (category_filter, category_id) = match filter {
            StickerPageFilter::All => ("", None),
            StickerPageFilter::Uncategorized => ("AND category_id IS NULL", None),
            StickerPageFilter::Category(id) => ("AND category_id = ?4", Some(id)),
        };
        let added_at = after.map(|cursor| cursor.added_at);
        let id = after.map(|cursor| cursor.id.as_str());
        let fetch = limit as i64 + 1;
        let mut parameters: Vec<&dyn ToSql> = vec![&added_at, &id, &fetch];
        if let Some(category_id) = category_id {
            parameters.push(category_id);
        }
        // 多读一张来判断还有没有下一页；先在子查询里分页，使用次数等列只为这一页计算。
        let mut statement = self.conn.prepare(&format!(
            "SELECT {STICKER_COLUMNS} FROM stickers s WHERE s.id IN (
                 SELECT id FROM stickers WHERE deleted_at IS NULL {category_filter}
                     AND (?1 IS NULL OR added_at < ?1 OR (added_at = ?1 AND id < ?2))
                 ORDER BY added_at DESC, id DESC LIMIT ?3)
             ORDER BY s.added_at DESC, s.id DESC"
        ))?;
        let mut stickers = statement
            .query_map(parameters.as_slice(), |row| {
                Sticker::from_row(row, &self.root)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let more = stickers.len() > limit;
        stickers.truncate(limit);
        let ids = serde_json::to_string(&stickers.iter().map(|s| &s.id).collect::<Vec<_>>())?;
        self.attach_categories(
            &mut stickers,
            "s.id IN (SELECT value FROM json_each(?1))",
            [ids],
        )?;
        let next = stickers.last().filter(|_| more).map(|last| StickerCursor {
            added_at: last.added_at,
            id: last.id.clone(),
        });
        Ok(StickerPage { stickers, next })
    }

    pub(crate) fn query_stickers(
        &self,
        filter: &str,
        order: &str,
        parameters: &[&str],
    ) -> Result<Vec<Sticker>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {STICKER_COLUMNS} FROM stickers s WHERE {filter} ORDER BY {order}"
        ))?;
        let mut stickers = statement
            .query_map(params_from_iter(parameters), |row| {
                Sticker::from_row(row, &self.root)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        self.attach_categories(&mut stickers, filter, params_from_iter(parameters))?;
        Ok(stickers)
    }

    /// 一次读取这批表情的分类，避免为每张表情单独查询。`scope` 是限定表情 `s` 的条件。
    fn attach_categories(
        &self,
        stickers: &mut [Sticker],
        scope: &str,
        parameters: impl rusqlite::Params,
    ) -> Result<()> {
        if stickers.is_empty() {
            return Ok(());
        }
        let indexes: HashMap<_, _> = stickers
            .iter()
            .enumerate()
            .map(|(i, sticker)| (sticker.id.clone(), i))
            .collect();
        let mut statement = self.conn.prepare(&format!(
            "SELECT c.id, c.name, c.color, c.position, s.id
             FROM categories c JOIN stickers s ON s.category_id = c.id
             WHERE {scope}"
        ))?;
        let categories = statement.query_map(parameters, |row| {
            Ok((row.get::<_, String>(4)?, Category::from_row(row)?))
        })?;
        for category in categories {
            let (sticker_id, category) = category?;
            if let Some(&index) = indexes.get(&sticker_id) {
                stickers[index].category = Some(category);
            }
        }
        Ok(())
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
            uncategorized: count(
                "SELECT COUNT(*) FROM stickers WHERE deleted_at IS NULL AND category_id IS NULL",
            )?,
            recently_deleted: count("SELECT COUNT(*) FROM stickers WHERE deleted_at IS NOT NULL")?,
        })
    }
}

impl Sticker {
    fn from_row(row: &Row<'_>, root: &Path) -> rusqlite::Result<Self> {
        let hash: String = row.get(7)?;
        let ext: String = row.get(8)?;
        let deleted_at: Option<i64> = row.get(10)?;
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            width: row.get(2)?,
            height: row.get(3)?,
            byte_size: row.get(4)?,
            animated: row.get(5)?,
            added_at: row.get(6)?,
            favorite: row.get(9)?,
            deleted_at,
            purge_at: deleted_at.map(|time| time + RETENTION_MS),
            usage_count: row.get(11)?,
            usage_devices: row.get::<_, u32>(14)? as usize,
            last_used_at: row.get(12)?,
            caption: row.get(13)?,
            category: None,
            object_path: root.join("objects").join(format!("{hash}.{ext}")),
            thumbnail_path: root.join("thumbs").join(format!("{hash}.png")),
        })
    }

    pub fn read_image(&self) -> Result<image::DynamicImage> {
        read_image(&self.object_path)
    }
}

/// 使用与导入共用解码逻辑，动图读取第一帧，保留原始尺寸。也用于读导出的副本。
pub fn read_image(path: &Path) -> Result<image::DynamicImage> {
    let format = image::ImageReader::open(path)?
        .with_guessed_format()?
        .format()
        .ok_or(Error::EmptyImage)?;
    Ok(crate::import::decode(path, format)?.0)
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
    fn invalid_database_does_not_create_device_or_object_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 10000).unwrap();
        drop(conn);
        assert!(matches!(
            Library::open(dir.path()),
            Err(Error::SchemaTooNew { .. })
        ));
        assert!(!dir.path().join("local").exists());
        assert!(!dir.path().join("objects").exists());
        assert!(!dir.path().join("thumbs").exists());
    }

    #[test]
    fn lists_views_in_order_and_uses_latest_usage_across_devices() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::open(dir.path()).unwrap();
        library.conn.execute_batch(
            "INSERT INTO stickers (id, hash, ext, name, favorite, width, height, byte_size, added_at, deleted_at) VALUES
                ('plain', 'h1', 'png', '收到', 0, 1, 1, 1, 30, NULL),
                ('fav', 'h2', 'gif', '晚安', 1, 1, 1, 1, 20, NULL),
                ('categorized', 'h3', 'png', '打工', 0, 1, 1, 1, 10, NULL),
                ('deleted', 'h4', 'png', '无语', 1, 1, 1, 1, 40, 50);
             INSERT INTO categories (id, name, color, position) VALUES ('work', '打工', '#3E7BFA', 0);
             UPDATE stickers SET category_id = 'work' WHERE id = 'categorized';
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
        assert_eq!(ids(LibraryView::All), ["plain", "fav", "categorized"]);
        assert_eq!(ids(LibraryView::Recent), ["plain", "fav"]);
        assert_eq!(ids(LibraryView::Favorites), ["fav"]);
        assert_eq!(ids(LibraryView::Uncategorized), ["plain", "fav"]);
        assert_eq!(ids(LibraryView::RecentlyDeleted), ["deleted"]);
        let overview = library.overview().unwrap();
        for (view, count) in [
            (LibraryView::All, overview.all),
            (LibraryView::Recent, overview.recent),
            (LibraryView::Favorites, overview.favorites),
            (LibraryView::Uncategorized, overview.uncategorized),
            (LibraryView::RecentlyDeleted, overview.recently_deleted),
        ] {
            assert_eq!(ids(view).len(), count as usize);
        }
    }

    #[test]
    fn pages_follow_added_time_and_new_imports_do_not_repeat_read_stickers() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::open(dir.path()).unwrap();
        library.conn.execute_batch(
            "INSERT INTO stickers (id, hash, ext, name, width, height, byte_size, added_at, deleted_at) VALUES
                ('a', 'h1', 'png', 'a', 1, 1, 1, 50, NULL),
                ('c', 'h2', 'png', 'c', 1, 1, 1, 40, NULL),
                ('b', 'h3', 'png', 'b', 1, 1, 1, 40, NULL),
                ('gone', 'h4', 'png', 'gone', 1, 1, 1, 35, 60),
                ('d', 'h5', 'png', 'd', 1, 1, 1, 30, NULL),
                ('e', 'h6', 'png', 'e', 1, 1, 1, 20, NULL);
             INSERT INTO categories (id, name, color, position) VALUES ('work', '打工', '#3E7BFA', 0);
             UPDATE stickers SET category_id = 'work' WHERE id = 'b';",
        ).unwrap();
        let ids = |page: &StickerPage| {
            page.stickers
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>()
        };

        let first = library
            .list_stickers_page(&StickerPageFilter::All, None, 2)
            .unwrap();
        assert_eq!(ids(&first), ["a", "c"]);
        // 第一页读完后才导入的表情排在最前，不会出现在后面的页里。
        library.conn.execute(
            "INSERT INTO stickers (id, hash, ext, name, width, height, byte_size, added_at) VALUES ('new', 'h7', 'png', 'new', 1, 1, 1, 99)",
            [],
        ).unwrap();
        let second = library
            .list_stickers_page(&StickerPageFilter::All, first.next.as_ref(), 2)
            .unwrap();
        assert_eq!(ids(&second), ["b", "d"]);
        assert_eq!(second.stickers[0].category.as_ref().unwrap().name, "打工");
        let last = library
            .list_stickers_page(&StickerPageFilter::All, second.next.as_ref(), 2)
            .unwrap();
        assert_eq!(ids(&last), ["e"]);
        assert_eq!(last.next, None);
        // 刚好读完一整页时也知道没有下一页，不用再请求一次空页。
        assert_eq!(
            library
                .list_stickers_page(&StickerPageFilter::All, None, 6)
                .unwrap()
                .next,
            None
        );
    }

    #[test]
    fn pages_only_contain_stickers_matching_the_filter() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::open(dir.path()).unwrap();
        library.conn.execute_batch(
            "INSERT INTO categories (id, name, color, position) VALUES
                ('work', '打工', '#3E7BFA', 0),
                ('cat', '猫', '#3E7BFA', 1);
             INSERT INTO stickers (id, hash, ext, name, width, height, byte_size, added_at, deleted_at, category_id) VALUES
                ('a', 'h1', 'png', 'a', 1, 1, 1, 50, NULL, 'work'),
                ('b', 'h2', 'png', 'b', 1, 1, 1, 40, NULL, NULL),
                ('c', 'h3', 'png', 'c', 1, 1, 1, 30, NULL, 'work'),
                ('d', 'h4', 'png', 'd', 1, 1, 1, 20, NULL, 'cat'),
                ('e', 'h5', 'png', 'e', 1, 1, 1, 10, NULL, 'work'),
                ('gone', 'h6', 'png', 'gone', 1, 1, 1, 5, 60, 'work'),
                ('lost', 'h7', 'png', 'lost', 1, 1, 1, 4, 60, NULL);",
        ).unwrap();
        let ids = |page: &StickerPage| {
            page.stickers
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>()
        };

        let work = StickerPageFilter::Category("work".into());
        let first = library.list_stickers_page(&work, None, 2).unwrap();
        assert_eq!(ids(&first), ["a", "c"]);
        // 分页位置在筛选后的列表里接着往下读，不会混进其他分类。
        let second = library
            .list_stickers_page(&work, first.next.as_ref(), 2)
            .unwrap();
        assert_eq!(ids(&second), ["e"]);
        assert_eq!(second.next, None);

        let uncategorized = library
            .list_stickers_page(&StickerPageFilter::Uncategorized, None, 10)
            .unwrap();
        assert_eq!(ids(&uncategorized), ["b"]);

        let empty = StickerPageFilter::Category("missing".into());
        let page = library.list_stickers_page(&empty, None, 10).unwrap();
        assert!(page.stickers.is_empty());
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
                     ('categorized',  'h3', 'png', '打工',   0, 1, 1, 1, 0, NULL),
                     ('deleted', 'h4', 'png', '无语',   1, 1, 1, 1, 0, 1000);
                 INSERT INTO categories (id, name, color, position) VALUES ('work', '打工', '#3E7BFA', 0);
                 UPDATE stickers SET category_id = 'work' WHERE id IN ('categorized', 'deleted');
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
                uncategorized: 2,
                recently_deleted: 1
            }
        );
    }
}
