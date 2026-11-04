use super::*;
use crate::sync::{compaction, storage::FileEntry};

fn sync_without_compaction(a: &Site, s: &dyn Storage, m: &Manifest) {
    synchronize_with_maintenance(&a.lib, s, &m.id, &|_| {}, &|_| {}, &|| false).unwrap();
}
fn own(s: &dyn Storage, a: &Site) -> Vec<Segment> {
    let id = a.lib.lock().unwrap().device.id.clone();
    segments(s)
        .unwrap()
        .into_iter()
        .filter(|s| s.device == id)
        .collect()
}
fn populate(a: &Site, s: &dyn Storage, m: &Manifest, n: usize) -> String {
    let id = a.import(220);
    for i in 1..=n {
        a.lib
            .lock()
            .unwrap()
            .set_sticker_caption(&id, &format!("第 {i} 轮"))
            .unwrap();
        sync_without_compaction(a, s, m);
    }
    id
}
struct Tracked<'a> {
    inner: &'a dyn Storage,
    calls: Mutex<Vec<char>>,
    fail_delete_after: usize,
    fail_write: bool,
}
impl<'a> Tracked<'a> {
    fn new(inner: &'a dyn Storage) -> Self {
        Self {
            inner,
            calls: Mutex::new(Vec::new()),
            fail_delete_after: usize::MAX,
            fail_write: false,
        }
    }
}
impl Storage for Tracked<'_> {
    fn list(&self, p: &str) -> Result<Vec<FileEntry>> {
        self.calls.lock().unwrap().push('l');
        self.inner.list(p)
    }
    fn read(&self, k: &str) -> Result<Option<Vec<u8>>> {
        self.calls.lock().unwrap().push('r');
        self.inner.read(k)
    }
    fn write(&self, k: &str, b: &[u8]) -> Result<()> {
        self.calls.lock().unwrap().push('w');
        if self.fail_write && k.starts_with(".sticker-manager/ops/") {
            return Err(Error::Sync("模拟写失败".into()));
        }
        self.inner.write(k, b)
    }
    fn delete(&self, k: &str) -> Result<()> {
        let mut c = self.calls.lock().unwrap();
        if c.iter().filter(|c| **c == 'd').count() >= self.fail_delete_after {
            return Err(Error::Sync("模拟删除中断".into()));
        }
        c.push('d');
        drop(c);
        self.inner.delete(k)
    }
}

#[test]
fn hundred_segments_converge_at_cursors_0_37_100_and_new_joiner_with_exact_budget() {
    let (s, m) = backend();
    let a = Site::new();
    let b0 = Site::new();
    let b37 = Site::new();
    let b100 = Site::new();
    let id = populate(&a, &s, &m, 37);
    b37.sync(&s, &m);
    let device = a.lib.lock().unwrap().device.id.clone();
    assert_eq!(
        store::cursor(&b37.lib.lock().unwrap().conn, &m.id, &device).unwrap(),
        38 // 首轮多一条原图路径声明。
    );
    for i in 38..=100 {
        a.lib
            .lock()
            .unwrap()
            .set_sticker_caption(&id, &format!("第 {i} 轮"))
            .unwrap();
        sync_without_compaction(&a, &s, &m);
    }
    b100.sync(&s, &m);
    assert_eq!(own(&s, &a).len(), 100);
    let counted = Tracked::new(&s);
    compaction::compact(&counted, own(&s, &a), &|| true).unwrap();
    let calls = counted.calls.lock().unwrap();
    for (op, n) in [('r', 100), ('w', 1), ('l', 1), ('d', 100)] {
        assert_eq!(calls.iter().filter(|c| **c == op).count(), n);
    }
    drop(calls);
    assert_eq!(own(&s, &a).len(), 1);
    let c = Site::new();
    join(&c.lib, &s, &m.id).unwrap();
    for b in [&b0, &b37, &b100, &c] {
        b.sync(&s, &m);
        assert_eq!(a.state(), b.state());
    }
    // 合并段不改变本机 next_seq，不会被误判为身份冲突。
    a.sync(&s, &m);
}

