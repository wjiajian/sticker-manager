use super::model::{
    Action, Change, Entity, Kind, Model, Object, Operation, Register, Stamp, Usage,
};
use crate::{Library, Result, time::unix_millis};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn load(conn: &Connection) -> Result<(i64, Model)> {
    let (revision, json): (i64, String) = conn.query_row(
        "SELECT revision,state FROM sync_model WHERE singleton=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok((revision, serde_json::from_str(&json)?))
}
pub(super) fn save(conn: &Connection, revision: i64, model: &Model) -> Result<()> {
    conn.execute("INSERT INTO sync_model VALUES (1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision=excluded.revision,state=excluded.state",params![revision,serde_json::to_string(model)?])?;
    Ok(())
}
pub(super) fn append(conn: &Connection, device: &str, action: Action) -> Result<Change> {
    conn.execute(
        "UPDATE sync_meta SET max_at=MAX(?1,max_at+1)",
        [unix_millis()?],
    )?;
    let at = conn.query_row("SELECT max_at FROM sync_meta", [], |r| r.get(0))?;
    conn.execute(
        "INSERT INTO sync_outbox(at,device,action) VALUES (?1,?2,?3)",
        params![at, device, serde_json::to_string(&action)?],
    )?;
    Ok(Change {
        stamp: Stamp {
            at,
            device: device.into(),
        },
        action,
    })
}
pub(super) fn local_changes(conn: &Connection, since: i64) -> Result<Vec<(i64, Change)>> {
    let mut query = conn.prepare(
        "SELECT revision,at,device,action FROM sync_outbox WHERE revision>?1 ORDER BY revision",
    )?;
    let rows = query
        .query_map([since], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(seq, at, device, action)| {
            Ok((
                seq,
                Change {
                    stamp: Stamp { at, device },
                    action: serde_json::from_str(&action)?,
                },
            ))
        })
        .collect()
}
pub(super) fn flush(conn: &Connection, model: &mut Model, revision: &mut i64) -> Result<()> {
    for (seq, mut change) in local_changes(conn, *revision)? {
        // 触发器只知道被删的那一行；补上本机已知与它合并的 id 并写回，推送时原样发出，
        // 本机和其他设备删除的是同一组 id。
        if let Action::Purge { entity, ids } | Action::Delete { entity, ids } = &mut change.action {
            let members = model.members(*entity, ids);
            if members != *ids {
                *ids = members;
                conn.execute(
                    "UPDATE sync_outbox SET action=?2 WHERE revision=?1",
                    params![seq, serde_json::to_string(&change.action)?],
                )?;
            }
        }
        let mut objects = BTreeMap::new();
        change.objects(&mut objects);
        let available = objects.into_keys().collect();
        if !model.apply(&change, &available)? {
            pending(
                conn,
                "local",
                &change.stamp.device,
                seq,
                vec![change.clone()],
            )?;
        }
        *revision = seq;
    }
    Ok(())
}
pub(super) fn pending(
    conn: &Connection,
    library: &str,
    device: &str,
    seq: i64,
    changes: Vec<Change>,
) -> Result<()> {
    if changes.is_empty() {
        conn.execute(
            "DELETE FROM sync_pending WHERE library_id=?1 AND device=?2 AND seq=?3",
            params![library, device, seq],
        )?;
    } else {
        conn.execute("INSERT INTO sync_pending VALUES (?1,?2,?3,?4) ON CONFLICT(library_id,device,seq) DO UPDATE SET change=excluded.change",params![library,device,seq,serde_json::to_string(&changes)?])?;
    }
    Ok(())
}
type PendingRow = (String, String, i64, Vec<Change>);

pub(super) fn pending_rows(conn: &Connection) -> Result<Vec<PendingRow>> {
    let mut stmt = conn.prepare(
        "SELECT library_id,device,seq,change FROM sync_pending ORDER BY library_id,device,seq",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(l, d, s, c)| Ok((l, d, s, serde_json::from_str(&c)?)))
        .collect()
}
pub(super) fn retry(
    conn: &Connection,
    model: &mut Model,
    available: &BTreeSet<String>,
) -> Result<()> {
    loop {
        let mut progressed = false;
        for (library, device, seq, changes) in pending_rows(conn)? {
            let mut waiting = Vec::new();
            for change in changes {
                if model.apply(&change, available)? {
                    if library != "local" {
                        super::presence::record_origin(conn, &library, &device, seq, &change)?;
                    }
                    progressed = true;
                } else {
                    waiting.push(change);
                }
            }
            pending(conn, &library, &device, seq, waiting)?;
        }
        if !progressed {
            break;
        }
    }
    Ok(())
}
pub(super) fn deduplicate(conn: &Connection, model: &mut Model, device: &str) -> Result<()> {
    while let Some(action) = model.duplicate() {
        let change = append(conn, device, action)?;
        model.apply(&change, &BTreeSet::new())?;
    }
    Ok(())
}
pub(super) fn flatten(change: &Change) -> Vec<Change> {
    match &change.action {
        Action::Snapshot { changes } => changes.clone(),
        _ => vec![change.clone()],
    }
}
pub(super) fn current_revision(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE((SELECT seq FROM sqlite_sequence WHERE name='sync_outbox'),0)",
        [],
        |r| r.get(0),
    )?)
}

