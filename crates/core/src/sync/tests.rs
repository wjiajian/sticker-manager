use super::model::{Change, Stamp};
use super::storage::MemoryStorage;
use super::*;
use crate::LibraryView;
use image::{Rgba, RgbaImage};
use serde_json::{Value, json};

struct Site {
    dir: tempfile::TempDir,
    lib: Mutex<Library>,
}
impl Site {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let lib = Mutex::new(Library::open(&dir.path().join("library")).unwrap());
        Self { dir, lib }
    }
    fn import(&self, color: u8) -> String {
        let path = self.dir.path().join(format!("{color}.png"));
        RgbaImage::from_pixel(8, 6, Rgba([color, 0, 0, 255]))
            .save(&path)
            .unwrap();
        let mut l = self.lib.lock().unwrap();
        let result = l.import_paths(&[path]);
        assert!(result.failures.is_empty(), "{:?}", result.failures);
        l.conn
            .query_row(
                "SELECT id FROM stickers WHERE name=?1",
                [color.to_string()],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn sync(&self, s: &dyn Storage, m: &Manifest) -> SyncReport {
        synchronize(&self.lib, s, &m.id).unwrap()
    }
    fn state(&self) -> Value {
        let l = self.lib.lock().unwrap();
        let mut result = serde_json::Map::new();
        for (table, cols) in [
            (
                "stickers",
                "id,hash,ext,name,caption,favorite,animated,width,height,byte_size,added_at,deleted_at",
            ),
            ("tags", "id,name,color,group_id,position"),
            ("tag_groups", "id,name,position"),
            ("sticker_tags", "sticker_id,tag_id"),
            ("usages", "sticker_id,device_id,count,last_used_at"),
        ] {
            let mut q = l
                .conn
                .prepare(&format!("SELECT {cols} FROM {table} ORDER BY 1,2"))
                .unwrap();
            let n = q.column_count();
            let rows = q
                .query_map([], |r| {
                    Ok((0..n)
                        .map(|i| match r.get_ref(i).unwrap() {
                            rusqlite::types::ValueRef::Null => Value::Null,
                            rusqlite::types::ValueRef::Integer(n) => json!(n),
                            rusqlite::types::ValueRef::Text(s) => {
                                json!(std::str::from_utf8(s).unwrap())
                            }
                            _ => panic!("unexpected SQL type"),
                        })
                        .collect::<Vec<_>>())
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            result.insert(table.into(), json!(rows));
        }
        Value::Object(result)
    }
}
fn backend() -> (MemoryStorage, Manifest) {
    let s = MemoryStorage::default();
    let m = Manifest::create(&s).unwrap();
    (s, m)
}
fn settle(sites: &[&Site], s: &MemoryStorage, m: &Manifest) {
    s.hold(false);
    s.deliver_all();
    for _ in 0..4 {
        for site in sites {
            site.sync(s, m);
        }
        s.deliver_all();
    }
}
fn change(at: i64, device: &str, action: Action) -> Change {
    Change {
        stamp: Stamp {
            at,
            device: device.into(),
        },
        action,
    }
}
fn write_ops(s: &dyn Storage, device: &str, first: i64, changes: &[Change]) {
    let ops = changes
        .iter()
        .enumerate()
        .map(|(i, c)| Operation {
            seq: first + i as i64,
            change: c.clone(),
        })
        .collect::<Vec<_>>();
    write_segment(s, device, &ops);
}
fn write_segment(s: &dyn Storage, device: &str, ops: &[Operation]) {
    let mut b = Vec::new();
    for op in ops {
        serde_json::to_writer(&mut b, op).unwrap();
        b.push(b'\n');
    }
    s.write(
        &format!(
            "ops/{device}/{:012}-{:012}.jsonl",
            ops.first().unwrap().seq,
            ops.last().unwrap().seq
        ),
        &b,
    )
    .unwrap();
}
fn remote_tag(device: &str, id: &str, at: i64, name: &str) -> Change {
    change(
        at,
        device,
        Action::Create {
            entity: Kind::Tag,
            id: id.into(),
            data: None,
            fields: serde_json::from_value(
                json!({"name":name,"color":"#F2B705","group_id":null,"position":0}),
            )
            .unwrap(),
        },
    )
}

#[test]
fn actual_folder_two_library_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let s = FolderStorage::new(dir.path()).unwrap();
    two_library_end_to_end(&s);
}
fn two_library_end_to_end(s: &dyn Storage) {
    let m = Manifest::create(s).unwrap();
    let a = Site::new();
    let b = Site::new();
    let x = a.import(10);
    let y = a.import(20);
    let tag = {
        let mut l = a.lib.lock().unwrap();
        let t = l.create_tag_and_add(&x, "开心").unwrap();
        l.set_sticker_caption(&x, "收到").unwrap();
        t.id
    };
    a.sync(s, &m);
    let info = inspect(s).unwrap();
    assert_eq!((info.devices, info.stickers), (1, 2));
    b.sync(s, &m);
    assert_eq!(a.state(), b.state());
    {
        let l = b.lib.lock().unwrap();
        l.toggle_favorite(&x).unwrap();
        l.record_usage(&x).unwrap();
        l.delete_sticker(&y).unwrap();
        assert!(l.sticker(&x).unwrap().thumbnail_path.is_file());
    }
    b.sync(s, &m);
    a.sync(s, &m);
    assert_eq!(a.state(), b.state());
    a.lib.lock().unwrap().restore_sticker(&y).unwrap();
    a.sync(s, &m);
    b.sync(s, &m);
    let t1 = a
        .lib
        .lock()
        .unwrap()
        .create_tag_and_add(&x, "Shared")
        .unwrap()
        .id;
    let t2 = b
        .lib
        .lock()
        .unwrap()
        .create_tag_and_add(&y, "shared")
        .unwrap()
        .id;
    for _ in 0..3 {
        a.sync(s, &m);
        b.sync(s, &m);
    }
    assert_eq!(a.state(), b.state());
    let l = a.lib.lock().unwrap();
    let sidebar = l.tag_sidebar().unwrap();
    assert_eq!(sidebar.ungrouped.len(), 2);
    assert_eq!(l.sticker(&y).unwrap().tags[0].id, t1.min(t2));
    assert!(l.sticker(&x).unwrap().tags.iter().any(|t| t.id == tag));
    let key = format!(
        "objects/{}",
        l.sticker(&y)
            .unwrap()
            .object_path
            .file_name()
            .unwrap()
            .to_string_lossy()
    );
    drop(l);
    {
        let mut l = a.lib.lock().unwrap();
        l.delete_sticker(&y).unwrap();
        l.purge_sticker(&y).unwrap();
    }
    a.sync(s, &m);
    b.sync(s, &m);
    a.sync(s, &m);
    assert_eq!(a.state(), b.state());
    assert!(s.read(&key).unwrap().is_none(), "彻底删除应移除远端原图");
}

#[test]
fn independent_fields_hlc_and_edits_during_recently_deleted() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let id = a.import(1);
    settle(&[&a, &b], &s, &m);
    {
        let l = a.lib.lock().unwrap();
        l.delete_sticker(&id).unwrap();
    }
    {
        let mut l = b.lib.lock().unwrap();
        l.set_sticker_caption(&id, "仍在").unwrap();
        l.toggle_favorite(&id).unwrap();
        l.create_tag_and_add(&id, "晚到").unwrap();
    }
    settle(&[&b, &a], &s, &m);
    a.lib.lock().unwrap().restore_sticker(&id).unwrap();
    settle(&[&a, &b], &s, &m);
    let l = a.lib.lock().unwrap();
    let sticker = l.sticker(&id).unwrap();
    assert_eq!(sticker.caption.as_deref(), Some("仍在"));
    assert!(sticker.favorite);
    assert_eq!(sticker.tags.len(), 1);
    drop(l);
    assert_eq!(a.state(), b.state());
}

#[test]
fn delayed_alias_operations_and_per_device_usage_components_survive_merge() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let ia = a.import(2);
    let ib = b.import(2);
    a.lib.lock().unwrap().record_usage(&ia).unwrap();
    {
        let l = b.lib.lock().unwrap();
        l.record_usage(&ib).unwrap();
        l.record_usage(&ib).unwrap();
        l.set_sticker_caption(&ib, "延迟配文").unwrap();
    }
    let before = b.lib.lock().unwrap().sticker(&ib).unwrap().last_used_at;
    settle(&[&a, &b], &s, &m);
    let into = ia.clone().min(ib.clone());
    let from = ia.max(ib);
    let device = Uuid::new_v4().to_string();
    write_ops(
        &s,
        &device,
        1,
        &[change(
            unix_millis().unwrap() + 100_000,
            &device,
            Action::Set {
                entity: Kind::Sticker,
                id: from,
                field: "caption".into(),
                value: json!("合并后迟到"),
            },
        )],
    );
    settle(&[&a, &b], &s, &m);
    {
        let l = a.lib.lock().unwrap();
        let v = l.sticker(&into).unwrap();
        assert_eq!(v.usage_count, 3);
        assert!(v.last_used_at >= before);
        assert_eq!(v.caption.as_deref(), Some("合并后迟到"));
        l.record_usage(&into).unwrap();
    }
    settle(&[&a, &b], &s, &m);
    assert_eq!(a.lib.lock().unwrap().sticker(&into).unwrap().usage_count, 4);
    assert_eq!(a.state(), b.state());
    let max_at: i64 = a
        .lib
        .lock()
        .unwrap()
        .conn
        .query_row("SELECT max_at FROM sync_meta", [], |r| r.get(0))
        .unwrap();
    assert!(max_at > unix_millis().unwrap() + 90_000);
}

