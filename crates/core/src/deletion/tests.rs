use std::path::Path;

use super::*;
use crate::{LibraryOverview, LibraryView};

fn library(root: &Path) -> Library {
    let library = Library::open(root).unwrap();
    library
        .conn
        .execute_batch(
            "INSERT INTO stickers (id, hash, ext, name, width, height, byte_size, added_at) VALUES
            ('one', 'hash1', 'png', '表情一', 1, 1, 1, 1),
            ('two', 'hash2', 'png', '表情二', 1, 1, 1, 2),
            ('three', 'hash3', 'png', '表情三', 1, 1, 1, 3);",
        )
        .unwrap();
    for sticker in library.list_stickers(LibraryView::All).unwrap() {
        fs::write(sticker.object_path, b"original").unwrap();
        fs::write(sticker.thumbnail_path, b"thumbnail").unwrap();
    }
    library
}

fn add_metadata(library: &mut Library, id: &str) {
    library
        .create_category_and_set_stickers(&[id.to_owned()], "开心")
        .unwrap();
    library.set_sticker_caption(id, "收到").unwrap();
    library.toggle_favorite(id).unwrap();
    library.record_usage(id).unwrap();
    library.record_usage(id).unwrap();
    library
        .conn
        .execute(
            "INSERT INTO usages VALUES (?1, 'other-device', 7, 100)",
            [id],
        )
        .unwrap();
}

fn ids(library: &Library, view: LibraryView) -> Vec<String> {
    library
        .list_stickers(view)
        .unwrap()
        .into_iter()
        .map(|sticker| sticker.id)
        .collect()
}

#[test]
fn deletion_updates_views_search_and_counts_without_removing_the_category() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    add_metadata(&mut library, "one");
    let tag = library.sticker("one").unwrap().category.unwrap();
    assert_eq!(ids(&library, LibraryView::Category(tag.id.clone())), ["one"]);
    assert_eq!(library.search_stickers("收到").unwrap().len(), 1);
    assert_eq!(
        library.overview().unwrap(),
        LibraryOverview {
            all: 3,
            recent: 1,
            favorites: 1,
            uncategorized: 2,
            recently_deleted: 0,
        }
    );

    let before = unix_millis().unwrap();
    library.delete_sticker("one").unwrap();
    library.delete_sticker("two").unwrap();
    assert!(
        (before..=unix_millis().unwrap())
            .contains(&library.sticker("one").unwrap().deleted_at.unwrap())
    );
    assert_eq!(ids(&library, LibraryView::All), ["three"]);
    assert_eq!(ids(&library, LibraryView::Uncategorized), ["three"]);
    for view in [
        LibraryView::Favorites,
        LibraryView::Recent,
        LibraryView::Category(tag.id),
    ] {
        assert!(ids(&library, view).is_empty());
    }
    for query in ["表情一", "收到", "开心"] {
        assert!(library.search_stickers(query).unwrap().is_empty());
    }
    assert_eq!(library.search_stickers("表情").unwrap().len(), 1);
    assert_eq!(
        library.overview().unwrap(),
        LibraryOverview {
            all: 1,
            recent: 0,
            favorites: 0,
            uncategorized: 1,
            recently_deleted: 2,
        }
    );
    let deleted = ids(&library, LibraryView::RecentlyDeleted);
    assert!(deleted.contains(&"one".into()) && deleted.contains(&"two".into()));
    for tag in library.categories().unwrap() {
        assert_eq!((tag.sticker_count, tag.total_sticker_count), (0, 1));
    }
}

#[test]
fn batch_delete_and_restore_skip_stickers_in_the_wrong_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let ask = |names: &[&str]| names.iter().map(|id| id.to_string()).collect::<Vec<_>>();

    library.delete_sticker("three").unwrap();
    let mut deleted = library
        .delete_stickers(&ask(&["one", "two", "three", "missing"]))
        .unwrap();
    deleted.sort();
    assert_eq!(deleted, ["one", "two"]);
    assert!(ids(&library, LibraryView::All).is_empty());

    let mut restored = library
        .restore_stickers(&ask(&["one", "two", "missing"]))
        .unwrap();
    restored.sort();
    assert_eq!(restored, ["one", "two"]);
    assert_eq!(ids(&library, LibraryView::RecentlyDeleted), ["three"]);
    assert!(library.restore_stickers(&ask(&["one"])).unwrap().is_empty());
    assert!(library.delete_stickers(&[]).unwrap().is_empty());
}