/// 只有当前目标还会读取 outbox 增量；切回旧目标必须 join_sync，追加快照。
pub(super) fn prune(conn: &Connection, active: Option<&str>) -> Result<()> {
    conn.execute(
        "DELETE FROM sync_outbox WHERE revision <= MIN(
            (SELECT revision FROM sync_model WHERE singleton=1),
            COALESCE((SELECT revision FROM sync_targets WHERE library_id=?1),
                     (SELECT revision FROM sync_model WHERE singleton=1)))",
        [active],
    )?;
    conn.execute("DELETE FROM sync_stream WHERE seq <= (SELECT pushed_seq FROM sync_targets WHERE library_id=sync_stream.library_id)", [])?;
    Ok(())
}

impl Library {
    /// 未配置同步时也折叠本地操作。调用者须传当前目标，不能传已切走的历史目标。
    pub fn maintain_sync(&mut self, active: Option<&str>) -> Result<()> {
        let tx = self.conn.transaction()?;
        let folded: i64 = tx.query_row(
            "SELECT revision FROM sync_model WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        if folded < current_revision(&tx)? {
            let (mut revision, mut model) = load(&tx)?;
            flush(&tx, &mut model, &mut revision)?;
            save(&tx, revision, &model)?;
        }
        prune(&tx, active)?;
        tx.commit()?;
        Ok(())
    }