#[test]
fn purge_is_permanent_and_reimport_has_no_old_links_or_usage() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let old = a.import(3);
    {
        let mut l = a.lib.lock().unwrap();
        l.create_tag_and_add(&old, "旧标签").unwrap();
        l.record_usage(&old).unwrap();
        l.toggle_favorite(&old).unwrap();
    }
    settle(&[&a, &b], &s, &m);
    {
        let mut l = a.lib.lock().unwrap();
        l.delete_sticker(&old).unwrap();
        l.purge_sticker(&old).unwrap();
    }
    let new = a.import(3);
    assert_ne!(new, old);
    b.lib
        .lock()
        .unwrap()
        .set_sticker_caption(&old, "迟到")
        .unwrap();
    settle(&[&b, &a], &s, &m);
    let l = b.lib.lock().unwrap();
    assert!(l.sticker(&old).is_err());
    let v = l.sticker(&new).unwrap();
    assert_eq!(v.usage_count, 0);
    assert!(v.tags.is_empty());
    assert!(!v.favorite);
    assert_eq!(v.caption, None);
    drop(l);
    assert_eq!(a.state(), b.state());
}

#[test]
fn missing_objects_and_cross_device_dependencies_wait_and_retry_without_new_segments() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let c = Site::new();
    let id = a.import(4);
    a.sync(&s, &m);
    b.sync(&s, &m);
    let key = a
        .lib
        .lock()
        .unwrap()
        .sticker(&id)
        .unwrap()
        .object_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let object = s.read(&format!("objects/{key}")).unwrap().unwrap();
    s.delete(&format!("objects/{key}")).unwrap();
    b.lib
        .lock()
        .unwrap()
        .create_tag_and_add(&id, "依赖")
        .unwrap();
    b.sync(&s, &m); // 补传原图
    s.delete(&format!("objects/{key}")).unwrap();
    let report = c.sync(&s, &m);
    assert!(report.pending > 0);
    assert_eq!(c.lib.lock().unwrap().overview().unwrap().all, 0);
    s.write(&format!("objects/{key}"), b"truncated image")
        .unwrap();
    assert!(c.sync(&s, &m).pending > 0);
    s.write(&format!("objects/{key}"), &object).unwrap();
    assert_eq!(c.sync(&s, &m).pending, 0);
    assert_eq!(c.lib.lock().unwrap().sticker(&id).unwrap().tags.len(), 1);
}