#[test]
fn invalid_transitions_return_the_corresponding_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    for id in ["one", "missing"] {
        assert!(matches!(
            library.restore_sticker(id),
            Err(Error::StickerNotDeleted)
        ));
        assert!(matches!(
            library.purge_sticker(id),
            Err(Error::StickerNotDeleted)
        ));
    }
    library.delete_sticker("one").unwrap();
    let deleted_at = library.sticker("one").unwrap().deleted_at;
    for id in ["one", "missing"] {
        assert!(matches!(
            library.delete_sticker(id),
            Err(Error::StickerUnavailable)
        ));
    }
    assert_eq!(library.sticker("one").unwrap().deleted_at, deleted_at);
    library.purge_sticker("one").unwrap();
    assert!(matches!(
        library.restore_sticker("one"),
        Err(Error::StickerNotDeleted)
    ));
    assert!(matches!(
        library.purge_sticker("one"),
        Err(Error::StickerNotDeleted)
    ));
}

#[test]
fn restoring_preserves_metadata_and_each_devices_usage() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    add_metadata(&mut library, "one");
    let before = library.sticker("one").unwrap();
    let usages = |library: &Library| {
        library.conn.prepare("SELECT device_id, count, last_used_at FROM usages WHERE sticker_id = 'one' ORDER BY device_id").unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    let before_usages = usages(&library);
    library.delete_sticker("one").unwrap();
    library.restore_sticker("one").unwrap();
    let after = library.sticker("one").unwrap();
    assert_eq!(after.deleted_at, None);
    assert_eq!(after.purge_at, None);
    assert_eq!(after.category, before.category);
    assert_eq!(after.caption, before.caption);
    assert_eq!(after.favorite, before.favorite);
    assert_eq!(
        (after.usage_count, after.last_used_at),
        (9, before.last_used_at)
    );
    assert_eq!(usages(&library), before_usages);
    assert_eq!((after.id, after.added_at), (before.id, before.added_at));
    assert_eq!(ids(&library, LibraryView::Favorites), ["one"]);
    assert_eq!(ids(&library, LibraryView::Recent), ["one"]);
    for tag in library.categories().unwrap() {
        assert_eq!((tag.sticker_count, tag.total_sticker_count), (1, 1));
    }
}

#[test]
fn purging_cascades_metadata_and_removes_both_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    add_metadata(&mut library, "one");
    let sticker = library.sticker("one").unwrap();
    library.delete_sticker("one").unwrap();
    library.purge_sticker("one").unwrap();
    assert!(matches!(
        library.sticker("one"),
        Err(Error::StickerUnavailable)
    ));
    for table in ["usages"] {
        let count: i64 = library
            .conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE sticker_id = 'one'"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
    assert!(!sticker.object_path.exists());
    assert!(!sticker.thumbnail_path.exists());
    assert_eq!(library.categories().unwrap().len(), 1);
    assert!(library.sticker("two").unwrap().object_path.is_file());
}

#[test]
fn bulk_purge_uses_confirmed_ids_skips_restored_and_cascades_metadata_and_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    add_metadata(&mut library, "one");
    let purged = library.sticker("one").unwrap();
    library.delete_sticker("one").unwrap();
    library.delete_sticker("two").unwrap();
    let confirmed = vec!["one".into(), "two".into(), "one".into(), "missing".into()];
    library.restore_sticker("two").unwrap();
    library.delete_sticker("three").unwrap();
    assert_eq!(library.purge_deleted_stickers(&confirmed).unwrap(), 1);
    assert!(library.sticker("one").is_err());
    assert!(library.sticker("two").unwrap().deleted_at.is_none());
    assert!(library.sticker("three").unwrap().deleted_at.is_some());
    for table in ["usages", "sync_usage_offsets"] {
        let count: i64 = library
            .conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE sticker_id='one'"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
    assert!(!purged.object_path.exists() && !purged.thumbnail_path.exists());
    assert!(library.sticker("two").unwrap().object_path.exists());
    assert!(library.sticker("three").unwrap().thumbnail_path.exists());
    assert_eq!(library.purge_deleted_stickers(&confirmed).unwrap(), 0);
    assert_eq!(library.purge_deleted_stickers(&[]).unwrap(), 0);
    let purges: i64 = library
        .conn
        .query_row(
            "SELECT COUNT(*) FROM sync_outbox WHERE json_extract(action,'$.type')='purge'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(purges, 1);
}

#[test]
fn bulk_purge_commit_failure_keeps_every_sticker_and_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    for id in ["one", "two"] {
        library.delete_sticker(id).unwrap();
    }
    library.conn.execute_batch(
        "CREATE TABLE retained (sticker_id TEXT REFERENCES stickers(id) DEFERRABLE INITIALLY DEFERRED);
         CREATE TRIGGER fail_bulk AFTER DELETE ON stickers WHEN OLD.id='two'
         BEGIN INSERT INTO retained VALUES (OLD.id); END;"
    ).unwrap();
    assert!(
        library
            .purge_deleted_stickers(&["one".into(), "two".into()])
            .is_err()
    );
    for id in ["one", "two"] {
        let s = library.sticker(id).unwrap();
        assert!(s.deleted_at.is_some());
        assert!(s.object_path.is_file() && s.thumbnail_path.is_file());
    }
}

