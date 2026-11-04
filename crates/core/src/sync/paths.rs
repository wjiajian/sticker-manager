//! 远端目录只承载一层分类；排序直接比较 UTF-8 字节，不依赖平台。
use crate::{Error, Result};

pub(super) const META: &str = ".sticker-manager/";

pub(super) fn component(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains("..")
        && !name.contains(['/', '\\', '\0'])
}

pub(super) fn image(key: &str) -> Option<(Option<&str>, &str, &str)> {
    let parts = key.split('/').collect::<Vec<_>>();
    if !(1..=2).contains(&parts.len()) || parts.iter().any(|p| !component(p)) {
        return None;
    }
    let file = parts.last()?;
    let lower = file.to_ascii_lowercase();
    if lower.contains("conflict") || lower.contains("冲突副本") || lower.ends_with(".icloud") {
        return None;
    }
    let (name, ext) = file.rsplit_once('.')?;
    if !matches!(
        ext.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp"
    ) || name.is_empty()
    {
        return None;
    }
    let category = (parts.len() == 2 && parts[0] != "未分类").then_some(parts[0]);
    Some((category, name, ext))
}

pub(super) fn key(category: Option<&str>, name: &str, ext: &str, number: usize) -> Result<String> {
    if category.is_some_and(|s| !component(s) || s == "未分类") {
        return Err(Error::Sync("分类名称不能用作同步目录名".into()));
    }
    let mut name: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\') {
                '_'
            } else {
                c
            }
        })
        .collect();
    while name.contains("..") {
        name = name.replace("..", ".");
    }
    while let Some(at) = name.to_ascii_lowercase().find("conflict") {
        name.replace_range(at..at + 8, "冲突");
    }
    while name.contains("冲突副本") {
        name = name.replace("冲突副本", "副本");
    }
    name = name
        .trim_start_matches('.')
        .trim_end_matches(|c: char| c.is_whitespace() || c == '.')
        .to_owned();
    let suffix = if number > 1 {
        format!(" ({number})")
    } else {
        String::new()
    };
    let limit = 200usize.saturating_sub(suffix.len() + ext.len() + 1);
    let mut end = name.len().min(limit);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name.truncate(end);
    name = name
        .trim_end_matches(|c: char| c.is_whitespace() || c == '.')
        .to_owned();
    if name.trim().is_empty() {
        name = "表情".into();
    }
    Ok(format!(
        "{}{}{suffix}.{ext}",
        category.map(|c| format!("{c}/")).unwrap_or_default(),
        name
    ))
}
