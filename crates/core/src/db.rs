use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::{Error, Result};

/// 按顺序执行的迁移，执行完第 n 条后 `user_version` 记为 n。
/// 已经发布过的迁移不要再改，只往后追加。
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_init.sql"),
    include_str!("migrations/0002_sync.sql"),
    include_str!("migrations/0003_sync_presence.sql"),
    include_str!("migrations/0004_categories.sql"),
    include_str!("migrations/0005_sync_tree.sql"),
    include_str!("migrations/0006_drop_sync_deleted_objects.sql"),
];
const LATEST_VERSION: u32 = MIGRATIONS.len() as u32;

pub fn open(path: &Path) -> Result<Connection> {
    if path.exists() {
        // 先只读检查，再切换 WAL 或迁移，避免改动无法打开的仓库。
        let existing = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        check_version(&existing)?;
        let check: String = existing.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        if check != "ok" {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
                Some(check),
            )
            .into());
        }
    }
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn user_version(conn: &Connection) -> Result<u32> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

fn check_version(conn: &Connection) -> Result<u32> {
    let found = user_version(conn)?;
    if found > LATEST_VERSION {
        return Err(Error::SchemaTooNew {
            found,
            supported: LATEST_VERSION,
        });
    }
    Ok(found)
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let found = check_version(conn)?;
    for (version, sql) in (1..).zip(MIGRATIONS).skip(found as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopening_does_not_rerun_migrations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        open(&path).unwrap();
        let conn = open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), LATEST_VERSION);
        assert!(!conn.table_exists(None, "sync_deleted_objects").unwrap());
    }

    #[test]
    fn removes_unused_deleted_objects_table_without_changing_library_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        for sql in &MIGRATIONS[..5] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 5).unwrap();
        conn.execute_batch(
            "INSERT INTO sync_deleted_objects VALUES ('library', 'objects/deleted.png');
             INSERT INTO stickers (id, hash, ext, name, width, height, byte_size, added_at)
             VALUES ('kept', 'hash', 'png', '保留的表情', 1, 1, 1, 1);
             INSERT INTO sync_file_dirty VALUES ('kept', 7);",
        )
        .unwrap();
        drop(conn);

        let conn = open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), LATEST_VERSION);
        assert!(!conn.table_exists(None, "sync_deleted_objects").unwrap());
        let preserved: (String, i64) = conn
            .query_row(
                "SELECT s.name, d.revision FROM stickers s
                 JOIN sync_file_dirty d ON d.sticker_id = s.id WHERE s.id = 'kept'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(preserved, ("保留的表情".into(), 7));
    }

    #[test]
    fn refuses_database_from_newer_app() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", LATEST_VERSION + 1)
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(matches!(open(&path), Err(Error::SchemaTooNew { .. })));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!dir.path().join("library.db-wal").exists());
    }

    #[test]
    fn rejects_corrupt_database_without_changing_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let bytes = b"not a sqlite database";
        std::fs::write(&path, bytes).unwrap();
        assert!(open(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn migration_to_categories_keeps_the_first_tag_and_drops_old_sync_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        for sql in &MIGRATIONS[..3] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 3).unwrap();
        conn.execute_batch(
            "INSERT INTO stickers (id, hash, ext, name, width, height, byte_size, added_at) VALUES
                ('multi', 'h1', 'png', 'a', 1, 1, 1, 1),
                ('single', 'h2', 'png', 'b', 1, 1, 1, 2),
                ('none', 'h3', 'png', 'c', 1, 1, 1, 3);
             INSERT INTO tag_groups VALUES ('g', '分组', 0);
             INSERT INTO tags VALUES ('late', '后面', '#F2B705', 'g', 5), ('early', '前面', '#3E7BFA', NULL, 1);
             INSERT INTO sticker_tags VALUES ('multi', 'late'), ('multi', 'early'), ('single', 'late');
             INSERT INTO sync_model VALUES (1, 0, '{}');
             UPDATE sync_meta SET recording = 1;",
        )
        .unwrap();
        drop(conn);

        let conn = open(&path).unwrap();
        let category = |id: &str| -> Option<String> {
            conn.query_row(
                "SELECT category_id FROM stickers WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(category("multi").as_deref(), Some("early"));
        assert_eq!(category("single").as_deref(), Some("late"));
        assert_eq!(category("none"), None);
        let names: Vec<String> = conn
            .prepare("SELECT name FROM categories ORDER BY position")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(names, ["前面", "后面"]);
        for table in ["tags", "tag_groups", "sticker_tags"] {
            assert!(conn.prepare(&format!("SELECT 1 FROM {table}")).is_err());
        }
        let stale: i64 = conn
            .query_row("SELECT COUNT(*) FROM sync_model", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stale, 0);
        // 迁移期间的改动不能写进旧的操作日志。
        let logged: i64 = conn
            .query_row("SELECT COUNT(*) FROM sync_outbox", [], |r| r.get(0))
            .unwrap();
        assert_eq!(logged, 0);
    }
}