#[test]
fn missing_original_and_file_cleanup_failure_do_not_fail_purging() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let sticker = library.sticker("one").unwrap();
    fs::remove_file(&sticker.object_path).unwrap();
    library.delete_sticker("one").unwrap();
    library.purge_sticker("one").unwrap();
    assert!(!sticker.thumbnail_path.exists());

    let sticker = library.sticker("two").unwrap();
    fs::remove_file(&sticker.object_path).unwrap();
    fs::create_dir(&sticker.object_path).unwrap();
    library.delete_sticker("two").unwrap();
    library.purge_sticker("two").unwrap();
    assert!(matches!(
        library.sticker("two"),
        Err(Error::StickerUnavailable)
    ));
    assert!(sticker.object_path.is_dir());
    assert!(!sticker.thumbnail_path.exists());
}

#[test]
fn failed_commit_preserves_metadata_and_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    add_metadata(&mut library, "one");
    let sticker = library.sticker("one").unwrap();
    library.delete_sticker("one").unwrap();
    // 延迟外键检查，让 DELETE 成功但 COMMIT 失败。
    library.conn.execute_batch(
        "CREATE TABLE retained (sticker_id TEXT REFERENCES stickers(id) DEFERRABLE INITIALLY DEFERRED);
         CREATE TRIGGER fail_commit AFTER DELETE ON stickers BEGIN INSERT INTO retained VALUES (OLD.id); END;"
    ).unwrap();
    assert!(library.purge_sticker("one").is_err());
    let after = library.sticker("one").unwrap();
    assert!(after.deleted_at.is_some());
    assert_eq!(after.category, sticker.category);
    assert_eq!(after.usage_count, sticker.usage_count);
    assert!(sticker.object_path.is_file());
    assert!(sticker.thumbnail_path.is_file());
}

#[test]
fn importing_after_purge_creates_a_new_sticker_without_old_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let source = dir.path().join("收到.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
        .save(&source)
        .unwrap();
    assert_eq!(library.import_paths(std::slice::from_ref(&source)).added, 1);
    let before = library.list_stickers(LibraryView::All).unwrap().remove(0);
    add_metadata(&mut library, &before.id);
    library.delete_sticker(&before.id).unwrap();
    library.purge_sticker(&before.id).unwrap();
    let report = library.import_paths(&[source]);
    assert!(report.failures.is_empty());
    assert_eq!((report.added, report.existing, report.restored), (1, 0, 0));
    let after = library.list_stickers(LibraryView::All).unwrap().remove(0);
    assert_ne!(after.id, before.id);
    assert!(after.category.is_none());
    assert!(!after.favorite);
    assert_eq!(after.caption, None);
    assert_eq!((after.usage_count, after.last_used_at), (0, None));
    assert!(after.object_path.is_file() && after.thumbnail_path.is_file());
}

#[test]
fn expiry_includes_exact_boundary_and_preserves_active_stickers() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let now = RETENTION_MS + 1000;
    library
        .conn
        .execute_batch(
            "UPDATE stickers SET deleted_at = 1000 WHERE id = 'one';
         UPDATE stickers SET deleted_at = 1001 WHERE id = 'two';",
        )
        .unwrap();
    let expired = library.sticker("one").unwrap();
    assert_eq!(library.purge_expired_at(now).unwrap(), 1);
    assert!(!expired.object_path.exists() && !expired.thumbnail_path.exists());
    assert_eq!(ids(&library, LibraryView::RecentlyDeleted), ["two"]);
    assert_eq!(ids(&library, LibraryView::All), ["three"]);
    assert_eq!(library.purge_expired_at(now).unwrap(), 0);
    assert_eq!(library.purge_expired_at(now + 1).unwrap(), 1);
    assert_eq!(library.purge_expired().unwrap(), 0);
    assert!(library.sticker("three").unwrap().object_path.is_file());
}

