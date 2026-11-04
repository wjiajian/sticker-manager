use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct Portable {
    root: Option<PathBuf>,
}

impl Portable {
    pub fn detect() -> std::io::Result<Self> {
        #[cfg(target_os = "windows")]
        {
            let executable = std::env::current_exe()?;
            let directory = executable
                .parent()
                .ok_or_else(|| std::io::Error::other("无法确定便携程序所在目录"))?;
            Self::at(directory)
        }
        #[cfg(not(target_os = "windows"))]
        Ok(Self::default())
    }

    #[cfg(target_os = "windows")]
    fn at(directory: &std::path::Path) -> std::io::Result<Self> {
        match std::fs::metadata(directory.join("portable.flag")) {
            Ok(metadata) if metadata.is_file() => {
                std::fs::create_dir_all(directory.join("data"))?;
                std::fs::create_dir_all(directory.join("webview"))?;
                Ok(Self {
                    root: Some(directory.to_path_buf()),
                })
            }
            Ok(_) => Err(std::io::Error::other("portable.flag 必须是文件")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }
}

pub fn root(app: &AppHandle) -> Option<PathBuf> {
    app.state::<Portable>().root.clone()
}

pub fn enabled(app: &AppHandle) -> bool {
    root(app).is_some()
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn marker_selects_adjacent_data_and_survives_directory_move() {
        let parent = tempfile::tempdir().unwrap();
        let source = parent.path().join("portable");
        std::fs::create_dir(&source).unwrap();
        assert!(Portable::at(&source).unwrap().root.is_none());
        std::fs::write(source.join("portable.flag"), b"").unwrap();
        assert_eq!(Portable::at(&source).unwrap().root, Some(source.clone()));
        std::fs::write(source.join("data/library.db"), b"existing").unwrap();
        let destination = parent.path().join("moved");
        std::fs::rename(source, &destination).unwrap();
        assert_eq!(
            Portable::at(&destination).unwrap().root,
            Some(destination.clone())
        );
        assert_eq!(
            std::fs::read(destination.join("data/library.db")).unwrap(),
            b"existing"
        );
    }
}