#[test]
fn gap_overlap_parse_failure_unknown_operation_and_conflict_names() {
    let (s, m) = backend();
    let a = Site::new();
    let d = Uuid::new_v4().to_string();
    let first = remote_tag(&d, "tag", 1, "first");
    let second = change(
        2,
        &d,
        Action::Set {
            entity: Kind::Tag,
            id: "tag".into(),
            field: "name".into(),
            value: json!("second"),
        },
    );
    write_ops(&s, &d, 2, std::slice::from_ref(&second));
    a.sync(&s, &m);
    assert!(
        a.lib
            .lock()
            .unwrap()
            .tag_sidebar()
            .unwrap()
            .ungrouped
            .is_empty()
    );
    let key = format!("ops/{d}/000000000001-000000000001.jsonl");
    s.write(&key, b"{bad\n").unwrap();
    a.sync(&s, &m);
    assert_eq!(
        store::cursor(&a.lib.lock().unwrap().conn, &m.id, &d).unwrap(),
        0
    );
    s.write(
        &format!("ops/{d}/000000000001-000000000001 (conflict).jsonl"),
        b"bad",
    )
    .unwrap();
    s.write(
        &format!("ops/{d}/.000000000001-000000000001.jsonl.icloud"),
        b"bad",
    )
    .unwrap();
    write_ops(&s, &d, 1, &[first, second.clone()]);
    a.sync(&s, &m);
    assert_eq!(
        store::cursor(&a.lib.lock().unwrap().conn, &m.id, &d).unwrap(),
        2
    );
    let before = a.state();
    a.sync(&s, &m);
    assert_eq!(before, a.state());
    write_ops(
        &s,
        &d,
        3,
        &[
            change(3, &d, Action::Unknown),
            change(
                4,
                &d,
                Action::Set {
                    entity: Kind::Tag,
                    id: "tag".into(),
                    field: "name".into(),
                    value: json!("must not apply"),
                },
            ),
        ],
    );
    assert!(matches!(
        synchronize(&a.lib, &s, &m.id),
        Err(Error::SyncUpgrade(_))
    ));
    assert_eq!(
        store::cursor(&a.lib.lock().unwrap().conn, &m.id, &d).unwrap(),
        2
    );
    assert_eq!(before, a.state());
}

#[test]
fn newer_format_and_cloned_device_identity_stop_before_writes() {
    let (s, m) = backend();
    let a = Site::new();
    a.sync(&s, &m);
    let device = a.lib.lock().unwrap().device.id.clone();
    let next: i64 = a
        .lib
        .lock()
        .unwrap()
        .conn
        .query_row("SELECT next_seq FROM sync_targets", [], |r| r.get(0))
        .unwrap();
    write_ops(
        &s,
        &device,
        next + 1,
        &[remote_tag(&device, "other", 7, "clone")],
    );
    assert!(matches!(
        synchronize(&a.lib, &s, &m.id),
        Err(Error::DeviceConflict)
    ));
    let mut newer = m.clone();
    newer.format = 2;
    s.write("library.json", &serde_json::to_vec(&newer).unwrap())
        .unwrap();
    assert!(matches!(
        synchronize(&a.lib, &s, &m.id),
        Err(Error::SyncUpgrade(_))
    ));
}

