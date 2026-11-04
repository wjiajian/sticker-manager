//! 只重组同一设备的 format-2 原始字节，不丢历史，不依赖本地 stream。
use super::{Segment, decode_segment, storage::Storage};
use crate::{Error, Result};
use std::collections::BTreeMap;

const LIMIT: u64 = 4 * 1024 * 1024;
const MIN_SOURCES: usize = 32;
const MAX_SOURCES: usize = 128;

/// 读不到或大小与列举不符都当作还没送达，留到下轮。
fn read_listed(storage: &dyn Storage, segment: &Segment) -> Result<Vec<u8>> {
    let bytes = storage
        .read(&segment.key)?
        .ok_or_else(|| Error::Sync("合并源段尚未送达".into()))?;
    if bytes.len() as u64 != segment.size {
        return Err(Error::Sync(format!(
            "日志段与列举大小不符：{}",
            segment.key
        )));
    }
    Ok(bytes)
}

fn read_checked(storage: &dyn Storage, source: &Segment) -> Result<Vec<u8>> {
    let bytes = read_listed(storage, source)?;
    if !decode_segment(&bytes, source).is_some_and(|ops| ops.iter().all(|op| op.change.known())) {
        return Err(Error::Sync(format!("合并源段校验失败：{}", source.key)));
    }
    Ok(bytes)
}

/// covered 已按首序号排好，检查它们是否连续铺满 cover 的范围。
fn tiles(cover: &Segment, covered: &[&Segment]) -> bool {
    let mut next = cover.first;
    for source in covered {
        if source.first > next {
            return false;
        }
        next = next.max(source.last + 1);
    }
    next > cover.last
}

pub(super) fn compact(
    storage: &dyn Storage,
    own: Vec<Segment>,
    allowed: &dyn Fn() -> bool,
) -> Result<()> {
    if !allowed() {
        return Ok(());
    }
    // 本轮两次 push 可能给出同名段；列举结果与新写段先按键去重。
    let mut all = own
        .into_iter()
        .map(|s| (s.key.clone(), s))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect::<Vec<_>>();
    all.sort_by_key(|s| (s.first, std::cmp::Reverse(s.last)));

    // 上轮写成但删到一半：验证已有覆盖段及残留源段后继续删，不重写满段。
    for cover in &all {
        let covered = all
            .iter()
            .filter(|s| s.key != cover.key && s.first >= cover.first && s.last <= cover.last)
            .collect::<Vec<_>>();
        if covered.is_empty() {
            continue;
        }
        let bytes = read_listed(storage, cover)?;
        // 写到一半的覆盖段（上次合并或推送中断），读取方本来就跳过。
        // 源段仍连续铺满它的范围时删掉它，下轮重新合并；否则每轮都会卡在这里。
        let Some(ops) = decode_segment(&bytes, cover) else {
            if !tiles(cover, &covered) {
                return Err(Error::Sync(format!("覆盖段残缺且源段不全：{}", cover.key)));
            }
            if allowed() {
                storage.delete(&cover.key)?;
            }
            return Ok(());
        };
        if !ops.iter().all(|op| op.change.known()) {
            return Err(Error::Sync(format!("覆盖段校验失败：{}", cover.key)));
        }
        let lines = bytes.split_inclusive(|b| *b == b'\n').collect::<Vec<_>>();
        let covered = covered
            .into_iter()
            .take(MAX_SOURCES - 1)
            .collect::<Vec<_>>();
        for source in &covered {
            if !allowed() {
                return Ok(());
            }
            let old = read_listed(storage, source)?;
            let expected = lines
                [(source.first - cover.first) as usize..=(source.last - cover.first) as usize]
                .concat();
            // 写到一半的源段同样跳过比较，随后删掉；格式完整但内容不同说明序号冲突，全部保留。
            if decode_segment(&old, source).is_some() && old != expected {
                return Err(Error::Sync("重叠段内容不一致，保留全部源段".into()));
            }
        }
        if !allowed() {
            return Ok(());
        }
        if !storage
            .list(&format!(".sticker-manager/ops/{}/", cover.device))?
            .iter()
            .any(|f| f.key == cover.key && f.size == bytes.len() as u64)
        {
            return Err(Error::Sync("覆盖段尚未完整送达".into()));
        }
        for source in covered {
            if !allowed() {
                return Ok(());
            }
            storage.delete(&source.key)?;
        }
        return Ok(());
    }

    let small = all.iter().filter(|s| s.size < LIMIT).collect::<Vec<_>>();
    if small.len() < MIN_SOURCES {
        return Ok(());
    }
    let mut sources = Vec::<&Segment>::new();
    let mut size = 0;
    for source in small {
        if let Some(last) = sources.last()
            && source.first != last.last + 1
        {
            if sources.len() >= 2 {
                break;
            }
            sources.clear();
            size = 0;
        }
        size += source.size;
        sources.push(source);
        // 完整纳入最后一个源段，达到 4 MiB 后封段，最多不足 8 MiB。
        if size >= LIMIT || sources.len() == MAX_SOURCES {
            break;
        }
    }
    if sources.len() < 2 {
        return Ok(());
    }
    let mut bytes = Vec::new();
    for source in &sources {
        if !allowed() {
            return Ok(());
        }
        bytes.extend(read_checked(storage, source)?);
    }
    let first = sources[0];
    let key = format!(
        ".sticker-manager/ops/{}/{:012}-{:012}.jsonl",
        first.device,
        first.first,
        sources.last().unwrap().last
    );
    if !allowed() {
        return Ok(());
    }
    storage.write(&key, &bytes)?;
    if !allowed() {
        return Ok(());
    }
    if !storage
        .list(&format!(".sticker-manager/ops/{}/", first.device))?
        .iter()
        .any(|f| f.key == key && f.size == bytes.len() as u64)
    {
        return Err(Error::Sync("合并段尚未完整送达".into()));
    }
    for source in sources {
        if !allowed() {
            return Ok(());
        }
        storage.delete(&source.key)?;
    }
    Ok(())
}
