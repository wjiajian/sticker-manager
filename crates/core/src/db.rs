use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::{Error, Result};

/// 按顺序执行的迁移，执行完第 n 条后 `user_version` 记为 n。
/// 已经发布过的迁移不要再改，只往后追加。
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_init.sql"),
    include_str!("migrations/0002_sync.sql"),
    include_str!("migrations/0003_sync_presence.sql"),
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
}
