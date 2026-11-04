use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn png(color: u8) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    RgbaImage::from_pixel(8, 6, Rgba([color, 0, 0, 255]))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
struct Counted {
    inner: MemoryStorage,
    reads: Mutex<Vec<String>>,
}
impl Storage for Counted {
    fn list(&self, p: &str) -> Result<Vec<storage::FileEntry>> {
        self.inner.list(p)
    }
    fn read(&self, p: &str) -> Result<Option<Vec<u8>>> {
        self.reads.lock().unwrap().push(p.into());
        self.inner.read(p)
    }
    fn write(&self, p: &str, b: &[u8]) -> Result<()> {
        self.inner.write(p, b)
    }
    fn delete(&self, p: &str) -> Result<()> {
        self.inner.delete(p)
    }
}
#[test]
fn same_size_replacement_changes_hash_and_unchanged_second_scan_reads_no_image() {
    let (inner, m) = backend();
    let a = Site::new();
    let s = Counted {
        inner,
        reads: Mutex::new(Vec::new()),
    };
    let first = png(1);
    let mut second = png(2);
    // PNG 解码允许尾部数据，补齐尺寸使测试独立于压缩器选择。
    let length = first.len().max(second.len());
    let mut first = first;
    first.resize(length, 0);
    second.resize(length, 0);
    s.write("中文 分类/收到.png", &first).unwrap();
    a.sync(&s, &m);
    let before = a
        .lib
        .lock()
        .unwrap()
        .list_stickers(LibraryView::All)
        .unwrap()[0]
        .object_path
        .clone();
    s.reads.lock().unwrap().clear();
    a.sync(&s, &m);
    assert!(
        !s.reads
            .lock()
            .unwrap()
            .iter()
            .any(|p| paths::image(p).is_some())
    );
    assert!(
        !s.reads
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.ends_with(".jsonl"))
    );
    s.write("中文 分类/收到.png", &second).unwrap();
    a.sync(&s, &m);
    let l = a.lib.lock().unwrap();
    let all = l.list_stickers(LibraryView::All).unwrap();
    assert_eq!(all.len(), 1);
    assert_ne!(all[0].object_path, before);
    assert_eq!(all[0].category.as_ref().unwrap().name, "中文 分类");
    assert_eq!(
        l.list_stickers(LibraryView::RecentlyDeleted).unwrap().len(),
        1
    );
}
#[test]
fn manual_duplicate_uses_utf8_path_order_and_leaves_other_copy() {
    let (s, m) = backend();
    let a = Site::new();
    s.write("乙/图片.png", &png(3)).unwrap();
    s.write("甲/图片.png", &png(3)).unwrap();
    settle(&[&a], &s, &m);
    let l = a.lib.lock().unwrap();
    let all = l.list_stickers(LibraryView::All).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].category.as_ref().unwrap().name, "乙");
    assert_eq!(remote_images(&s).len(), 2);
}
#[test]
fn whole_folder_rename_preserves_identity_caption_and_usage() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let id = a.import(11);
    a.lib.lock().unwrap().categorize(&id, "旧目录").unwrap();
    a.lib.lock().unwrap().record_usage(&id).unwrap();
    a.lib
        .lock()
        .unwrap()
        .set_sticker_caption(&id, "保留")
        .unwrap();
    settle(&[&a, &b], &s, &m);
    let old = remote_key(&a, &id);
    let bytes = s.read(&old).unwrap().unwrap();
    s.write("新目录/改名.png", &bytes).unwrap();
    s.delete(&old).unwrap();
    settle(&[&a, &b], &s, &m);
    let l = a.lib.lock().unwrap();
    let sticker = l.sticker(&id).unwrap();
    assert_eq!(sticker.name, "改名");
    assert_eq!(sticker.category.unwrap().name, "新目录");
    assert_eq!(sticker.caption.as_deref(), Some("保留"));
    assert_eq!(sticker.usage_count, 1);
    drop(l);
    assert_eq!(a.state(), b.state());
}
#[test]
fn remote_path_wins_over_unpublished_local_category_and_name() {
    let (s, m) = backend();
    let a = Site::new();
    let id = a.import(12);
    a.sync(&s, &m);
    a.lib.lock().unwrap().categorize(&id, "本机").unwrap();
    a.lib
        .lock()
        .unwrap()
        .conn
        .execute("UPDATE stickers SET name='本机名称' WHERE id=?1", [&id])
        .unwrap();
    let old = remote_key(&a, &id);
    let bytes = s.read(&old).unwrap().unwrap();
    s.write("远端/远端名称.png", &bytes).unwrap();
    s.delete(&old).unwrap();
    a.sync(&s, &m);
    let l = a.lib.lock().unwrap();
    let sticker = l.sticker(&id).unwrap();
    assert_eq!(sticker.name, "远端名称");
    assert_eq!(sticker.category.unwrap().name, "远端");
    assert_eq!(
        remote_images(&s)
            .iter()
            .map(|f| f.key.as_str())
            .collect::<Vec<_>>(),
        ["远端/远端名称.png"]
    );
}
#[test]
fn app_delete_moves_to_trash_restore_moves_back_and_purge_removes_trash() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let id = a.import(13);
    a.lib.lock().unwrap().categorize(&id, "收藏夹").unwrap();
    settle(&[&a, &b], &s, &m);
    let path = remote_key(&a, &id);
    let trash = format!(".sticker-manager/trash/{id}.png");
    a.lib.lock().unwrap().delete_sticker(&id).unwrap();
    settle(&[&a, &b], &s, &m);
    assert!(s.read(&path).unwrap().is_none());
    assert!(s.read(&trash).unwrap().is_some());
    b.lib.lock().unwrap().restore_sticker(&id).unwrap();
    settle(&[&b, &a], &s, &m);
    assert!(s.read(&path).unwrap().is_some());
    assert!(s.read(&trash).unwrap().is_none());
    a.lib.lock().unwrap().delete_sticker(&id).unwrap();
    a.sync(&s, &m);
    a.lib.lock().unwrap().purge_sticker(&id).unwrap();
    settle(&[&a, &b], &s, &m);
    assert!(s.read(&trash).unwrap().is_none());
    assert!(remote_images(&s).is_empty());
}
#[test]
fn failed_delete_after_copy_retries_without_losing_image_or_leaving_duplicate() {
    struct Fail {
        inner: MemoryStorage,
        fail: AtomicBool,
    }
    impl Storage for Fail {
        fn list(&self, p: &str) -> Result<Vec<storage::FileEntry>> {
            self.inner.list(p)
        }
        fn read(&self, p: &str) -> Result<Option<Vec<u8>>> {
            self.inner.read(p)
        }
        fn write(&self, p: &str, b: &[u8]) -> Result<()> {
            self.inner.write(p, b)
        }
        fn delete(&self, p: &str) -> Result<()> {
            if paths::image(p).is_some() && self.fail.swap(false, Ordering::SeqCst) {
                return Err(Error::Sync("模拟移动中断".into()));
            }
            self.inner.delete(p)
        }
    }
    let (inner, m) = backend();
    let s = Fail {
        inner,
        fail: AtomicBool::new(false),
    };
    let a = Site::new();
    let id = a.import(14);
    a.sync(&s, &m);
    let category = a.lib.lock().unwrap().categorize(&id, "分类").unwrap();
    a.sync(&s, &m);
    a.lib
        .lock()
        .unwrap()
        .rename_category(&category.id, "新分类")
        .unwrap();
    s.fail.store(true, Ordering::SeqCst);
    assert!(synchronize(&a.lib, &s, &m.id).is_err());
    assert_eq!(remote_images(&s).len(), 2);
    a.sync(&s, &m);
    assert_eq!(remote_images(&s).len(), 1);
    assert!(s.read("新分类/14.png").unwrap().is_some());
    a.lib
        .lock()
        .unwrap()
        .conn
        .execute("UPDATE stickers SET name='新名称' WHERE id=?1", [&id])
        .unwrap();
    s.fail.store(true, Ordering::SeqCst);
    assert!(synchronize(&a.lib, &s, &m.id).is_err());
    a.sync(&s, &m);
    assert_eq!(remote_images(&s).len(), 1);
    assert!(s.read("新分类/新名称.png").unwrap().is_some());
}
#[test]
fn old_format_is_rejected_and_hidden_or_temporary_images_are_ignored() {
    let s = MemoryStorage::default();
    s.write("library.json", br#"{"format":1}"#).unwrap();
    assert!(
        Manifest::read(&s)
            .unwrap_err()
            .to_string()
            .contains("重新创建同步仓库")
    );
    let (s, m) = backend();
    let a = Site::new();
    for p in [
        ".隐藏/a.png",
        "分类/.a.png",
        "分类/a.png.icloud",
        "分类/a (conflict).png",
        "分类/a.tmp",
        "分类/嵌套/a.png",
    ] {
        s.write(p, &png(4)).unwrap();
    }
    a.sync(&s, &m);
    assert_eq!(a.lib.lock().unwrap().overview().unwrap().all, 0);
}
#[test]
fn logs_never_publish_sticker_category_id_and_cache_is_local_only() {
    let (s, m) = backend();
    let a = Site::new();
    let id = a.import(15);
    a.lib.lock().unwrap().categorize(&id, "测试").unwrap();
    a.sync(&s, &m);
    for segment in segments(&s).unwrap() {
        let bytes = s.read(&segment.key).unwrap().unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("category_id"));
        assert!(!text.contains("sync_path_cache"));
    }
    assert!(s.list("objects/").unwrap().is_empty());
}

