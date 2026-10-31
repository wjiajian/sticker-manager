use std::fs;

use rusqlite::{Params, params};

use crate::{Error, Library, Result, time::unix_millis};

pub(crate) const RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

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

    pub fn purge_sticker(&mut self, id: &str) -> Result<()> {
        if self.purge_matching("id = ?1 AND deleted_at IS NOT NULL", [id])? == 0 {
            return Err(Error::StickerNotDeleted);
        }
        Ok(())
    }

    pub fn purge_expired(&mut self) -> Result<usize> {
        self.purge_expired_at(unix_millis()?)
    }

    fn purge_expired_at(&mut self, now: i64) -> Result<usize> {
        self.purge_matching("deleted_at <= ?1", [now - RETENTION_MS])
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

#[cfg(test)]
mod tests;
