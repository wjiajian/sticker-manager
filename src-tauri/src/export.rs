use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use sticker_core::Sticker;

/// 文件引用可能在退出应用后才被读取，所以副本不能用完就删，保留七天。
const RETENTION: Duration = Duration::from_secs(7 * 86400);

/// 复制、拖出时交给其他应用的表情副本。
///
/// 应用常驻运行，可能很久不重启，所以除了启动时，每次准备新副本前也清理过期的。
/// 清理只是尽力而为，删不掉的（比如还被其他应用占用）留到下次，不影响启动和复制。
pub struct ExportDirectory {
    root: PathBuf,
}

impl ExportDirectory {
    pub fn new(root: PathBuf) -> Self {
        let directory = Self { root };
        directory.remove_expired(SystemTime::now());
        directory
    }

    pub fn prepare(&self, sticker: &Sticker) -> std::io::Result<PathBuf> {
        self.remove_expired(SystemTime::now());
        fs::create_dir_all(&self.root)?;
        let directory = tempfile::Builder::new()
            .prefix("export-")
            .tempdir_in(&self.root)?;
        let path = directory
            .path()
            .join(file_name(&sticker.name, &sticker.object_path));
        // 副本与仓库原文件独立，目标应用即使修改副本也不会破坏原文件。
        fs::copy(&sticker.object_path, &path)?;
        let _ = directory.keep();
        Ok(path)
    }

    fn remove_expired(&self, now: SystemTime) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            let expired = entry.file_name().to_string_lossy().starts_with("export-")
                && entry.file_type().is_ok_and(|kind| kind.is_dir())
                && entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .is_ok_and(|modified| {
                        now.duration_since(modified).unwrap_or_default() > RETENTION
                    });
            if expired {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
}

fn file_name(name: &str, original: &Path) -> String {
    let mut stem = String::new();
    for c in name.trim().chars() {
        let c = if c.is_control() || "<>:\"/\\|?*".contains(c) {
            '_'
        } else {
            c
        };
        if stem.len() + c.len_utf8() > 120 {
            break;
        }
        stem.push(c);
    }
    let stem = stem.trim_matches([' ', '.']);
    let stem = if stem.is_empty() { "表情" } else { stem };
    let base = stem.split('.').next().unwrap_or(stem).to_ascii_uppercase();
    let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || base
            .strip_prefix("COM")
            .or_else(|| base.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            });
    let extension = original.extension().unwrap_or_default().to_string_lossy();
    format!("{}{stem}.{extension}", if reserved { "_" } else { "" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sticker_core::{Library, LibraryView};

    fn imported_sticker(dir: &Path) -> Sticker {
        let source = dir.join("收到.png");
        image::RgbaImage::new(2, 2).save(&source).unwrap();
        let mut library = Library::open(&dir.join("library")).unwrap();
        assert_eq!(library.import_paths(&[source]).added, 1);
        library.list_stickers(LibraryView::All).unwrap().remove(0)
    }

    fn after_retention() -> SystemTime {
        SystemTime::now() + RETENTION + Duration::from_secs(60)
    }

    #[test]
    fn export_preserves_bytes_and_name_and_cannot_modify_original() {
        let dir = tempfile::tempdir().unwrap();
        let sticker = imported_sticker(dir.path());
        let original = fs::read(&sticker.object_path).unwrap();
        let exports = ExportDirectory::new(dir.path().join("exports"));
        let path = exports.prepare(&sticker).unwrap();
        assert_eq!(path.file_name().unwrap(), "收到.png");
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::write(&path, "修改副本").unwrap();
        assert_eq!(fs::read(&sticker.object_path).unwrap(), original);
        let second = exports.prepare(&sticker).unwrap();
        assert_ne!(path, second);
        assert_eq!(fs::read(second).unwrap(), original);
    }

    #[test]
    fn removes_only_expired_copies() {
        let dir = tempfile::tempdir().unwrap();
        let sticker = imported_sticker(dir.path());
        let exports = ExportDirectory::new(dir.path().join("exports"));
        let copy = exports.prepare(&sticker).unwrap();
        let unrelated = dir.path().join("exports/其他文件夹");
        fs::create_dir(&unrelated).unwrap();

        exports.remove_expired(SystemTime::now());
        assert!(copy.exists());
        exports.remove_expired(after_retention());
        assert!(!copy.parent().unwrap().exists());
        assert!(unrelated.exists());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_failure_is_skipped() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let sticker = imported_sticker(dir.path());
        let exports = ExportDirectory::new(dir.path().join("exports"));
        let stuck = exports.prepare(&sticker).unwrap();
        let other = exports.prepare(&sticker).unwrap();
        // 去掉写权限后里面的副本删不掉，模拟仍被其他应用占用
        let stuck_dir = stuck.parent().unwrap();
        fs::set_permissions(stuck_dir, fs::Permissions::from_mode(0o555)).unwrap();

        exports.remove_expired(after_retention());
        assert!(stuck.exists());
        assert!(!other.exists());
        assert!(exports.prepare(&sticker).is_ok());
        fs::set_permissions(stuck_dir, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn unusable_directory_fails_the_copy_not_startup() {
        let dir = tempfile::tempdir().unwrap();
        let sticker = imported_sticker(dir.path());
        let blocked = dir.path().join("不是文件夹");
        fs::write(&blocked, "").unwrap();
        let exports = ExportDirectory::new(blocked);
        assert!(exports.prepare(&sticker).is_err());
    }

    #[test]
    fn names_are_safe_on_both_platforms() {
        let original = Path::new("hash.gif");
        assert_eq!(file_name("../收到:好的?", original), "_收到_好的_.gif");
        assert_eq!(file_name(" .. ", original), "表情.gif");
        assert_eq!(file_name("CON", original), "_CON.gif");
        assert_eq!(file_name("lpt1.txt", original), "_lpt1.txt.gif");
        assert!(file_name(&"长".repeat(100), original).len() <= 124);
    }
}
