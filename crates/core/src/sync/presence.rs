use super::*;
use model::Change;
use rusqlite::Connection;

pub(super) fn record_origin(
    conn: &Connection,
    library: &str,
    device: &str,
    seq: i64,
    change: &Change,
) -> Result<()> {
    for change in store::flatten(change) {
        if let Action::Create {
            entity: Kind::Sticker,
            id,
            ..
        } = change.action
        {
            conn.execute(
                "INSERT OR IGNORE INTO sync_origins VALUES(?1,?2,?3,?4)",
                params![library, id, device, seq],
            )?;
        }
    }
    Ok(())
}

pub(super) fn received(conn: &Connection, library: &str) -> Result<BTreeMap<String, i64>> {
    let mut stmt = conn.prepare("SELECT p.device, MIN(p.seq, COALESCE((SELECT MIN(w.seq)-1 FROM sync_pending w WHERE w.library_id=p.library_id AND w.device=p.device),p.seq)) FROM sync_progress p WHERE p.library_id=?1")?;
    Ok(stmt
        .query_map([library], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

pub fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        ""
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StickerSyncInfo {
    pub waiting_upload: bool,
    /// 旧表情缺少来源记录时为 None，界面只显示“已同步”。
    pub device_count: Option<usize>,
}

impl Library {
    pub fn cached_sync_devices(&self, library: &str) -> Result<(Vec<RemoteDevice>, Option<i64>)> {
        let row: Option<(String, i64)> = self
            .conn
            .query_row(
                "SELECT devices,last_sync_at FROM sync_device_cache WHERE library_id=?1",
                [library],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            Some((json, time)) => Ok((serde_json::from_str(&json)?, Some(time))),
            None => Ok((Vec::new(), None)),
        }
    }

    pub fn sticker_sync_info(
        &self,
        id: &str,
        library: &str,
        devices: &[RemoteDevice],
    ) -> Result<StickerSyncInfo> {
        self.sticker(id)?;
        let (_, model) = store::load(&self.conn)?;
        let members = model.members(Kind::Sticker, &[id.into()]);
        let mut origins = Vec::<(String, i64)>::new();
        for member in &members {
            if model.dead(Kind::Sticker, member) {
                continue;
            }
            let mut stmt = self.conn.prepare(
                "SELECT device,seq FROM sync_origins WHERE library_id=?1 AND sticker_id=?2",
            )?;
            origins.extend(
                stmt.query_map(params![library, member], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?,
            );
        }
        let (revision, pushed): (i64, i64) = self
            .conn
            .query_row(
                "SELECT revision,pushed_seq FROM sync_targets WHERE library_id=?1",
                [library],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .unwrap_or((0, 0));
        let local_unassigned: bool = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM sync_outbox WHERE revision>?1 AND json_extract(action,'$.type')='create' AND json_extract(action,'$.entity')='sticker' AND json_extract(action,'$.id')=?2)", params![revision,id], |r|r.get(0))?;
        let published = origins
            .iter()
            .filter(|(d, s)| d != &self.device.id || *s <= pushed)
            .collect::<Vec<_>>();
        if published.is_empty() {
            return Ok(StickerSyncInfo {
                waiting_upload: local_unassigned || !origins.is_empty(),
                device_count: None,
            });
        }
        let mut holders = BTreeSet::from([self.device.id.clone()]);
        for (source, seq) in published {
            holders.insert(source.clone());
            for device in devices {
                if device
                    .received
                    .get(source)
                    .is_some_and(|received| received >= seq)
                {
                    holders.insert(device.id.clone());
                }
            }
        }
        Ok(StickerSyncInfo {
            waiting_upload: false,
            device_count: Some(holders.len()),
        })
    }
}

/// 比较原身份的哈希（含最近删除），避免恢复、同图合并及本机并发导入计入新到达。
pub(super) fn known_hashes(model: &Model) -> BTreeSet<String> {
    model
        .entities
        .get(&Kind::Sticker)
        .into_iter()
        .flat_map(|e| e.values())
        .filter_map(|e| e.data.as_ref().map(|o| o.hash.clone()))
        .collect()
}
pub(super) fn new_stickers(before: &BTreeSet<String>, model: &Model) -> usize {
    model
        .projected(Kind::Sticker)
        .values()
        .filter(|e| e.fields.get("deleted_at").is_none_or(|v| v.value.is_null()))
        .filter_map(|e| e.data.as_ref().map(|o| &o.hash))
        .filter(|hash| !before.contains(*hash))
        .collect::<BTreeSet<_>>()
        .len()
}