#[test]
fn empty_folder_creates_category_and_emptying_it_keeps_category() {
    let dir = tempfile::tempdir().unwrap();
    let s = FolderStorage::new(dir.path()).unwrap();
    let m = Manifest::create(&s).unwrap();
    let a = Site::new();
    fs::create_dir(dir.path().join("空分类")).unwrap();
    a.sync(&s, &m);
    let category = a
        .lib
        .lock()
        .unwrap()
        .categories()
        .unwrap()
        .remove(0)
        .category;
    assert_eq!(category.name, "空分类");
    fs::remove_dir(dir.path().join("空分类")).unwrap();
    a.sync(&s, &m);
    assert_eq!(
        a.lib.lock().unwrap().categories().unwrap()[0].category.id,
        category.id
    );
}
#[test]
fn missing_version_uses_size_cache_without_polling_reads() {
    struct NoVersion(Counted);
    impl Storage for NoVersion {
        fn list(&self, p: &str) -> Result<Vec<storage::FileEntry>> {
            Ok(self
                .0
                .list(p)?
                .into_iter()
                .map(|mut f| {
                    f.version = None;
                    f
                })
                .collect())
        }
        fn read(&self, p: &str) -> Result<Option<Vec<u8>>> {
            self.0.read(p)
        }
        fn write(&self, p: &str, b: &[u8]) -> Result<()> {
            self.0.write(p, b)
        }
        fn delete(&self, p: &str) -> Result<()> {
            self.0.delete(p)
        }
    }
    let (inner, m) = backend();
    let s = NoVersion(Counted {
        inner,
        reads: Mutex::new(Vec::new()),
    });
    let a = Site::new();
    let mut first = png(21);
    let mut second = png(22);
    let n = first.len().max(second.len());
    first.resize(n, 0);
    second.resize(n, 0);
    s.write("无版本.png", &first).unwrap();
    a.sync(&s, &m);
    s.0.reads.lock().unwrap().clear();
    s.write("无版本.png", &second).unwrap();
    a.sync(&s, &m);
    assert!(
        !s.0.reads
            .lock()
            .unwrap()
            .iter()
            .any(|p| paths::image(p).is_some())
    );
    let l = a.lib.lock().unwrap();
    let original = &l.list_stickers(LibraryView::All).unwrap()[0].object_path;
    assert_eq!(fs::read(original).unwrap(), first);
}
#[test]
fn log_path_hint_populates_cache_without_hash_download_when_original_is_local() {
    let (inner, m) = backend();
    let a = Site::new();
    let b = Site::new();
    a.import(23);
    b.import(23);
    a.sync(&inner, &m);
    let s = Counted {
        inner,
        reads: Mutex::new(Vec::new()),
    };
    b.sync(&s, &m);
    assert!(
        !s.reads
            .lock()
            .unwrap()
            .iter()
            .any(|p| paths::image(p).is_some())
    );
    assert_eq!(
        b.lib
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT COUNT(*) FROM sync_path_cache", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn same_name_different_content_gets_numbered_filename() {
    let (s, m) = backend();
    let a = Site::new();
    let first = a.import(24);
    let second = a.import(25);
    a.lib
        .lock()
        .unwrap()
        .set_sticker_name(&first, "同名")
        .unwrap();
    a.lib
        .lock()
        .unwrap()
        .set_sticker_name(&second, "同名")
        .unwrap();
    a.sync(&s, &m);
    let files = remote_images(&s);
    assert_eq!(
        files.iter().map(|f| f.key.as_str()).collect::<Vec<_>>(),
        ["同名 (2).png", "同名.png"]
    );
    assert_ne!(s.read("同名.png").unwrap(), s.read("同名 (2).png").unwrap());
}
#[test]
fn path_rules_support_chinese_spaces_and_reject_hidden_and_traversal() {
    for key in ["中文 分类/收到 了.png", ".sticker-manager/library.json"] {
        storage::validate_key(key).unwrap();
    }
    for key in [
        ".隐藏/a.png",
        "分类/.隐藏.png",
        "a..b.png",
        "a\\b.png",
        "../a.png",
    ] {
        assert!(storage::validate_key(key).is_err());
    }
}

#[test]
fn creating_manifest_cannot_replace_existing_empty_library() {
    let (s, m) = backend();
    assert!(Manifest::create(&s).is_err());
    assert_eq!(Manifest::read(&s).unwrap(), m);
}

#[test]
fn migration_keeps_clocks_tombstones_aliases_and_removes_old_category_operations() {
    let a = Site::new();
    let id = a.import(26);
    let (revision, mut model) = {
        let mut l = a.lib.lock().unwrap();
        l.maintain_sync(None).unwrap();
        store::load(&l.conn).unwrap()
    };
    model
        .entities
        .get_mut(&Kind::Sticker)
        .unwrap()
        .get_mut(&id)
        .unwrap()
        .fields
        .insert(
            "category_id".into(),
            model::Register {
                value: json!("old-category"),
                stamp: Stamp {
                    at: 99,
                    device: "old-device".into(),
                },
            },
        );
    model
        .tombstones
        .entry(Kind::Sticker)
        .or_default()
        .insert("purged".into());
    model
        .aliases
        .entry(Kind::Sticker)
        .or_default()
        .insert("z".into(), "a".into());
    let expected = model.entities[&Kind::Sticker][&id].fields["name"].clone();
    let root = a.lib.lock().unwrap().root.clone();
    drop(a.lib);
    let conn = rusqlite::Connection::open(root.join("library.db")).unwrap();
    store::save(&conn, revision, &model).unwrap();
    // 还原 0004 的触发器和表结构，再让正常打开路径执行新迁移。
    conn.execute_batch("DROP TRIGGER sync_stickers_file; DROP TRIGGER sync_categories_file; DROP TRIGGER sync_stickers_create; DROP TABLE sync_path_cache; DROP TABLE sync_file_moves; DROP TABLE sync_file_dirty; DROP TABLE sync_file_revision; DROP TABLE sync_directory_cache;").unwrap();
    let v4 = include_str!("../../migrations/0004_categories.sql");
    let triggers = &v4[v4.find("CREATE TRIGGER sync_stickers_create").unwrap()
        ..v4.find("CREATE TRIGGER sync_categories_create").unwrap()];
    conn.execute_batch(triggers).unwrap();
    let pending = json!([{"at":100,"device":"old-device","type":"set","entity":"sticker","id":id,"field":"category_id","value":"old-category"},{"at":101,"device":"old-device","type":"set","entity":"sticker","id":"later","field":"caption","value":"保留"}]);
    conn.execute(
        "INSERT INTO sync_pending VALUES('old','device',1,?1)",
        [pending.to_string()],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 4).unwrap();
    drop(conn);
    let l = Library::open(&root).unwrap();
    let (_, model) = store::load(&l.conn).unwrap();
    assert_eq!(model.entities[&Kind::Sticker][&id].fields["name"], expected);
    assert!(
        !model.entities[&Kind::Sticker][&id]
            .fields
            .contains_key("category_id")
    );
    assert!(model.dead(Kind::Sticker, "purged"));
    assert_eq!(model.resolve(Kind::Sticker, "z"), "a");
    let pending = store::pending_rows(&l.conn).unwrap();
    assert_eq!(pending[0].3.len(), 1);
    assert!(matches!(&pending[0].3[0].action,Action::Set {field,..} if field=="caption"));
}

#[test]
fn ignored_duplicate_is_untouched_when_selected_image_is_deleted_and_purged() {
    let (s, m) = backend();
    let a = Site::new();
    s.write("A/图.png", &png(27)).unwrap();
    s.write("B/图.png", &png(27)).unwrap();
    a.sync(&s, &m);
    let id = a
        .lib
        .lock()
        .unwrap()
        .list_stickers(LibraryView::All)
        .unwrap()[0]
        .id
        .clone();
    a.lib.lock().unwrap().delete_sticker(&id).unwrap();
    a.sync(&s, &m);
    a.sync(&s, &m);
    assert_eq!(s.read("B/图.png").unwrap(), Some(png(27)));
    a.lib.lock().unwrap().purge_sticker(&id).unwrap();
    a.sync(&s, &m);
    a.sync(&s, &m);
    assert_eq!(s.read("B/图.png").unwrap(), Some(png(27)));
    assert_eq!(a.lib.lock().unwrap().overview().unwrap().all, 0);
}
#[test]
fn deleting_category_removes_empty_remote_folder_without_removing_sticker() {
    let dir = tempfile::tempdir().unwrap();
    let s = FolderStorage::new(dir.path()).unwrap();
    let m = Manifest::create(&s).unwrap();
    let a = Site::new();
    let id = a.import(28);
    let category = a.lib.lock().unwrap().categorize(&id, "旧分类").unwrap();
    a.sync(&s, &m);
    a.sync(&s, &m);
    a.lib.lock().unwrap().delete_category(&category.id).unwrap();
    a.sync(&s, &m);
    assert!(!dir.path().join("旧分类").exists());
    assert!(s.read("28.png").unwrap().is_some());
}

#[test]
fn editing_name_during_upload_remains_pending_for_next_round() {
    struct Editing<'a> {
        inner: MemoryStorage,
        site: &'a Site,
        id: String,
        once: AtomicBool,
    }
    impl Storage for Editing<'_> {
        fn list(&self, p: &str) -> Result<Vec<storage::FileEntry>> {
            self.inner.list(p)
        }
        fn read(&self, p: &str) -> Result<Option<Vec<u8>>> {
            self.inner.read(p)
        }
        fn write(&self, p: &str, b: &[u8]) -> Result<()> {
            self.inner.write(p, b)?;
            if paths::image(p).is_some() && self.once.swap(false, Ordering::SeqCst) {
                self.site
                    .lib
                    .lock()
                    .unwrap()
                    .set_sticker_name(&self.id, "传输期间修改")?;
            }
            Ok(())
        }
        fn delete(&self, p: &str) -> Result<()> {
            self.inner.delete(p)
        }
    }
    let (inner, m) = backend();
    let a = Site::new();
    let id = a.import(29);
    let s = Editing {
        inner,
        site: &a,
        id: id.clone(),
        once: AtomicBool::new(true),
    };
    a.sync(&s, &m);
    a.sync(&s, &m);
    assert_eq!(
        remote_images(&s)
            .iter()
            .map(|f| f.key.as_str())
            .collect::<Vec<_>>(),
        ["传输期间修改.png"]
    );
    assert_eq!(
        a.lib.lock().unwrap().sticker(&id).unwrap().name,
        "传输期间修改"
    );
}