#[test]
fn recently_deleted_orders_by_deletion_time_and_exposes_purge_time() {
    let dir = tempfile::tempdir().unwrap();
    let library = library(dir.path());
    assert_eq!(library.sticker("three").unwrap().purge_at, None);
    library
        .conn
        .execute_batch(
            "UPDATE stickers SET deleted_at = 300 WHERE id = 'one';
         UPDATE stickers SET deleted_at = 100 WHERE id = 'two';
         UPDATE stickers SET deleted_at = 200 WHERE id = 'three';",
        )
        .unwrap();
    assert_eq!(
        ids(&library, LibraryView::RecentlyDeleted),
        ["one", "three", "two"]
    );
    for sticker in library.list_stickers(LibraryView::RecentlyDeleted).unwrap() {
        assert_eq!(
            sticker.purge_at,
            Some(sticker.deleted_at.unwrap() + RETENTION_MS)
        );
        assert_eq!(
            serde_json::to_value(&sticker).unwrap()["purgeAt"],
            sticker.purge_at.unwrap()
        );
    }
}

#[test]
fn sweeping_removes_only_old_unreferenced_hash_files_and_temporary_files() {
    let dir = tempfile::tempdir().unwrap();
    let library = library(dir.path());
    let hash = |c: char| c.to_string().repeat(64);
    let live = hash('a');
    let deleted = hash('b');
    let orphan = hash('c');
    library
        .conn
        .execute_batch(&format!(
            "INSERT INTO stickers (id, hash, ext, name, width, height, byte_size, added_at, deleted_at) VALUES
            ('live', '{live}', 'gif', '在用', 1, 1, 1, 4, NULL),
            ('deleted', '{deleted}', 'png', '已删除', 1, 1, 1, 5, 6);"
        ))
        .unwrap();
    let objects = dir.path().join("objects");
    let thumbs = dir.path().join("thumbs");
    let kept = [
        objects.join(format!("{live}.gif")),
        thumbs.join(format!("{live}.png")),
        objects.join(format!("{deleted}.png")),
        thumbs.join(format!("{deleted}.png")),
        objects.join(".DS_Store"),
        objects.join("说明.txt"),
        objects.join(format!("{}.png", "C".repeat(64))),
    ];
    let removed = [
        objects.join(format!("{orphan}.webp")),
        thumbs.join(format!("{orphan}.png")),
        objects.join(".tmpAbC123"),
        thumbs.join(".tmpXyZ789"),
    ];
    for path in kept.iter().chain(&removed) {
        fs::write(path, b"file").unwrap();
    }

    // 刚写下的孤立文件可能是同步刚下载、还没合并日志的原图，不能立刻清理。
    assert_eq!(library.sweep_orphan_files().unwrap(), 0);
    assert!(removed.iter().all(|path| path.is_file()));

    let later = SystemTime::now() + ORPHAN_GRACE + Duration::from_secs(60);
    assert_eq!(library.sweep_orphan_files_at(later).unwrap(), removed.len());
    assert!(removed.iter().all(|path| !path.exists()));
    assert!(kept.iter().all(|path| path.is_file()));
    for sticker in library.list_stickers(LibraryView::All).unwrap() {
        assert!(sticker.object_path.is_file() && sticker.thumbnail_path.is_file());
    }
}

#[test]
fn sweeping_cleans_up_files_a_failed_purge_left_behind() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(&dir.path().join("library")).unwrap();
    let source = dir.path().join("收到.png");
    image::RgbaImage::new(2, 2).save(&source).unwrap();
    assert_eq!(library.import_paths(&[source]).added, 1);
    let sticker = library.list_stickers(LibraryView::All).unwrap().remove(0);
    library.delete_sticker(&sticker.id).unwrap();
    // 模拟彻底删除时删文件失败：元数据已删，文件还在。
    library
        .conn
        .execute("DELETE FROM stickers WHERE id = ?1", [&sticker.id])
        .unwrap();
    assert!(sticker.object_path.is_file() && sticker.thumbnail_path.is_file());

    let later = SystemTime::now() + ORPHAN_GRACE + Duration::from_secs(60);
    assert_eq!(library.sweep_orphan_files_at(later).unwrap(), 2);
    assert!(!sticker.object_path.exists() && !sticker.thumbnail_path.exists());
}
