use std::collections::HashSet;
use std::fs;
use std::time::{Duration, SystemTime};

use rusqlite::{Params, params};

use crate::{Error, Library, Result, time::unix_millis};

pub(crate) const RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// 没有表情记录的文件放这么久才清理：同步先下载原图，合并完日志才写表情记录，中间可能隔很久。
const ORPHAN_GRACE: Duration = Duration::from_secs(24 * 60 * 60);

impl Library {
    pub fn delete_sticker(&self, id: &str) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE stickers SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            params![id, unix_millis()?],
        )?;
        if changed == 0 {
            return Err(Error::StickerUnavailable);
        }
        Ok(())
    }

    pub fn restore_sticker(&self, id: &str) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE stickers SET deleted_at = NULL WHERE id = ?1 AND deleted_at IS NOT NULL",
            [id],
        )?;
        if changed == 0 {
            return Err(Error::StickerNotDeleted);
        }
        Ok(())
    }

    /// 一次删除一批表情，返回实际移进最近删除的 id；已删除或已不存在的跳过。
    pub fn delete_stickers(&mut self, ids: &[String]) -> Result<Vec<String>> {
        self.set_deleted_at(Some(unix_millis()?), "deleted_at IS NULL", ids)
    }

    /// 一次恢复一批表情，返回实际恢复的 id；不在最近删除里的跳过。撤销删除用它。
    pub fn restore_stickers(&mut self, ids: &[String]) -> Result<Vec<String>> {
        self.set_deleted_at(None, "deleted_at IS NOT NULL", ids)
    }

    fn set_deleted_at(
        &mut self,
        deleted_at: Option<i64>,
        filter: &str,
        ids: &[String],
    ) -> Result<Vec<String>> {
        let tx = self.conn.transaction()?;
        let changed = {
            let mut statement = tx.prepare(&format!(
                "UPDATE stickers SET deleted_at = ?2 WHERE {filter} AND id IN (SELECT value FROM json_each(?1)) RETURNING id"
            ))?;
            statement
                .query_map(params![serde_json::to_string(ids)?, deleted_at], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.commit()?;
        Ok(changed)
    }

    pub fn purge_sticker(&mut self, id: &str) -> Result<()> {
        if self.purge_matching("id = ?1 AND deleted_at IS NOT NULL", [id])? == 0 {
            return Err(Error::StickerNotDeleted);
        }
        Ok(())
    }

    /// 只处理确认时的列表。期间恢复的表情、新进入最近删除的表情都不受影响。
    pub fn purge_deleted_stickers(&mut self, ids: &[String]) -> Result<usize> {
        self.purge_matching(
            "deleted_at IS NOT NULL AND id IN (SELECT value FROM json_each(?1))",
            [serde_json::to_string(ids)?],
        )
    }

    pub fn purge_expired(&mut self) -> Result<usize> {
        self.purge_expired_at(unix_millis()?)
    }

    fn purge_expired_at(&mut self, now: i64) -> Result<usize> {
        self.purge_matching("deleted_at <= ?1", [now - RETENTION_MS])
    }

    /// 清理没有表情记录的原图和缩略图（彻底删除时没删掉的），以及中断留下的临时文件。
    /// 只认按哈希命名的文件和 `.tmp` 开头的临时文件，其他文件不动。
    pub fn sweep_orphan_files(&self) -> Result<usize> {
        self.sweep_orphan_files_at(SystemTime::now())
    }

    fn sweep_orphan_files_at(&self, now: SystemTime) -> Result<usize> {
        // Library -> files，与导入提交、同步下载的发布互斥。
        let files = self.files.clone();
        let _files = files.lock().unwrap();
        let known = self
            .conn
            .prepare("SELECT hash FROM stickers")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        let mut removed = 0;
        for directory in ["objects", "thumbs"] {
            for entry in fs::read_dir(self.root.join(directory))? {
                let entry = entry?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let orphan = name.starts_with(".tmp")
                    || name
                        .split_once('.')
                        .is_some_and(|(hash, _)| is_hash(hash) && !known.contains(hash));
                let stale = || {
                    entry.metadata().is_ok_and(|metadata| {
                        metadata.is_file()
                            && metadata.modified().is_ok_and(|modified| {
                                now.duration_since(modified).unwrap_or_default() >= ORPHAN_GRACE
                            })
                    })
                };
                // 删不掉的（比如被其他程序占用）留到下次。
                if orphan && stale() && fs::remove_file(entry.path()).is_ok() {
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }

    fn purge_matching(&mut self, filter: &str, parameters: impl Params) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let files = {
            let mut statement = tx.prepare(&format!(
                "DELETE FROM stickers WHERE {filter} RETURNING hash, ext"
            ))?;
            statement
                .query_map(parameters, |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        // 先提交元数据和级联删除，再清理文件；提交失败时原文件必须还在。
        tx.commit()?;
        for (hash, ext) in &files {
            // 文件清理失败不撤销彻底删除，再导入时会沿用留下的文件。
            let _ = fs::remove_file(self.root.join("objects").join(format!("{hash}.{ext}")));
            let _ = fs::remove_file(self.root.join("thumbs").join(format!("{hash}.png")));
        }
        Ok(files.len())
    }
}

fn is_hash(name: &str) -> bool {
    name.len() == 64 && name.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests;