#[test]
fn app_empty_category_is_visible_as_folder_and_manual_import_counts_as_arrival() {
    let (s, m) = backend();
    let a = Site::new();
    a.sync(&s, &m);
    let category = a
        .lib
        .lock()
        .unwrap()
        .create_category("空目录", None)
        .unwrap();
    a.sync(&s, &m);
    assert!(s.list("").unwrap().iter().any(|f| f.key == "空目录/"));
    s.write("空目录/手建.png", &png(30)).unwrap();
    let report = a.sync(&s, &m);
    assert_eq!(report.new_stickers, 1);
    let l = a.lib.lock().unwrap();
    assert_eq!(
        l.list_stickers(LibraryView::All).unwrap()[0]
            .category
            .as_ref()
            .unwrap()
            .id,
        category.id
    );
}

#[test]
fn sanitized_names_round_trip_and_manual_input_is_strict() {
    let long = "中文".repeat(200);
    for name in [
        "a..b",
        "my conflict",
        "收到（冲突副本）",
        "冲突冲突副本",
        "   ",
        ".icloud",
        "...",
        "a/b\\c\n",
        "CONFLICT",
        &long,
    ] {
        for category in [None, Some("分类")] {
            for ext in ["png", "jpg", "gif", "webp"] {
                for n in [1, 2, 12345] {
                    let key = paths::key(category, name, ext, n).unwrap();
                    let (actual, _, _) = paths::image(&key).unwrap();
                    assert_eq!(actual, category, "{key}");
                    assert!(key.rsplit('/').next().unwrap().len() <= 200);
                }
            }
        }
    }
    let site = Site::new();
    let id = site.import(80);
    let lib = site.lib.lock().unwrap();
    for name in ["", "  ", "a/b", "a\\b", "a\nb", ".a", "a..b"] {
        assert!(lib.set_sticker_name(&id, name).is_err(), "{name:?}");
    }
    for name in ["my conflict", "收到（冲突副本）", &long] {
        lib.set_sticker_name(&id, name).unwrap();
    }
}