#[test]
fn tags_groups_fields_delete_and_group_dependencies() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let id = a.import(5);
    let (group, tag) = {
        let mut l = a.lib.lock().unwrap();
        let g = l.create_tag_group("Work").unwrap();
        let t = l.create_tag("Reply", None, Some(&g.id)).unwrap();
        l.add_sticker_tag(&id, &t.id).unwrap();
        (g.id, t.id)
    };
    settle(&[&a, &b], &s, &m);
    {
        let mut l = a.lib.lock().unwrap();
        l.rename_tag(&tag, "新名字").unwrap();
        l.rename_tag_group(&group, "工作").unwrap();
    }
    {
        let l = b.lib.lock().unwrap();
        l.set_tag_color(&tag, "#E5484D").unwrap();
        l.delete_tag_group(&group).unwrap();
    }
    settle(&[&a, &b], &s, &m);
    let t = a
        .lib
        .lock()
        .unwrap()
        .tag_sidebar()
        .unwrap()
        .ungrouped
        .remove(0)
        .tag;
    assert_eq!(t.name, "新名字");
    assert_eq!(t.color, "#E5484D");
    assert_eq!(t.group_id, None);
    a.lib.lock().unwrap().delete_tag(&tag).unwrap();
    b.lib.lock().unwrap().rename_tag(&tag, "迟到改名").unwrap();
    settle(&[&b, &a], &s, &m);
    assert!(a.lib.lock().unwrap().sticker(&id).unwrap().tags.is_empty());
    assert!(
        a.lib
            .lock()
            .unwrap()
            .tag_sidebar()
            .unwrap()
            .ungrouped
            .is_empty()
    );
    assert_eq!(a.state(), b.state());
}

#[test]
fn baseline_migration_clocks_zero_snapshot_retains_stamps_and_switches_library() {
    let a = Site::new();
    let root = a.lib.lock().unwrap().root.clone();
    drop(a.lib);
    // 构造真实的 0001 数据库，不能修改已经执行过的迁移。
    fs::remove_file(root.join("library.db")).unwrap();
    let c = rusqlite::Connection::open(root.join("library.db")).unwrap();
    c.execute_batch(include_str!("../migrations/0001_init.sql"))
        .unwrap();
    c.pragma_update(None, "user_version", 1).unwrap();
    c.execute("INSERT INTO tag_groups VALUES('old','旧分组',0)", [])
        .unwrap();
    drop(c);
    let mut l = Library::open(&root).unwrap();
    let (_, model) = store::load(&l.conn).unwrap();
    assert_eq!(
        model.entities[&Kind::Group]["old"].fields["name"].stamp.at,
        0
    );
    l.rename_tag_group("old", "新分组").unwrap();
    let a = Site {
        dir: a.dir,
        lib: Mutex::new(l),
    };
    let (s, m) = backend();
    a.sync(&s, &m);
    let (s2, m2) = backend();
    a.sync(&s2, &m2);
    let b = Site::new();
    b.sync(&s2, &m2);
    assert_eq!(a.state(), b.state());
    let (_, am) = store::load(&a.lib.lock().unwrap().conn).unwrap();
    let (_, bm) = store::load(&b.lib.lock().unwrap().conn).unwrap();
    assert_eq!(am.entities, bm.entities);
}

#[test]
fn every_public_mutation_is_captured_and_transaction_rollback_has_no_log() {
    let a = Site::new();
    let id = a.import(6);
    let mut l = a.lib.lock().unwrap();
    let mut last = l.local_revision().unwrap();
    assert!(last > 0);
    macro_rules! changed {
        ($e:expr) => {{
            $e.unwrap();
            let next = l.local_revision().unwrap();
            assert!(next > last);
            last = next;
        }};
    }
    changed!(l.create_tag_group("分组"));
    let group = l.tag_sidebar().unwrap().groups[0].id.clone();
    changed!(l.rename_tag_group(&group, "改组名"));
    changed!(l.create_tag("标签", None, None));
    let tag = l.tag_sidebar().unwrap().ungrouped[0].tag.id.clone();
    changed!(l.rename_tag(&tag, "改标签名"));
    changed!(l.set_tag_color(&tag, "#E5484D"));
    changed!(l.move_tag(&tag, Some(&group)));
    changed!(l.add_sticker_tag(&id, &tag));
    changed!(l.remove_sticker_tag(&id, &tag));
    changed!(l.create_tag_and_add(&id, "新标签"));
    changed!(l.set_sticker_caption(&id, "配文"));
    changed!(l.toggle_favorite(&id));
    changed!(l.record_usage(&id));
    changed!(l.delete_tag(&tag));
    changed!(l.delete_tag_group(&group));
    changed!(l.delete_sticker(&id));
    changed!(l.restore_sticker(&id));
    changed!(l.delete_sticker(&id));
    changed!(l.purge_sticker(&id));
    let tx = l.conn.unchecked_transaction().unwrap();
    tx.execute("INSERT INTO tag_groups VALUES('rollback','回滚',0)", [])
        .unwrap();
    drop(tx);
    assert_eq!(l.local_revision().unwrap(), last);
    drop(l);
    let id = a.import(6);
    let mut l = a.lib.lock().unwrap();
    l.delete_sticker(&id).unwrap();
    l.conn
        .execute("UPDATE stickers SET deleted_at=1 WHERE id=?1", [id])
        .unwrap();
    last = l.local_revision().unwrap();
    l.purge_expired().unwrap();
    assert!(l.local_revision().unwrap() > last);
}