    /// 仅在启动、窗口出现前回收明显的空闲空间；运行中的清理只删行并复用空页。
    pub fn compact_database_at_startup(&mut self) -> Result<()> {
        let free: i64 = self
            .conn
            .pragma_query_value(None, "freelist_count", |r| r.get(0))?;
        let pages: i64 = self
            .conn
            .pragma_query_value(None, "page_count", |r| r.get(0))?;
        if free >= 256 && free * 4 >= pages {
            self.conn.execute_batch(
                "PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA wal_checkpoint(TRUNCATE);",
            )?;
        }
        Ok(())
    }
    pub(crate) fn initialize_sync(&self) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let exists: bool =
            tx.query_row("SELECT EXISTS(SELECT 1 FROM sync_model)", [], |r| r.get(0))?;
        if !exists {
            let mut model = Model::default();
            let stamp = Stamp {
                at: 0,
                device: self.device.id.clone(),
            };
            for (kind, table, columns) in [
                (
                    Kind::Sticker,
                    "stickers",
                    "id,hash,ext,animated,width,height,byte_size,added_at,name,caption,favorite,deleted_at",
                ),
                (Kind::Tag, "tags", "id,name,color,group_id,position"),
                (Kind::Group, "tag_groups", "id,name,position"),
            ] {
                let mut stmt = tx.prepare(&format!("SELECT {columns} FROM {table}"))?;
                let mut rows = stmt.query([])?;
                while let Some(row) = rows.next()? {
                    let id: String = row.get(0)?;
                    let (data, values) = match kind {
                        Kind::Sticker => (
                            Some(Object {
                                hash: row.get(1)?,
                                ext: row.get(2)?,
                                animated: row.get(3)?,
                                width: row.get(4)?,
                                height: row.get(5)?,
                                byte_size: row.get(6)?,
                                added_at: row.get(7)?,
                            }),
                            json!({"name":row.get::<_,String>(8)?,"caption":row.get::<_,Option<String>>(9)?,"favorite":row.get::<_,i64>(10)?,"deleted_at":row.get::<_,Option<i64>>(11)?}),
                        ),
                        Kind::Tag => (
                            None,
                            json!({"name":row.get::<_,String>(1)?,"color":row.get::<_,String>(2)?,"group_id":row.get::<_,Option<String>>(3)?,"position":row.get::<_,i64>(4)?}),
                        ),
                        Kind::Group => (
                            None,
                            json!({"name":row.get::<_,String>(1)?,"position":row.get::<_,i64>(2)?}),
                        ),
                    };
                    let fields = values
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                Register {
                                    value: v.clone(),
                                    stamp: stamp.clone(),
                                },
                            )
                        })
                        .collect();
                    model.entities.entry(kind).or_default().insert(
                        id,
                        Entity {
                            data,
                            created: stamp.clone(),
                            fields,
                        },
                    );
                }
            }
            let mut stmt = tx.prepare("SELECT sticker_id,tag_id FROM sticker_tags")?;
            for row in
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            {
                let (s, t) = row?;
                model.links.entry(s).or_default().insert(
                    t,
                    Register {
                        value: json!(1),
                        stamp: stamp.clone(),
                    },
                );
            }
            let mut stmt =
                tx.prepare("SELECT sticker_id,device_id,count,last_used_at FROM usages")?;
            for row in stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })? {
                let (s, d, count, last_used_at) = row?;
                model.usages.entry(s).or_default().insert(
                    d.clone(),
                    Usage {
                        count,
                        last_used_at,
                        stamp: Stamp { at: 0, device: d },
                    },
                );
            }
            save(&tx, 0, &model)?;
        }
        tx.execute(
            "UPDATE sync_meta SET device=?1,recording=1",
            [&self.device.id],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn local_revision(&self) -> Result<i64> {
        current_revision(&self.conn)
    }
    pub fn rename_device(&mut self, name: &str) -> Result<()> {
        self.device.rename(&self.root, name)
    }
    pub fn storage_bytes(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COALESCE(SUM(byte_size),0) FROM stickers", [], |r| {
                r.get(0)
            })?)
    }

    /// 加入/切换仓库时先固化快照。seq 在一个 library id 内按设备递增，重连同一仓库不复用序号。
    pub fn join_sync(&mut self, library_id: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        let (mut revision, mut model) = load(&tx)?;
        flush(&tx, &mut model, &mut revision)?;
        let mut changes = model.snapshot();
        for (_, _, _, waiting) in pending_rows(&tx)? {
            changes.extend(waiting);
        }
        let change = Change {
            stamp: Stamp {
                at: unix_millis()?,
                device: self.device.id.clone(),
            },
            action: Action::Snapshot { changes },
        };
        tx.execute("INSERT INTO sync_targets(library_id,revision,initial_pull_complete) VALUES(?1,?2,0) ON CONFLICT(library_id) DO UPDATE SET revision=excluded.revision",params![library_id,revision])?;
        enqueue(&tx, library_id, change)?;
        save(&tx, revision, &model)?;
        prune(&tx, Some(library_id))?;
        tx.commit()?;
        Ok(())
    }
}
pub(super) fn enqueue(conn: &Connection, library: &str, change: Change) -> Result<()> {
    let seq = conn.query_row(
        "UPDATE sync_targets SET next_seq=next_seq+1 WHERE library_id=?1 RETURNING next_seq",
        [library],
        |r| r.get(0),
    )?;
    let operation = Operation { seq, change };
    super::presence::record_origin(
        conn,
        library,
        &operation.change.stamp.device,
        seq,
        &operation.change,
    )?;
    conn.execute(
        "INSERT INTO sync_stream VALUES(?1,?2,?3)",
        params![library, seq, serde_json::to_string(&operation)?],
    )?;
    Ok(())
}