#[test]
fn folder_case_collision_keeps_both_images_on_both_devices() {
    let dir = tempfile::tempdir().unwrap();
    let s = FolderStorage::new(dir.path()).unwrap();
    let m = Manifest::create(&s).unwrap();
    let a = Site::new();
    let b = Site::new();
    let first = a.import(81);
    let second = a.import(82);
    {
        let l = a.lib.lock().unwrap();
        l.set_sticker_name(&first, "A").unwrap();
        l.set_sticker_name(&second, "a").unwrap();
    }
    for _ in 0..3 {
        a.sync(&s, &m);
        b.sync(&s, &m);
    }
    let files = s
        .list("")
        .unwrap()
        .into_iter()
        .filter(|f| paths::image(&f.key).is_some())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 2);
    assert_ne!(files[0].key.to_lowercase(), files[1].key.to_lowercase());
    for site in [&a, &b] {
        assert_eq!(
            site.lib
                .lock()
                .unwrap()
                .list_stickers(LibraryView::All)
                .unwrap()
                .len(),
            2
        );
    }
    assert_eq!(a.state(), b.state());
}

struct Listed {
    inner: MemoryStorage,
    lists: Mutex<usize>,
}
impl Storage for Listed {
    fn list(&self, p: &str) -> Result<Vec<storage::FileEntry>> {
        if p.is_empty() {
            *self.lists.lock().unwrap() += 1;
        }
        self.inner.list(p)
    }
    fn read(&self, p: &str) -> Result<Option<Vec<u8>>> {
        self.inner.read(p)
    }
    fn write(&self, p: &str, b: &[u8]) -> Result<()> {
        self.inner.write(p, b)
    }
    fn delete(&self, p: &str) -> Result<()> {
        self.inner.delete(p)
    }
}