#[test]
fn folder_validation_atomic_files_and_original_repair() {
    let a = Site::new();
    let root = a.lib.lock().unwrap().root.clone();
    for p in [&root, &root.join("objects"), a.dir.path()] {
        assert!(inspect_folder(p, &root).is_err());
    }
    let other = tempfile::tempdir().unwrap();
    fs::write(other.path().join("notes"), "keep").unwrap();
    assert!(inspect_folder(other.path(), &root).is_err());
    // 访达和 Windows 自动放的文件不算内容，仍可在这里新建仓库。
    let fresh = tempfile::tempdir().unwrap();
    fs::write(fresh.path().join(".DS_Store"), "x").unwrap();
    fs::write(fresh.path().join("desktop.ini"), "x").unwrap();
    assert!(
        inspect_folder(fresh.path(), &root)
            .unwrap()
            .manifest
            .is_none()
    );
    Manifest::create(&FolderStorage::new(fresh.path()).unwrap()).unwrap();
    let folder = tempfile::tempdir().unwrap();
    let s = FolderStorage::new(folder.path()).unwrap();
    s.write("a/file", b"first").unwrap();
    s.write("a/file", b"second").unwrap();
    assert_eq!(s.read("a/file").unwrap().unwrap(), b"second");
    assert_eq!(s.list("").unwrap().len(), 1);
    assert!(s.write("../outside", b"no").is_err());
    let (s, m) = backend();
    let id = a.import(7);
    a.sync(&s, &m);
    let key = a
        .lib
        .lock()
        .unwrap()
        .sticker(&id)
        .unwrap()
        .object_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    s.delete(&format!("objects/{key}")).unwrap();
    a.sync(&s, &m);
    assert!(s.read(&format!("objects/{key}")).unwrap().is_some());
}

#[test]
fn backend_io_never_holds_library_lock_and_failure_retries_the_same_segment() {
    struct Checked<'a> {
        lib: &'a Mutex<Library>,
        inner: MemoryStorage,
        fail: std::sync::atomic::AtomicBool,
    }
    impl Storage for Checked<'_> {
        fn list(&self, p: &str) -> Result<Vec<storage::FileEntry>> {
            assert!(self.lib.try_lock().is_ok());
            self.inner.list(p)
        }
        fn read(&self, k: &str) -> Result<Option<Vec<u8>>> {
            assert!(self.lib.try_lock().is_ok());
            self.inner.read(k)
        }
        fn write(&self, k: &str, b: &[u8]) -> Result<()> {
            assert!(self.lib.try_lock().is_ok());
            self.inner.write(k, b)?;
            if k.starts_with("ops/") && self.fail.swap(false, std::sync::atomic::Ordering::SeqCst) {
                return Err(Error::Sync("模拟写成后未记账".into()));
            }
            Ok(())
        }
        fn delete(&self, k: &str) -> Result<()> {
            assert!(self.lib.try_lock().is_ok());
            self.inner.delete(k)
        }
    }
    let (s, m) = backend();
    let a = Site::new();
    a.import(8);
    let checked = Checked {
        lib: &a.lib,
        inner: s.clone(),
        fail: std::sync::atomic::AtomicBool::new(true),
    };
    assert!(synchronize(&a.lib, &checked, &m.id).is_err());
    let before = s.list("ops/").unwrap();
    a.sync(&checked, &m);
    assert_eq!(s.list("ops/").unwrap(), before);
    let b = Site::new();
    b.sync(&s, &m);
    assert_eq!(a.state(), b.state());
}