#[test]
fn partial_delete_overlaps_apply_once_then_resume_without_rewriting() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    populate(&a, &s, &m, 100);
    let mut fail = Tracked::new(&s);
    fail.fail_delete_after = 40;
    assert!(compaction::compact(&fail, own(&s, &a), &|| true).is_err());
    assert_eq!(own(&s, &a).len(), 61);
    b.sync(&s, &m);
    b.sync(&s, &m);
    assert_eq!(a.state(), b.state());
    let resume = Tracked::new(&s);
    compaction::compact(&resume, own(&s, &a), &|| true).unwrap();
    assert!(!resume.calls.lock().unwrap().contains(&'w'));
    assert_eq!(own(&s, &a).len(), 1);
}

#[test]
fn bad_source_write_failure_and_not_yet_visible_output_never_delete_sources() {
    let (s, m) = backend();
    let a = Site::new();
    populate(&a, &s, &m, 32);
    let first = own(&s, &a)[0].key.clone();
    let original = s.read(&first).unwrap().unwrap();
    for corrupted in [b"bad\n".to_vec(), original[..original.len() - 1].to_vec()] {
        s.write(&first, &corrupted).unwrap();
        let t = Tracked::new(&s);
        assert!(compaction::compact(&t, own(&s, &a), &|| true).is_err());
        let calls = t.calls.lock().unwrap();
        assert!(!calls.contains(&'d') && !calls.contains(&'w'));
    }
    s.write(&first, &original).unwrap();
    let mut t = Tracked::new(&s);
    t.fail_write = true;
    assert!(compaction::compact(&t, own(&s, &a), &|| true).is_err());
    assert!(!t.calls.lock().unwrap().contains(&'d'));
    s.hold(true);
    assert!(compaction::compact(&s, own(&s, &a), &|| true).is_err());
    assert_eq!(own(&s, &a).len(), 32);
    s.deliver_all();
    s.hold(false);
    assert_eq!(own(&s, &a).len(), 33);
    compaction::compact(&s, own(&s, &a), &|| true).unwrap();
    assert_eq!(own(&s, &a).len(), 1);
}

#[test]
fn listed_source_removed_before_read_waits_for_next_round_merged_segment() {
    struct Race<'a> {
        s: &'a MemoryStorage,
        a: &'a Site,
        once: Mutex<bool>,
    }
    impl Storage for Race<'_> {
        fn list(&self, p: &str) -> Result<Vec<FileEntry>> {
            self.s.list(p)
        }
        fn read(&self, k: &str) -> Result<Option<Vec<u8>>> {
            if k.starts_with(".sticker-manager/ops/")
                && !std::mem::replace(&mut *self.once.lock().unwrap(), true)
            {
                compaction::compact(self.s, own(self.s, self.a), &|| true)?;
            }
            self.s.read(k)
        }
        fn write(&self, k: &str, b: &[u8]) -> Result<()> {
            self.s.write(k, b)
        }
        fn delete(&self, k: &str) -> Result<()> {
            self.s.delete(k)
        }
    }
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    populate(&a, &s, &m, 32);
    let race = Race {
        s: &s,
        a: &a,
        once: Mutex::new(false),
    };
    b.sync(&race, &m);
    let device = a.lib.lock().unwrap().device.id.clone();
    assert_eq!(
        store::cursor(&b.lib.lock().unwrap().conn, &m.id, &device).unwrap(),
        0
    );
    b.sync(&s, &m);
    assert_eq!(a.state(), b.state());
}