#[test]
fn idle_many_images_does_not_rewrite_stickers_and_lists_are_constant() {
    let (inner, m) = backend();
    let s = Listed {
        inner,
        lists: Mutex::new(0),
    };
    let a = Site::new();
    let b = Site::new();
    for n in 0..48 {
        a.import(n);
    }
    a.sync(&s, &m);
    assert_eq!(
        a.lib
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT count(*) FROM sync_file_dirty", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(*s.lists.lock().unwrap() <= 5);
    *s.lists.lock().unwrap() = 0;
    b.sync(&s, &m);
    assert!(*s.lists.lock().unwrap() <= 5);
    b.lib.lock().unwrap().conn.execute_batch("CREATE TABLE sticker_writes(n INTEGER); INSERT INTO sticker_writes VALUES(0); CREATE TRIGGER count_insert AFTER INSERT ON stickers BEGIN UPDATE sticker_writes SET n=n+1; END; CREATE TRIGGER count_update AFTER UPDATE ON stickers BEGIN UPDATE sticker_writes SET n=n+1; END; CREATE TRIGGER count_delete AFTER DELETE ON stickers BEGIN UPDATE sticker_writes SET n=n+1; END;").unwrap();
    *s.lists.lock().unwrap() = 0;
    b.sync(&s, &m);
    assert_eq!(
        b.lib
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT n FROM sticker_writes", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(*s.lists.lock().unwrap() <= 4);
}

#[test]
#[ignore = "800 张表情同步性能，使用 release 显式运行"]
fn tree_800_images_sync_performance() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    for n in 0u32..800 {
        let path = a.dir.path().join(format!("{n}.png"));
        RgbaImage::from_pixel(8, 6, Rgba([(n & 255) as u8, (n >> 8) as u8, 0, 255]))
            .save(&path)
            .unwrap();
        let result = a.lib.lock().unwrap().import_paths(&[path]);
        assert!(result.failures.is_empty());
    }
    for (label, site) in [
        ("A first", &a),
        ("B first", &b),
        ("A idle", &a),
        ("B idle", &b),
    ] {
        let start = std::time::Instant::now();
        site.sync(&s, &m);
        let elapsed = start.elapsed();
        println!("{label}: {elapsed:?}");
        if label.ends_with("idle") {
            assert!(elapsed.as_secs_f64() < 0.5, "{label}: {elapsed:?}");
        }
    }
}

#[test]
fn unsafe_import_names_and_missing_original_do_not_block_other_uploads() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    for (i, name) in ["a..b.png", "my conflict.png", "收到（冲突副本）.png"]
        .iter()
        .enumerate()
    {
        let path = a.dir.path().join(name);
        fs::write(&path, png(90 + i as u8)).unwrap();
        assert!(
            a.lib
                .lock()
                .unwrap()
                .import_paths(&[path])
                .failures
                .is_empty()
        );
    }
    let missing = a.import(99);
    let object = a.lib.lock().unwrap().sticker(&missing).unwrap().object_path;
    fs::remove_file(object).unwrap();
    a.sync(&s, &m);
    b.sync(&s, &m);
    a.sync(&s, &m);
    assert_eq!(remote_images(&s).len(), 3);
    let l = b.lib.lock().unwrap();
    assert_eq!(l.list_stickers(LibraryView::All).unwrap().len(), 3);
    assert!(
        l.list_stickers(LibraryView::RecentlyDeleted)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn file_history_is_bounded_order_independent_and_purge_dominates() {
    use super::model::{Action, Kind, Model, Object};
    let data = Object {
        hash: "a".repeat(64),
        ext: "png".into(),
        animated: 0,
        width: 1,
        height: 1,
        byte_size: 1,
        added_at: 1,
    };
    let operations = (0..20)
        .map(|n| Change {
            stamp: Stamp {
                at: n,
                device: "a".into(),
            },
            action: Action::File {
                id: "id".into(),
                path: if n % 2 == 0 {
                    format!("分类/{n}.png")
                } else {
                    format!(".sticker-manager/trash/{n}.png")
                },
                data: data.clone(),
                version: Some(n.to_string()),
            },
        })
        .collect::<Vec<_>>();
    let mut baseline = Model::default();
    for op in &operations {
        baseline.apply(op, &Default::default()).unwrap();
    }
    assert_eq!(baseline.file_history["id"].iter().count(), 2);
    for shift in 0..20 {
        let mut order = operations.clone();
        order.rotate_left(shift);
        if shift % 2 == 0 {
            order.reverse();
        }
        let mut actual = Model::default();
        for op in &order {
            actual.apply(op, &Default::default()).unwrap();
        }
        assert_eq!(actual, baseline);
        let purge = Change {
            stamp: Stamp {
                at: 30,
                device: "b".into(),
            },
            action: Action::Purge {
                entity: Kind::Sticker,
                ids: vec!["id".into()],
            },
        };
        actual.apply(&purge, &Default::default()).unwrap();
        assert!(actual.file_history.is_empty());
        let mut early = Model::default();
        early.apply(&purge, &Default::default()).unwrap();
        for op in &order {
            early.apply(op, &Default::default()).unwrap();
        }
        assert_eq!(actual, early);
        assert!(
            !early
                .snapshot()
                .iter()
                .any(|op| matches!(op.action, Action::File { .. }))
        );
    }
    let mut other = operations[0].clone();
    if let Action::File { id, .. } = &mut other.action {
        *id = "other".into();
    }
    baseline.apply(&other, &Default::default()).unwrap();
    assert_eq!(baseline.file_history.len(), 2);
}

#[test]
fn overwritten_logged_path_converges_with_new_device() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let c = Site::new();
    let first = a.import(100);
    let second = a.import(101);
    a.lib.lock().unwrap().categorize(&first, "旧目录").unwrap();
    settle(&[&a, &b], &s, &m);
    let first_path = remote_key(&a, &first);
    let second_path = remote_key(&a, &second);
    let replacement = s.read(&second_path).unwrap().unwrap();
    s.write(&first_path, &replacement).unwrap();
    s.delete(&second_path).unwrap();
    settle(&[&c, &a, &b], &s, &m);
    assert_eq!(a.state(), b.state());
    assert_eq!(a.state(), c.state());
    for site in [&a, &b, &c] {
        let l = site.lib.lock().unwrap();
        assert_eq!(l.list_stickers(LibraryView::All).unwrap().len(), 1);
        assert_eq!(
            l.list_stickers(LibraryView::RecentlyDeleted).unwrap().len(),
            1
        );
        assert_eq!(l.sticker(&second).unwrap().category.unwrap().name, "旧目录");
    }
}

#[test]
fn category_rename_advances_each_stickers_dirty_revision() {
    let (s, m) = backend();
    let a = Site::new();
    let ids = [a.import(110), a.import(111)];
    for id in &ids {
        a.lib.lock().unwrap().categorize(id, "原分类").unwrap();
    }
    a.sync(&s, &m);
    {
        let mut l = a.lib.lock().unwrap();
        let before: i64 = l
            .conn
            .query_row("SELECT revision FROM sync_file_revision", [], |r| r.get(0))
            .unwrap();
        let category = l.categories().unwrap()[0].category.id.clone();
        l.rename_category(&category, "新分类").unwrap();
        let count: i64 = l
            .conn
            .query_row(
                "SELECT count(*) FROM sync_file_dirty WHERE revision>?1",
                [before],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }
    a.sync(&s, &m);
    assert_eq!(
        a.lib
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT count(*) FROM sync_file_dirty", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn surviving_alias_without_original_does_not_project_orphan_usage() {
    let a = Site::new();
    let id = a.import(120);
    a.lib.lock().unwrap().record_usage(&id).unwrap();
    let l = a.lib.lock().unwrap();
    let (mut rev, mut model) = store::load(&l.conn).unwrap();
    store::flush(&l.conn, &mut model, &mut rev).unwrap();
    for action in [
        Action::Merge {
            entity: Kind::Sticker,
            from: id.clone(),
            into: "alias".into(),
        },
        Action::Set {
            entity: Kind::Sticker,
            id: "alias".into(),
            field: "name".into(),
            value: json!("别名"),
        },
        Action::Usage {
            sticker: "alias".into(),
            owner: "device".into(),
            count: 1,
            last_used_at: 1,
        },
        Action::Purge {
            entity: Kind::Sticker,
            ids: vec![id],
        },
    ] {
        assert!(
            model
                .apply(
                    &Change {
                        stamp: Stamp {
                            at: i64::MAX - 1,
                            device: "d".into()
                        },
                        action
                    },
                    &Default::default()
                )
                .unwrap()
        );
    }
    store::project(&l.conn, &model).unwrap();
    assert!(l.list_stickers(LibraryView::All).unwrap().is_empty());
}

/// 网盘同步后各设备看到的修改时间可能不同；给每台设备的列举 version 加不同前缀来模拟。
struct Skewed<'a> {
    inner: &'a MemoryStorage,
    tag: &'static str,
}
impl Storage for Skewed<'_> {
    fn list(&self, p: &str) -> Result<Vec<storage::FileEntry>> {
        Ok(self
            .inner
            .list(p)?
            .into_iter()
            .map(|mut f| {
                f.version = f.version.map(|v| format!("{}{v}", self.tag));
                f
            })
            .collect())
    }
    fn read(&self, p: &str) -> Result<Option<Vec<u8>>> {
        self.inner.read(p)
    }
    fn write(&self, p: &str, b: &[u8]) -> Result<()> {
        self.inner.write(p, b)
    }
    fn delete(&self, p: &str) -> Result<()> {
        self.inner.delete(p)
    }
}

#[test]
fn devices_seeing_different_versions_do_not_keep_appending_file_ops() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    for n in 0..5 {
        a.import(n);
    }
    let sa = Skewed {
        inner: &s,
        tag: "a",
    };
    let sb = Skewed {
        inner: &s,
        tag: "b",
    };
    let last_seqs = || {
        let mut result = BTreeMap::<String, u64>::new();
        for f in s.list(".sticker-manager/ops/").unwrap() {
            let Some((device, range)) = f
                .key
                .strip_prefix(".sticker-manager/ops/")
                .and_then(|k| k.strip_suffix(".jsonl"))
                .and_then(|k| k.split_once('/'))
            else {
                continue;
            };
            let last = range.split_once('-').unwrap().1.parse::<u64>().unwrap();
            let entry = result.entry(device.to_owned()).or_default();
            *entry = (*entry).max(last);
        }
        result
    };
    for _ in 0..3 {
        a.sync(&sa, &m);
        b.sync(&sb, &m);
    }
    let settled = last_seqs();
    for _ in 0..3 {
        a.sync(&sa, &m);
        b.sync(&sb, &m);
    }
    assert_eq!(last_seqs(), settled);
    assert_eq!(a.state(), b.state());
    assert_eq!(
        b.lib
            .lock()
            .unwrap()
            .list_stickers(LibraryView::All)
            .unwrap()
            .len(),
        5
    );
}

#[test]
fn manual_jpeg_extension_is_imported_and_keeps_its_remote_name() {
    let (s, m) = backend();
    let a = Site::new();
    let jpeg = |color: u8| {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            8,
            6,
            image::Rgb([color, 9, 9]),
        ))
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .unwrap();
        bytes.into_inner()
    };
    s.write("看戏/截图.jpeg", &jpeg(10)).unwrap();
    s.write("照片.JPEG", &jpeg(200)).unwrap();
    settle(&[&a], &s, &m);
    let l = a.lib.lock().unwrap();
    let all = l.list_stickers(LibraryView::All).unwrap();
    assert_eq!(all.len(), 2);
    let shot = all.iter().find(|x| x.name == "截图").unwrap();
    assert_eq!(shot.category.as_ref().unwrap().name, "看戏");
    let photo = all.iter().find(|x| x.name == "照片").unwrap();
    assert!(photo.category.is_none());
    let mut keys = remote_images(&s)
        .into_iter()
        .map(|f| f.key)
        .collect::<Vec<_>>();
    keys.sort();
    assert_eq!(keys, ["照片.JPEG", "看戏/截图.jpeg"]);
}
