use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Sticker,
    Category,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stamp {
    pub at: i64,
    pub device: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Register {
    pub value: Value,
    #[serde(flatten)]
    pub stamp: Stamp,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Object {
    pub hash: String,
    pub ext: String,
    pub animated: i64,
    pub width: u32,
    pub height: u32,
    pub byte_size: i64,
    pub added_at: i64,
}
impl Object {
    pub fn key(&self) -> String {
        format!("objects/{}.{}", self.hash, self.ext)
    }
    pub fn validate(&self) -> bool {
        self.hash.len() == 64
            && self
                .hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && matches!(self.ext.as_str(), "png" | "jpg" | "gif" | "webp")
            && self.width > 0
            && self.height > 0
            && self.byte_size >= 0
            && matches!(self.animated, 0 | 1)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Create {
        entity: Kind,
        id: String,
        data: Option<Object>,
        fields: BTreeMap<String, Value>,
    },
    Set {
        entity: Kind,
        id: String,
        field: String,
        value: Value,
    },
    /// 只删除列出的 id。本机删除时列出已知的同一身份的全部 id（见 ADR 0005）。
    Purge {
        entity: Kind,
        ids: Vec<String>,
    },
    Delete {
        entity: Kind,
        ids: Vec<String>,
    },
    Usage {
        sticker: String,
        owner: String,
        count: i64,
        last_used_at: i64,
    },
    Merge {
        entity: Kind,
        from: String,
        into: String,
    },
    Snapshot {
        changes: Vec<Change>,
    },
    #[serde(other)]
    Unknown,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Change {
    #[serde(flatten)]
    pub stamp: Stamp,
    #[serde(flatten)]
    pub action: Action,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Operation {
    pub seq: i64,
    #[serde(flatten)]
    pub change: Change,
}
impl Change {
    pub fn known(&self) -> bool {
        match &self.action {
            Action::Unknown => false,
            Action::Snapshot { changes } => changes
                .iter()
                .all(|c| !matches!(c.action, Action::Snapshot { .. }) && c.known()),
            Action::Set { entity, field, .. } => fields(*entity).contains(&field.as_str()),
            _ => true,
        }
    }
    pub fn max_at(&self) -> i64 {
        match &self.action {
            Action::Snapshot { changes } => changes
                .iter()
                .map(Change::max_at)
                .max()
                .unwrap_or(0)
                .max(self.stamp.at),
            _ => self.stamp.at,
        }
    }
    pub fn objects(&self, result: &mut BTreeMap<String, Object>) {
        match &self.action {
            Action::Create {
                data: Some(object), ..
            } => {
                result.insert(object.key(), object.clone());
            }
            Action::Snapshot { changes } => {
                for change in changes {
                    change.objects(result);
                }
            }
            _ => {}
        }
    }
}
pub fn fields(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Sticker => &["name", "caption", "favorite", "deleted_at", "category_id"],
        Kind::Category => &["name", "color", "position"],
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Entity {
    pub data: Option<Object>,
    pub created: Stamp,
    pub fields: BTreeMap<String, Register>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Usage {
    pub count: i64,
    pub last_used_at: i64,
    pub stamp: Stamp,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Model {
    pub entities: BTreeMap<Kind, BTreeMap<String, Entity>>,
    pub aliases: BTreeMap<Kind, BTreeMap<String, String>>,
    pub tombstones: BTreeMap<Kind, BTreeSet<String>>,
    pub usages: BTreeMap<String, BTreeMap<String, Usage>>,
}
impl Model {
    pub fn resolve(&self, kind: Kind, id: &str) -> String {
        let mut id = id;
        if let Some(aliases) = self.aliases.get(&kind) {
            while let Some(next) = aliases.get(id) {
                id = next;
            }
        }
        id.to_owned()
    }
    /// 只看这个 id 自己：合并身份里其他 id 被彻底删除，不影响它。
    pub fn dead(&self, kind: Kind, id: &str) -> bool {
        self.tombstones
            .get(&kind)
            .is_some_and(|ids| ids.contains(id))
    }
    /// 合并身份里任何一个 id 有记录即可。
    fn exists(&self, kind: Kind, id: &str) -> bool {
        let Some(entities) = self.entities.get(&kind) else {
            return false;
        };
        if entities.contains_key(id) {
            return true;
        }
        let root = self.resolve(kind, id);
        entities.contains_key(&root)
            || self.aliases.get(&kind).is_some_and(|aliases| {
                aliases
                    .keys()
                    .any(|alias| entities.contains_key(alias) && self.resolve(kind, alias) == root)
            })
    }
    /// 给出的 id 加上本机已知与它们合并的全部 id。
    pub fn members(&self, kind: Kind, ids: &[String]) -> Vec<String> {
        let roots = ids
            .iter()
            .map(|id| self.resolve(kind, id))
            .collect::<BTreeSet<_>>();
        let mut result = ids.iter().cloned().collect::<BTreeSet<_>>();
        let known = self
            .entities
            .get(&kind)
            .into_iter()
            .flat_map(|entities| entities.keys())
            .chain(
                self.aliases
                    .get(&kind)
                    .into_iter()
                    .flat_map(|aliases| aliases.keys().chain(aliases.values())),
            );
        for id in known {
            if roots.contains(&self.resolve(kind, id)) {
                result.insert(id.clone());
            }
        }
        result.into_iter().collect()
    }
    /// 合并身份的根 id -> 其中还活着的最小 id，用作本地行的 id。整个身份都已删除时没有条目。
    pub fn representatives(&self, kind: Kind) -> BTreeMap<String, String> {
        let mut result = BTreeMap::new();
        for id in self.entities.get(&kind).into_iter().flat_map(|e| e.keys()) {
            if !self.dead(kind, id) {
                result
                    .entry(self.resolve(kind, id))
                    .or_insert_with(|| id.clone());
            }
        }
        result
    }
    /// 已删除的 id 自己的记录（关系、使用）不再算进合并身份。
    pub fn representative(
        &self,
        representatives: &BTreeMap<String, String>,
        kind: Kind,
        id: &str,
    ) -> Option<String> {
        if self.dead(kind, id) {
            return None;
        }
        representatives.get(&self.resolve(kind, id)).cloned()
    }
    /// false 表示依赖没到，调用者持久化到待应用表；删除标记不依赖实体先到。
    pub fn apply(&mut self, change: &Change, available: &BTreeSet<String>) -> Result<bool> {
        let stamp = &change.stamp;
        match &change.action {
            Action::Create {
                entity,
                id,
                data,
                fields: values,
            } => {
                if self.dead(*entity, id) {
                    return Ok(true);
                }
                if let Some(object) = data {
                    if !object.validate() {
                        return Err(Error::Sync("原图元数据不合法".into()));
                    }
                    if !available.contains(&object.key()) {
                        return Ok(false);
                    }
                } else if *entity == Kind::Sticker {
                    return Err(Error::Sync("导入缺少原图信息".into()));
                }
                if !self.category_ready(*entity, values.get("category_id")) {
                    return Ok(false);
                }
                let entity = self
                    .entities
                    .entry(*entity)
                    .or_default()
                    .entry(id.clone())
                    .or_insert_with(|| Entity {
                        data: data.clone(),
                        created: stamp.clone(),
                        fields: BTreeMap::new(),
                    });
                if entity.data.is_none() {
                    entity.data = data.clone();
                }
                for (field, value) in values {
                    set_register(&mut entity.fields, field.clone(), value.clone(), stamp);
                }
            }
            Action::Set {
                entity,
                id,
                field,
                value,
            } => {
                if self.dead(*entity, id) {
                    return Ok(true);
                }
                if !self.exists(*entity, id)
                    || (field == "category_id" && !self.category_ready(*entity, Some(value)))
                {
                    return Ok(false);
                }
                // 保留原 id 的寄存器；投影时按合并后的 id 逐字段归并，迟到操作不丢失。
                let target = self
                    .entities
                    .entry(*entity)
                    .or_default()
                    .entry(id.clone())
                    .or_insert_with(|| Entity {
                        data: None,
                        created: stamp.clone(),
                        fields: BTreeMap::new(),
                    });
                set_register(&mut target.fields, field.clone(), value.clone(), stamp);
            }
            Action::Purge { entity, ids } | Action::Delete { entity, ids } => {
                self.tombstones
                    .entry(*entity)
                    .or_default()
                    .extend(ids.iter().cloned());
            }
            Action::Merge { entity, from, into } => {
                let a = self.resolve(*entity, from);
                let b = self.resolve(*entity, into);
                if a != b {
                    let (from, into) = if a > b { (a, b) } else { (b, a) };
                    self.aliases.entry(*entity).or_default().insert(from, into);
                }
            }
            Action::Usage {
                sticker,
                owner,
                count,
                last_used_at,
            } => {
                if self.dead(Kind::Sticker, sticker) {
                    return Ok(true);
                }
                if !self.exists(Kind::Sticker, sticker) {
                    return Ok(false);
                }
                if *count < 0 {
                    return Err(Error::Sync("使用次数不能为负数".into()));
                }
                let usages = self.usages.entry(sticker.clone()).or_default();
                if usages.get(owner).is_none_or(|u| stamp > &u.stamp) {
                    usages.insert(
                        owner.clone(),
                        Usage {
                            count: *count,
                            last_used_at: *last_used_at,
                            stamp: stamp.clone(),
                        },
                    );
                }
            }
            // 快照在应用边界拆开，使每个缺失依赖可以独立重试。
            Action::Snapshot { .. } | Action::Unknown => {
                return Err(Error::SyncUpgrade("不认识的操作类型".into()));
            }
        }
        Ok(true)
    }
    /// 表情所属的分类还没到时，先放进待应用表；分类已被删除则按未分类处理。
    fn category_ready(&self, entity: Kind, value: Option<&Value>) -> bool {
        if entity != Kind::Sticker {
            return true;
        }
        value
            .and_then(Value::as_str)
            .is_none_or(|id| self.dead(Kind::Category, id) || self.exists(Kind::Category, id))
    }
    /// 按代表 id 汇总合并身份里还活着的 id 的字段。
    pub fn projected(&self, kind: Kind) -> BTreeMap<String, Entity> {
        let representatives = self.representatives(kind);
        let mut result: BTreeMap<String, Entity> = BTreeMap::new();
        if let Some(entities) = self.entities.get(&kind) {
            for (id, entity) in entities {
                let Some(representative) = self.representative(&representatives, kind, id) else {
                    continue;
                };
                let entry = result
                    .entry(representative)
                    .or_insert_with(|| entity.clone());
                if entry.data.is_none() {
                    entry.data = entity.data.clone();
                }
                for (field, register) in &entity.fields {
                    set_register(
                        &mut entry.fields,
                        field.clone(),
                        register.value.clone(),
                        &register.stamp,
                    );
                }
            }
        }
        result
    }
    pub fn duplicate(&self) -> Option<Action> {
        for kind in [Kind::Sticker, Kind::Category] {
            let mut keys = BTreeMap::new();
            for (id, entity) in self.projected(kind) {
                let key = if kind == Kind::Sticker {
                    entity.data.as_ref().map(|d| d.hash.clone())
                } else {
                    // SQLite NOCASE 只折叠 ASCII。
                    entity
                        .fields
                        .get("name")
                        .and_then(|r| r.value.as_str())
                        .map(str::to_ascii_lowercase)
                };
                // 原图或名称还没到的身份先不参与去重，不能因此跳过其他身份。
                let Some(key) = key else { continue };
                if let Some(into) = keys.get(&key) {
                    return Some(Action::Merge {
                        entity: kind,
                        from: id,
                        into: String::clone(into),
                    });
                }
                keys.insert(key, id);
            }
        }
        None
    }

    pub fn snapshot(&self) -> Vec<Change> {
        let mut result = Vec::new();
        // 先应用永久删除和映射，避免转移仓库后迟到的旧内容被暂时复活。
        for (kind, ids) in &self.tombstones {
            result.push(Change {
                stamp: Stamp::default(),
                action: Action::Delete {
                    entity: *kind,
                    ids: ids.iter().cloned().collect(),
                },
            });
        }
        for (kind, aliases) in &self.aliases {
            for (from, into) in aliases {
                result.push(Change {
                    stamp: Stamp::default(),
                    action: Action::Merge {
                        entity: *kind,
                        from: from.clone(),
                        into: into.clone(),
                    },
                });
            }
        }
        for kind in [Kind::Category, Kind::Sticker] {
            if let Some(entities) = self.entities.get(&kind) {
                for (id, entity) in entities {
                    if self.dead(kind, id) {
                        continue;
                    }
                    // 基础记录和各字段独立携带原时钟。
                    if kind != Kind::Sticker || entity.data.is_some() {
                        result.push(Change {
                            stamp: entity.created.clone(),
                            action: Action::Create {
                                entity: kind,
                                id: id.clone(),
                                data: entity.data.clone(),
                                fields: BTreeMap::new(),
                            },
                        });
                    }
                    for (field, register) in &entity.fields {
                        result.push(Change {
                            stamp: register.stamp.clone(),
                            action: Action::Set {
                                entity: kind,
                                id: id.clone(),
                                field: field.clone(),
                                value: register.value.clone(),
                            },
                        });
                    }
                }
            }
        }
        for (sticker, usages) in &self.usages {
            for (owner, value) in usages {
                result.push(Change {
                    stamp: value.stamp.clone(),
                    action: Action::Usage {
                        sticker: sticker.clone(),
                        owner: owner.clone(),
                        count: value.count,
                        last_used_at: value.last_used_at,
                    },
                });
            }
        }
        result
    }
}
fn set_register(map: &mut BTreeMap<String, Register>, key: String, value: Value, stamp: &Stamp) {
    if map.get(&key).is_none_or(|r| stamp > &r.stamp) {
        map.insert(
            key,
            Register {
                value,
                stamp: stamp.clone(),
            },
        );
    }
}