#[test]
fn three_libraries_converge_under_seeded_random_edits_and_delayed_delivery() {
    for seed in [1_u64, 7, 42, 2026, 0xdeadbeef] {
        let (s, m) = backend();
        let sites = [Site::new(), Site::new(), Site::new()];
        let mut rng = seed;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        for a in &sites {
            for color in 10..13 {
                a.import(color);
            }
            a.lib.lock().unwrap().create_tag_group("共同分组").unwrap();
        }
        s.hold(true);
        for _ in 0..120 {
            let index = (next() % 3) as usize;
            let site = &sites[index];
            let action = next() % 12;
            if action == 0 {
                site.import((10 + next() % 3) as u8);
            } else {
                let mut l = site.lib.lock().unwrap();
                let all = l.list_stickers(LibraryView::All).unwrap();
                let deleted = l.list_stickers(LibraryView::RecentlyDeleted).unwrap();
                let id = all
                    .get((next() as usize) % all.len().max(1))
                    .map(|v| v.id.clone());
                match (action, id) {
                    (1, Some(id)) => {
                        let _ = l
                            .create_tag_and_add(&id, if next() % 2 == 0 { "same" } else { "SAME" });
                    }
                    (2, Some(id)) => {
                        l.set_sticker_caption(&id, &format!("配文{}", next() % 5))
                            .unwrap();
                    }
                    (3, Some(id)) => {
                        l.toggle_favorite(&id).unwrap();
                    }
                    (4, Some(id)) => {
                        l.record_usage(&id).unwrap();
                    }
                    (5, Some(id)) => {
                        l.delete_sticker(&id).unwrap();
                    }
                    (6, _) if !deleted.is_empty() => {
                        l.restore_sticker(&deleted[0].id).unwrap();
                    }
                    (7, _) if !deleted.is_empty() => {
                        l.purge_sticker(&deleted[0].id).unwrap();
                    }
                    (8, Some(id)) => {
                        let ts = l.tag_sidebar().unwrap();
                        let t = ts
                            .ungrouped
                            .first()
                            .or_else(|| ts.groups.iter().flat_map(|g| &g.tags).next());
                        if let Some(t) = t {
                            l.add_sticker_tag(&id, &t.tag.id).unwrap();
                        }
                    }
                    (9, Some(id)) => {
                        let v = l.sticker(&id).unwrap();
                        if let Some(t) = v.tags.first() {
                            l.remove_sticker_tag(&id, &t.id).unwrap();
                        }
                    }
                    (10, _) => {
                        let _ = l.create_tag_group("共同分组");
                    }
                    _ => {}
                }
            }
            if next() % 2 == 0 {
                site.sync(&s, &m);
            }
            let keys = s.pending_keys();
            if !keys.is_empty() {
                s.deliver(&keys[(next() as usize) % keys.len()]);
            }
        }
        settle(&[&sites[2], &sites[0], &sites[1]], &s, &m);
        let expected = sites[0].state();
        for site in &sites[1..] {
            assert_eq!(expected, site.state(), "seed {seed}");
        }
        for site in &sites {
            assert_eq!(site.sync(&s, &m).pending, 0, "seed {seed}");
        }
    }
}

#[test]
fn tie_breaker_field_positions_and_relationship_lww_are_order_independent() {
    let mut model = Model::default();
    let available = BTreeSet::new();
    let create = remote_tag("a", "tag", 0, "original");
    model.apply(&create, &available).unwrap();
    let earlier = change(
        10,
        "a",
        Action::Set {
            entity: Kind::Tag,
            id: "tag".into(),
            field: "name".into(),
            value: json!("earlier"),
        },
    );
    let winner = change(
        10,
        "z",
        Action::Set {
            entity: Kind::Tag,
            id: "tag".into(),
            field: "name".into(),
            value: json!("winner"),
        },
    );
    let position = change(
        8,
        "a",
        Action::Set {
            entity: Kind::Tag,
            id: "tag".into(),
            field: "position".into(),
            value: json!(12),
        },
    );
    let mut reversed = model.clone();
    for c in [&winner, &earlier, &position] {
        model.apply(c, &available).unwrap();
    }
    for c in [&position, &earlier, &winner] {
        reversed.apply(c, &available).unwrap();
    }
    assert_eq!(model, reversed);
    assert_eq!(
        model.projected(Kind::Tag)["tag"].fields["name"].value,
        "winner"
    );
    assert_eq!(
        model.projected(Kind::Tag)["tag"].fields["position"].value,
        12
    );
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let id = a.import(31);
    let tag = a
        .lib
        .lock()
        .unwrap()
        .create_tag_and_add(&id, "关系")
        .unwrap()
        .id;
    settle(&[&a, &b], &s, &m);
    a.lib.lock().unwrap().remove_sticker_tag(&id, &tag).unwrap();
    settle(&[&a, &b], &s, &m);
    assert!(b.lib.lock().unwrap().sticker(&id).unwrap().tags.is_empty());
    b.lib.lock().unwrap().add_sticker_tag(&id, &tag).unwrap();
    settle(&[&b, &a], &s, &m);
    assert_eq!(a.lib.lock().unwrap().sticker(&id).unwrap().tags.len(), 1);
}

#[test]
fn duplicate_group_names_merge_with_members_and_late_group_reference() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let (ga, ta) = {
        let mut l = a.lib.lock().unwrap();
        let g = l.create_tag_group("Work").unwrap().id;
        let t = l.create_tag("a", None, Some(&g)).unwrap().id;
        (g, t)
    };
    let gb = {
        let mut l = b.lib.lock().unwrap();
        let g = l.create_tag_group("WORK").unwrap().id;
        l.create_tag("b", None, Some(&g)).unwrap();
        g
    };
    settle(&[&a, &b], &s, &m);
    let sidebar = a.lib.lock().unwrap().tag_sidebar().unwrap();
    assert_eq!(sidebar.groups.len(), 1);
    assert_eq!(sidebar.groups[0].tags.len(), 2);
    assert_eq!(sidebar.groups[0].id, ga.clone().min(gb.clone()));
    let d = Uuid::new_v4().to_string();
    let future = unix_millis().unwrap() + 1000;
    write_ops(
        &s,
        &d,
        1,
        &[change(
            future,
            &d,
            Action::Set {
                entity: Kind::Tag,
                id: ta,
                field: "group_id".into(),
                value: json!(ga.max(gb)),
            },
        )],
    );
    settle(&[&a, &b], &s, &m);
    assert_eq!(a.state(), b.state());
    assert_eq!(
        a.lib.lock().unwrap().tag_sidebar().unwrap().groups[0]
            .tags
            .len(),
        2
    );
}

