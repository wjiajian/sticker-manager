//! 0.1.3 及以前的应用标识符是 com.shaojian.sticker-manager，之后改为 com.wjiajian.sticker-manager。
//! 仓库目录以标识符命名，第一次以新标识符启动时把旧仓库整个搬过来，设置和同步凭据都在仓库里。
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

pub const PREVIOUS_IDENTIFIER: &str = "com.shaojian.sticker-manager";

/// 新仓库目录不存在或为空、旧目录存在时，把旧目录改名过去，返回是否搬过。
/// 两者在同一个父目录下，rename 是原子的；新目录已有内容时不动任何一边。
pub fn migrate_previous_library(root: &Path) -> std::io::Result<bool> {
    migrate_named_library(root, PREVIOUS_IDENTIFIER)
}

#[cfg(target_os = "windows")]
pub fn migrate_windows_library(root: &Path) -> std::io::Result<bool> {
    if migrate_named_library(root, "com.wjiajian.sticker-manager")? {
        return Ok(true);
    }
    migrate_previous_library(root)
}

fn migrate_named_library(root: &Path, previous_name: &str) -> std::io::Result<bool> {
    let Some(previous) = root.parent().map(|parent| parent.join(previous_name)) else {
        return Ok(false);
    };
    if previous == root || !previous.is_dir() {
        return Ok(false);
    }
    match fs::read_dir(root) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Ok(false);
            }
            fs::remove_dir(root)?;
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(&previous, root)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_migration_prefers_current_identifier_and_preserves_existing_data() {
        let parent = tempfile::tempdir().unwrap();
        let current = parent.path().join("com.wjiajian.sticker-manager");
        let legacy = parent.path().join(PREVIOUS_IDENTIFIER);
        let root = parent.path().join("sticker-manager");
        fs::create_dir(&current).unwrap();
        fs::create_dir(&legacy).unwrap();
        fs::write(current.join("library.db"), b"current").unwrap();
        fs::write(legacy.join("library.db"), b"legacy").unwrap();
        assert!(migrate_windows_library(&root).unwrap());
        assert_eq!(fs::read(root.join("library.db")).unwrap(), b"current");
        assert!(!migrate_windows_library(&root).unwrap());
        assert_eq!(fs::read(legacy.join("library.db")).unwrap(), b"legacy");
    }

    fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let parent = tempfile::tempdir().unwrap();
        let previous = parent.path().join(PREVIOUS_IDENTIFIER);
        let root = parent.path().join("com.wjiajian.sticker-manager");
        fs::create_dir_all(previous.join("objects")).unwrap();
        fs::write(previous.join("library.db"), b"db").unwrap();
        (parent, previous, root)
    }

    #[test]
    fn moves_previous_library_when_new_root_is_missing_or_empty() {
        for create_empty_root in [false, true] {
            let (_parent, previous, root) = setup();
            if create_empty_root {
                fs::create_dir(&root).unwrap();
            }
            assert!(migrate_previous_library(&root).unwrap());
            assert!(!previous.exists());
            assert_eq!(fs::read(root.join("library.db")).unwrap(), b"db");
            assert!(root.join("objects").is_dir());
        }
    }

    #[test]
    fn leaves_both_alone_when_new_root_has_content() {
        let (_parent, previous, root) = setup();
        fs::create_dir(&root).unwrap();
        fs::write(root.join("library.db"), b"new").unwrap();
        assert!(!migrate_previous_library(&root).unwrap());
        assert_eq!(fs::read(previous.join("library.db")).unwrap(), b"db");
        assert_eq!(fs::read(root.join("library.db")).unwrap(), b"new");
    }

    #[test]
    fn does_nothing_without_previous_library() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("com.wjiajian.sticker-manager");
        assert!(!migrate_previous_library(&root).unwrap());
        assert!(!root.exists());
    }
}
