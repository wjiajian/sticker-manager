use std::path::Path;

use rusqlite::Connection;

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
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn user_version(conn: &Connection) -> Result<u32> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let found = user_version(conn)?;
    if found > LATEST_VERSION {
        return Err(Error::SchemaTooNew {
            found,
            supported: LATEST_VERSION,
        });
    }
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
        assert!(matches!(open(&path), Err(Error::SchemaTooNew { .. })));
    }
}