#[test]
fn pending_reference_can_arrive_before_its_creation_on_another_device() {
    let (s, m) = backend();
    let a = Site::new();
    let d = Uuid::new_v4().to_string();
    let e = Uuid::new_v4().to_string();
    write_ops(
        &s,
        &d,
        1,
        &[change(
            10,
            &d,
            Action::Set {
                entity: Kind::Tag,
                id: "late".into(),
                field: "name".into(),
                value: json!("修改先到"),
            },
        )],
    );
    assert_eq!(a.sync(&s, &m).pending, 1);
    assert_eq!(
        store::cursor(&a.lib.lock().unwrap().conn, &m.id, &d).unwrap(),
        1
    );
    write_ops(&s, &e, 1, &[remote_tag(&e, "late", 1, "创建晚到")]);
    assert_eq!(a.sync(&s, &m).pending, 0);
    assert_eq!(
        a.lib.lock().unwrap().tag_sidebar().unwrap().ungrouped[0]
            .tag
            .name,
        "修改先到"
    );
}

#[test]
fn usage_batch_is_absolute_and_device_rename_is_atomic_and_local() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let id = a.import(32);
    settle(&[&a, &b], &s, &m);
    {
        let mut l = a.lib.lock().unwrap();
        for _ in 0..5 {
            l.record_usage(&id).unwrap();
        }
        l.rename_device("工作电脑").unwrap();
        assert!(l.rename_device(" ").is_err());
    }
    a.sync(&s, &m);
    b.sync(&s, &m);
    assert_eq!(b.lib.lock().unwrap().sticker(&id).unwrap().usage_count, 5);
    let device = a.lib.lock().unwrap().device.id.clone();
    let ops = segments(&s)
        .unwrap()
        .into_iter()
        .filter(|s| s.device == device)
        .flat_map(|seg| read_segment(&s, &seg).unwrap().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        ops.iter()
            .filter(|o| matches!(o.change.action, Action::Usage { .. }))
            .count(),
        1
    );
    let root = a.lib.lock().unwrap().root.clone();
    assert_eq!(Library::open(&root).unwrap().device.name, "工作电脑");
    assert_eq!(
        read_devices(&s)
            .unwrap()
            .into_iter()
            .find(|d| d.id == device)
            .unwrap()
            .name,
        "工作电脑"
    );
}

#[test]
fn reimport_on_another_device_survives_merge_with_delayed_purge() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let c = Site::new();
    let old = a.import(33);
    {
        let mut l = a.lib.lock().unwrap();
        l.create_tag_and_add(&old, "旧关系").unwrap();
        l.record_usage(&old).unwrap();
    }
    settle(&[&a, &b, &c], &s, &m);
    let before = s
        .list("ops/")
        .unwrap()
        .into_iter()
        .map(|f| f.key)
        .collect::<BTreeSet<_>>();
    {
        let mut l = a.lib.lock().unwrap();
        l.delete_sticker(&old).unwrap();
        l.purge_sticker(&old).unwrap();
    }
    a.sync(&s, &m);
    b.sync(&s, &m);
    let a_device = a.lib.lock().unwrap().device.id.clone();
    let mut delayed = Vec::new();
    for f in s.list(&format!("ops/{a_device}/")).unwrap() {
        if !before.contains(&f.key) {
            delayed.push((f.key.clone(), s.read(&f.key).unwrap().unwrap()));
            s.delete(&f.key).unwrap();
        }
    }
    let new = b.import(33);
    b.sync(&s, &m);
    c.sync(&s, &m);
    for (key, bytes) in delayed {
        s.write(&key, &bytes).unwrap();
    }
    settle(&[&a, &b, &c], &s, &m);
    for site in [&a, &b, &c] {
        let l = site.lib.lock().unwrap();
        let v = l.sticker(&new).unwrap();
        assert!(v.tags.is_empty());
        assert_eq!(v.usage_count, 0);
        assert!(l.sticker(&old).is_err());
    }
}