#[test]
fn continued_push_seals_full_segments_and_bounds_sources_and_cancellation() {
    let (s, m) = backend();
    let a = Site::new();
    let id = a.import(221);
    let large = "字".repeat(50_000);
    for i in 0..32 {
        a.lib
            .lock()
            .unwrap()
            .set_sticker_caption(&id, &format!("{i}{large}"))
            .unwrap();
        sync_without_compaction(&a, &s, &m);
    }
    compaction::compact(&s, own(&s, &a), &|| true).unwrap();
    let sealed = own(&s, &a)
        .into_iter()
        .find(|s| s.size >= 4 * 1024 * 1024)
        .unwrap();
    let bytes = s.read(&sealed.key).unwrap().unwrap();
    for i in 32..60 {
        a.lib
            .lock()
            .unwrap()
            .set_sticker_caption(&id, &format!("{i}{large}"))
            .unwrap();
        sync_without_compaction(&a, &s, &m);
    }
    let t = Tracked::new(&s);
    compaction::compact(&t, own(&s, &a), &|| false).unwrap();
    assert!(t.calls.lock().unwrap().is_empty());
    compaction::compact(&s, own(&s, &a), &|| true).unwrap();
    assert_eq!(s.read(&sealed.key).unwrap().unwrap(), bytes);
    assert_eq!(
        own(&s, &a)
            .iter()
            .filter(|s| s.size >= 4 * 1024 * 1024)
            .count(),
        2
    );
    // 另一个目录留存 140 个小段，一轮最多读并删除 128 个。
    let (s, m) = backend();
    let a = Site::new();
    populate(&a, &s, &m, 140);
    compaction::compact(&s, own(&s, &a), &|| true).unwrap();
    assert_eq!(own(&s, &a).len(), 13);
}

#[test]
fn failed_push_retains_unpushed_stream_and_retries_after_outbox_cleanup() {
    let (s, m) = backend();
    let a = Site::new();
    a.import(222);
    let mut fail = Tracked::new(&s);
    fail.fail_write = true;
    assert!(synchronize(&a.lib, &fail, &m.id).is_err());
    let l = a.lib.lock().unwrap();
    assert_eq!(
        l.conn
            .query_row("SELECT COUNT(*) FROM sync_stream", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2 // 快照和原图路径声明都必须留在未发布队列。
    );
    drop(l);
    a.sync(&s, &m);
    let b = Site::new();
    b.sync(&s, &m);
    assert_eq!(a.state(), b.state());
}

#[test]
fn cancellation_after_write_leaves_sources_and_next_round_resumes() {
    let (s, m) = backend();
    let a = Site::new();
    populate(&a, &s, &m, 32);
    let t = Tracked::new(&s);
    compaction::compact(&t, own(&s, &a), &|| !t.calls.lock().unwrap().contains(&'w')).unwrap();
    assert_eq!(own(&s, &a).len(), 33);
    assert!(!t.calls.lock().unwrap().contains(&'d'));
    compaction::compact(&s, own(&s, &a), &|| true).unwrap();
    assert_eq!(own(&s, &a).len(), 1);
}

#[test]
fn broken_leftovers_from_interrupted_puts_do_not_block_merging_forever() {
    let (s, m) = backend();
    let a = Site::new();
    let b = Site::new();
    populate(&a, &s, &m, 32);
    let sources = own(&s, &a);
    let (device, first, last) = (
        sources[0].device.clone(),
        sources[0].first,
        sources.last().unwrap().last,
    );
    // 上次合并的写入断在中途，后端留下了半截覆盖段，源段都还在。
    let cover = format!(".sticker-manager/ops/{device}/{first:012}-{last:012}.jsonl");
    let merged = sources
        .iter()
        .flat_map(|source| s.read(&source.key).unwrap().unwrap())
        .collect::<Vec<_>>();
    s.write(&cover, &merged[..merged.len() - 3]).unwrap();
    compaction::compact(&s, own(&s, &a), &|| true).unwrap();
    assert!(s.read(&cover).unwrap().is_none());
    assert_eq!(own(&s, &a).len(), 32);
    compaction::compact(&s, own(&s, &a), &|| true).unwrap();
    assert_eq!(own(&s, &a).len(), 1);
    // 推送写到一半的残段，之后被完整段覆盖：残段删掉，完整段不动。
    let leftover = format!(
        ".sticker-manager/ops/{device}/{first:012}-{:012}.jsonl",
        first + 1
    );
    s.write(&leftover, b"{\"seq\":").unwrap();
    compaction::compact(&s, own(&s, &a), &|| true).unwrap();
    assert!(s.read(&leftover).unwrap().is_none());
    assert_eq!(s.read(&cover).unwrap().unwrap(), merged);
    b.sync(&s, &m);
    assert_eq!(a.state(), b.state());
}
