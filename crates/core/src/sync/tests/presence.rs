use super::*;
use crate::sync::presence::{StickerSyncInfo, received};

fn info(site: &Site, id: &str, m: &Manifest, s: &dyn Storage) -> StickerSyncInfo {
    site.lib
        .lock()
        .unwrap()
        .sticker_sync_info(id, &m.id, &read_devices(s).unwrap())
        .unwrap()
}
#[test]
fn device_format_roundtrips_and_old_and_new_readers_are_compatible() {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Old {
        id: String,
        name: String,
        last_sync_at: i64,
    }
    let id = Uuid::new_v4().to_string();
    let old = json!({"id":id,"name":"Mac","lastSyncAt":123});
    let device: RemoteDevice = serde_json::from_value(old).unwrap();
    assert!(device.platform.is_empty() && device.received.is_empty());
    let new = RemoteDevice {
        platform: "macos".into(),
        received: BTreeMap::from([("other".into(), 42)]),
        ..device
    };
    let encoded = serde_json::to_vec(&new).unwrap();
    let round: RemoteDevice = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(round.received["other"], 42);
    assert_eq!(round.platform, "macos");
    let legacy: Old = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        (legacy.id, legacy.name, legacy.last_sync_at),
        (id, "Mac".into(), 123)
    );
}
#[test]
fn three_devices_increase_presence_and_cache_survives_restart() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let c = Site::new();
    a.sync(&s, &m);
    let id = a.import(1);
    assert!(info(&a, &id, &m, &s).waiting_upload);
    a.sync(&s, &m);
    assert_eq!(info(&a, &id, &m, &s).device_count, Some(1));
    assert_eq!(b.sync(&s, &m).new_stickers, 0);
    assert_eq!(info(&a, &id, &m, &s).device_count, Some(2));
    assert_eq!(info(&b, &id, &m, &s).device_count, Some(2));
    c.sync(&s, &m);
    assert_eq!(info(&a, &id, &m, &s).device_count, Some(3));
    let report = a.sync(&s, &m);
    let root = a.lib.lock().unwrap().root.clone();
    let reopened = Library::open(&root).unwrap();
    let (cached, time) = reopened.cached_sync_devices(&m.id).unwrap();
    assert_eq!((cached.len(), time), (3, report.last_sync_at));
    assert_eq!(
        reopened
            .sticker_sync_info(&id, &m.id, &cached)
            .unwrap()
            .device_count,
        Some(3)
    );
}
#[test]
fn snapshot_origins_and_merged_same_hash_retain_each_publishers_source() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let c = Site::new();
    let aid = a.import(7);
    let bid = b.import(7);
    a.sync(&s, &m);
    b.sync(&s, &m);
    c.sync(&s, &m);
    a.sync(&s, &m);
    let lib = a.lib.lock().unwrap();
    let representative = lib.list_stickers(LibraryView::All).unwrap().remove(0).id;
    let origins:i64=lib.conn.query_row("SELECT COUNT(DISTINCT device) FROM sync_origins WHERE library_id=?1 AND sticker_id IN (?2,?3)",params![m.id,aid,bid],|r|r.get(0)).unwrap();
    assert_eq!(origins, 2);
    assert_eq!(
        lib.sticker_sync_info(&representative, &m.id, &read_devices(&s).unwrap())
            .unwrap()
            .device_count,
        Some(3)
    );
    lib.conn.execute("DELETE FROM sync_origins", []).unwrap();
    let legacy = lib
        .sticker_sync_info(&representative, &m.id, &read_devices(&s).unwrap())
        .unwrap();
    assert_eq!(
        legacy,
        StickerSyncInfo {
            waiting_upload: false,
            device_count: None
        }
    );
}
#[test]
fn received_stops_before_earliest_pending_and_delayed_first_download_is_not_new() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    a.sync(&s, &m);
    s.hold(true);
    let id = a.import(8);
    a.import(9);
    a.sync(&s, &m);
    for key in s
        .pending_keys()
        .into_iter()
        .filter(|k| paths::image(k).is_none())
    {
        s.deliver(&key);
    }
    let report = b.sync(&s, &m);
    assert!(report.pending > 0);
    let a_id = a.lib.lock().unwrap().device.id.clone();
    let applied = received(&b.lib.lock().unwrap().conn, &m.id).unwrap();
    assert_eq!(applied[&a_id], 1); // 空快照已应用，seq 2 的原图未到。
    assert_eq!(info(&a, &id, &m, &s).device_count, Some(1));
    s.hold(false);
    s.deliver_all();
    let report = b.sync(&s, &m);
    assert_eq!((report.pending, report.new_stickers), (0, 0));
    assert_eq!(
        received(&b.lib.lock().unwrap().conn, &m.id).unwrap()[&a_id],
        5
    );
    assert_eq!(info(&a, &id, &m, &s).device_count, Some(2));
}
#[test]
fn new_count_excludes_join_restore_merge_local_import_and_metadata_edits() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let old = a.import(4);
    a.sync(&s, &m);
    assert_eq!(b.sync(&s, &m).new_stickers, 0);
    let fresh = a.import(5);
    a.sync(&s, &m);
    let arrivals = std::sync::atomic::AtomicUsize::new(0);
    let report = synchronize_with_events(&b.lib, &s, &m.id, &|_| {}, &|count| {
        assert!(b.lib.try_lock().is_ok(), "通知必须在提交后的仓库锁外执行");
        arrivals.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
    })
    .unwrap();
    assert_eq!(report.new_stickers, 1);
    assert_eq!(arrivals.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(b.sync(&s, &m).new_stickers, 0);
    a.lib.lock().unwrap().delete_sticker(&old).unwrap();
    a.sync(&s, &m);
    b.sync(&s, &m);
    a.lib.lock().unwrap().restore_sticker(&old).unwrap();
    a.lib
        .lock()
        .unwrap()
        .set_sticker_caption(&fresh, "更新配文")
        .unwrap();
    a.sync(&s, &m);
    assert_eq!(b.sync(&s, &m).new_stickers, 0);
    b.import(6);
    a.import(6);
    a.sync(&s, &m);
    assert_eq!(b.sync(&s, &m).new_stickers, 0);
}
#[test]
fn reimport_and_purged_file_cleanup_never_remove_new_original_or_deadlock() {
    let (s, m) = backend();
    let site = Site::new();
    let id = site.import(10);
    site.sync(&s, &m);
    {
        let mut lib = site.lib.lock().unwrap();
        lib.delete_sticker(&id).unwrap();
        lib.purge_sticker(&id).unwrap();
    }
    site.sync(&s, &m); // 模型保留旧身份的原图清理候选。
    let (root, files) = {
        let lib = site.lib.lock().unwrap();
        (lib.root.clone(), lib.files.clone())
    };
    let path = site.dir.path().join("10.png");
    // 清理线程与锁外准备/锁内提交真实交错；两种执行次序都必须留下有效原图。
    std::thread::scope(|scope| {
        let cleanup = scope.spawn(|| {
            for _ in 0..100 {
                cleanup_purged_files(&site.lib, &root, &files).unwrap();
                std::thread::yield_now();
            }
        });
        let report =
            crate::import_paths_with_progress(&site.lib, &[path], &|| false, &mut |_| {}).unwrap();
        assert_eq!(report.added, 1);
        cleanup.join().unwrap();
    });
    cleanup_purged_files(&site.lib, &root, &files).unwrap();
    let sticker = site
        .lib
        .lock()
        .unwrap()
        .list_stickers(LibraryView::All)
        .unwrap()
        .remove(0);
    assert!(sticker.object_path.is_file() && sticker.thumbnail_path.is_file());
    assert!(sticker.read_image().is_ok());
}