#[test]
fn explicit_join_detects_cloned_identity_before_snapshot_sequence_is_allocated() {
    let (s, m) = backend();
    let a = Site::new();
    let device = a.lib.lock().unwrap().device.id.clone();
    write_ops(
        &s,
        &device,
        1,
        &[remote_tag(&device, "conflict", 1, "冲突")],
    );
    assert!(matches!(
        join(&a.lib, &s, &m.id),
        Err(Error::DeviceConflict)
    ));
    assert_eq!(
        a.lib
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT COUNT(*) FROM sync_targets", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn remote_purge_removes_local_files_and_backend_object() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let id = a.import(34);
    settle(&[&a, &b], &s, &m);
    let old = b.lib.lock().unwrap().sticker(&id).unwrap();
    {
        let mut l = a.lib.lock().unwrap();
        l.delete_sticker(&id).unwrap();
        l.purge_sticker(&id).unwrap();
    }
    a.sync(&s, &m);
    b.sync(&s, &m);
    assert!(!old.object_path.exists());
    assert!(!old.thumbnail_path.exists());
    assert!(s.list("objects/").unwrap().is_empty());
}

#[test]
fn late_joiner_keeps_its_copy_of_an_image_purged_after_sync() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let old = b.import(35);
    b.lib
        .lock()
        .unwrap()
        .create_tag_and_add(&old, "删除前")
        .unwrap();
    b.sync(&s, &m);
    {
        let mut l = b.lib.lock().unwrap();
        l.delete_sticker(&old).unwrap();
        l.purge_sticker(&old).unwrap();
    }
    b.sync(&s, &m);
    // A 加入前就有这张图；读到 B 的第一段时两张都还活着，会先合并，之后才读到彻底删除。
    let mine = a.import(35);
    a.sync(&s, &m);
    settle(&[&a, &b], &s, &m);
    for site in [&a, &b] {
        let l = site.lib.lock().unwrap();
        let v = l.sticker(&mine).unwrap();
        assert!(v.deleted_at.is_none());
        assert!(v.tags.is_empty());
        assert!(v.object_path.exists());
        assert!(l.sticker(&old).is_err());
    }
    assert_eq!(a.state(), b.state());
}

#[test]
fn purging_a_merged_sticker_lists_and_removes_every_known_copy() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    let ia = a.import(36);
    let ib = b.import(36);
    settle(&[&a, &b], &s, &m);
    let local = ia.clone().min(ib.clone());
    {
        let mut l = a.lib.lock().unwrap();
        l.delete_sticker(&local).unwrap();
        l.purge_sticker(&local).unwrap();
    }
    settle(&[&a, &b], &s, &m);
    for site in [&a, &b] {
        let l = site.lib.lock().unwrap();
        assert!(l.list_stickers(LibraryView::All).unwrap().is_empty());
        assert!(
            l.list_stickers(LibraryView::RecentlyDeleted)
                .unwrap()
                .is_empty()
        );
    }
    let purged: String = a
        .lib
        .lock()
        .unwrap()
        .conn
        .query_row(
            "SELECT action FROM sync_outbox WHERE action LIKE '%\"purge\"%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut both = vec![ia, ib];
    both.sort();
    assert_eq!(
        serde_json::from_str::<Value>(&purged).unwrap()["ids"],
        json!(both)
    );
}

#[test]
fn incomplete_manifest_and_device_wait_without_applying_partial_data() {
    let (storage, manifest) = backend();
    let site = Site::new();
    site.import(81);
    storage
        .write("library.json", br#"{"format":1,"id":"#)
        .unwrap();
    let report = synchronize(&site.lib, &storage, &manifest.id).unwrap();
    assert_eq!(report.last_sync_at, None);
    assert_eq!(report.pending, 1);
    assert!(storage.list("ops/").unwrap().is_empty());
    storage
        .write("library.json", &serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    storage
        .write(&format!("devices/{}.json", Uuid::new_v4()), b"{")
        .unwrap();
    site.sync(&storage, &manifest);
    assert_eq!(inspect(&storage).unwrap().devices, 1);
}
#[test]
fn missing_manifest_is_an_error_not_a_quiet_wait() {
    let (storage, manifest) = backend();
    let site = Site::new();
    site.sync(&storage, &manifest);
    let segments = storage.list("ops/").unwrap();
    storage.delete("library.json").unwrap();
    site.import(83);
    let error = synchronize(&site.lib, &storage, &manifest.id).unwrap_err();
    assert!(error.to_string().contains("找不到 library.json"), "{error}");
    assert_eq!(storage.list("ops/").unwrap(), segments);
}
#[test]
fn progress_is_outside_repository_lock_and_partial_object_is_repaired() {
    let (storage, manifest) = backend();
    let a = Site::new();
    let b = Site::new();
    a.import(82);
    a.sync(&storage, &manifest);
    let object = storage.list("objects/").unwrap().pop().unwrap();
    storage.write(&object.key, b"partial").unwrap();
    let updates = Mutex::new(Vec::new());
    synchronize_with_progress(&a.lib, &storage, &manifest.id, &|p| {
        assert!(a.lib.try_lock().is_ok());
        updates.lock().unwrap().push(p);
    })
    .unwrap();
    assert_eq!(
        storage.read(&object.key).unwrap().unwrap().len() as u64,
        object.size
    );
    assert!(
        updates
            .lock()
            .unwrap()
            .iter()
            .any(|p| matches!(p.stage, TransferStage::Uploading) && p.total == Some(p.completed))
    );
    let downloads = Mutex::new(Vec::new());
    synchronize_with_progress(&b.lib, &storage, &manifest.id, &|p| {
        assert!(b.lib.try_lock().is_ok());
        if matches!(p.stage, TransferStage::Downloading) {
            downloads.lock().unwrap().push((p.completed, p.total));
        }
    })
    .unwrap();
    assert_eq!(*downloads.lock().unwrap(), [(0, None), (1, None)]);
    assert_eq!(a.state(), b.state());
}

mod network_integration;

mod presence;