pub(super) fn project(conn: &Connection, model: &Model) -> Result<()> {
    conn.execute("UPDATE sync_meta SET recording=0", [])?;
    conn.execute_batch("DELETE FROM sticker_tags; DELETE FROM usages; DELETE FROM tags; DELETE FROM tag_groups; DELETE FROM stickers;")?;
    let groups = model.projected(Kind::Group);
    let tags = model.projected(Kind::Tag);
    let stickers = model.projected(Kind::Sticker);
    let group_ids = model.representatives(Kind::Group);
    let tag_ids = model.representatives(Kind::Tag);
    let sticker_ids = model.representatives(Kind::Sticker);
    for (id, e) in &groups {
        conn.execute(
            "INSERT INTO tag_groups VALUES(?1,?2,?3)",
            params![
                id,
                value(e, "name").as_str().unwrap_or(""),
                value(e, "position").as_i64().unwrap_or(0)
            ],
        )?;
    }
    for (id, e) in &tags {
        let group = value(e, "group_id")
            .as_str()
            .and_then(|g| model.representative(&group_ids, Kind::Group, g))
            .filter(|g| groups.contains_key(g));
        conn.execute(
            "INSERT INTO tags VALUES(?1,?2,?3,?4,?5)",
            params![
                id,
                value(e, "name").as_str().unwrap_or(""),
                value(e, "color").as_str().unwrap_or("#8A8A8A"),
                group,
                value(e, "position").as_i64().unwrap_or(0)
            ],
        )?;
    }
    for (id, e) in &stickers {
        let Some(o) = &e.data else { continue };
        conn.execute("INSERT INTO stickers(id,hash,ext,name,caption,favorite,animated,width,height,byte_size,added_at,deleted_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![id,o.hash,o.ext,value(e,"name").as_str().unwrap_or(""),value(e,"caption").as_str(),value(e,"favorite").as_i64().unwrap_or(0),o.animated,o.width,o.height,o.byte_size,o.added_at,value(e,"deleted_at").as_i64()])?;
    }
    let mut links: BTreeMap<(String, String), Register> = BTreeMap::new();
    for (s, ts) in &model.links {
        for (t, v) in ts {
            let (Some(s), Some(t)) = (
                model.representative(&sticker_ids, Kind::Sticker, s),
                model.representative(&tag_ids, Kind::Tag, t),
            ) else {
                continue;
            };
            if !stickers.contains_key(&s) || !tags.contains_key(&t) {
                continue;
            }
            let key = (s, t);
            if links.get(&key).is_none_or(|old| v.stamp > old.stamp) {
                links.insert(key, v.clone());
            }
        }
    }
    for ((s, t), v) in links {
        if v.value.as_i64() == Some(1) {
            conn.execute("INSERT INTO sticker_tags VALUES(?1,?2)", params![s, t])?;
        }
    }
    let mut usages: BTreeMap<(String, String), (i64, i64, i64)> = BTreeMap::new();
    for (original, devices) in &model.usages {
        for (device, u) in devices {
            let Some(s) = model.representative(&sticker_ids, Kind::Sticker, original) else {
                continue;
            };
            if !stickers.contains_key(&s) {
                continue;
            }
            let v = usages.entry((s.clone(), device.clone())).or_default();
            v.0 += u.count;
            v.1 = v.1.max(u.last_used_at);
            if s != *original {
                v.2 += u.count;
            }
        }
    }
    for ((s, d), (count, last, offset)) in usages {
        conn.execute(
            "INSERT INTO usages VALUES(?1,?2,?3,?4)",
            params![s, d, count, last],
        )?;
        conn.execute(
            "INSERT INTO sync_usage_offsets VALUES(?1,?2,?3)",
            params![s, d, offset],
        )?;
    }
    conn.execute("UPDATE sync_meta SET recording=1", [])?;
    Ok(())
}
fn value<'a>(entity: &'a Entity, field: &str) -> &'a Value {
    entity
        .fields
        .get(field)
        .map(|v| &v.value)
        .unwrap_or(&Value::Null)
}

pub(super) fn cursor(conn: &Connection, library: &str, device: &str) -> Result<i64> {
    Ok(conn
        .query_row(
            "SELECT seq FROM sync_progress WHERE library_id=?1 AND device=?2",
            params![library, device],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0))
}
