use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use sticker_core::{ImportFailure, ImportReport};
use tauri::{AppHandle, Emitter};
use tempfile::TempDir;

pub const MAX_ITEMS: usize = 256;
pub const MAX_BYTES: usize = 100 * 1024 * 1024;
pub const UNSUPPORTED_DROP: &str = "拖入来源没有提供图片或文件，请先保存图片后导入";

pub struct StagingRoot(pub PathBuf);
#[derive(Clone, Copy)]
pub enum Origin {
    Drop,
    Clipboard,
}
impl Origin {
    fn name(self) -> &'static str {
        match self {
            Self::Drop => "拖入的表情",
            Self::Clipboard => "剪贴板图片",
        }
    }
}
pub struct Staging {
    dir: TempDir,
    origin: Origin,
    bytes: usize,
    paths: Vec<PathBuf>,
    failures: Vec<ImportFailure>,
}
impl Staging {
    pub fn new(root: &Path, origin: Origin) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let dir = tempfile::Builder::new()
            .prefix("import-")
            .tempdir_in(root)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            dir,
            origin,
            bytes: 0,
            paths: vec![],
            failures: vec![],
        })
    }
    #[cfg(target_os = "macos")]
    pub fn path(&self) -> &Path {
        self.dir.path()
    }
    pub fn add_bytes(&mut self, bytes: &[u8]) -> Result<PathBuf, String> {
        let ext = sniff(bytes).ok_or("无法识别图片格式")?;
        self.add_named(&format!("{}.{ext}", self.origin.name()), bytes)
    }
    pub fn add_named(&mut self, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        let name = name.rsplit(['/', '\\']).next().unwrap_or("");
        if name.is_empty()
            || matches!(name, "." | "..")
            || name.contains(':')
            || name.chars().any(char::is_control)
        {
            return Err("图片文件名无效".into());
        }
        #[cfg(target_os = "windows")]
        {
            let base = name
                .split('.')
                .next()
                .unwrap_or(name)
                .trim_end_matches(' ')
                .to_ascii_uppercase();
            let reserved = matches!(
                base.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            ) || base
                .strip_prefix("COM")
                .or_else(|| base.strip_prefix("LPT"))
                .is_some_and(|suffix| {
                    matches!(
                        suffix,
                        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                    )
                });
            if reserved || name.ends_with([' ', '.']) || name.chars().any(|c| "<>\"|?*".contains(c))
            {
                return Err("图片文件名无效".into());
            }
        }
        self.check(bytes.len())?;
        let dir = self.dir.path().join(self.paths.len().to_string());
        fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(name);
        if let Err(e) = fs::write(&path, bytes) {
            let _ = fs::remove_dir_all(dir);
            return Err(e.to_string());
        }
        self.bytes += bytes.len();
        self.paths.push(path.clone());
        Ok(path)
    }
    fn check(&self, len: usize) -> Result<(), String> {
        if self.paths.len() >= MAX_ITEMS {
            return Err("图片超过 256 项".into());
        }
        if len > MAX_BYTES - self.bytes {
            return Err("图片数据总量超过 100 MB".into());
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    pub fn adopt(&mut self, path: &Path) -> Result<(), String> {
        let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink()
            || !(metadata.is_file() || metadata.is_dir())
            || !path
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(self.dir.path().canonicalize().map_err(|e| e.to_string())?)
        {
            return Err("接收的文件不在暂存目录内".into());
        }
        let len = if metadata.is_file() {
            usize::try_from(metadata.len()).unwrap_or(usize::MAX)
        } else {
            0
        };
        self.check(len)?;
        self.bytes += len;
        self.paths.push(path.to_owned());
        Ok(())
    }
    pub fn fail(&mut self, label: &str, message: impl Into<String>, count: usize) {
        self.failures.push(ImportFailure {
            path: label.into(),
            message: message.into(),
            count,
        });
    }
    pub fn finish(self) -> Staged {
        Staged {
            paths: self.paths,
            failures: self.failures,
            dir: Some(self.dir),
        }
    }
}
pub struct Staged {
    pub paths: Vec<PathBuf>,
    pub failures: Vec<ImportFailure>,
    dir: Option<TempDir>,
}
impl Staged {
    pub fn existing(paths: Vec<PathBuf>) -> Self {
        Self {
            paths,
            failures: vec![],
            dir: None,
        }
    }
    fn amend(&mut self, report: &mut ImportReport) {
        if let Some(dir) = &self.dir {
            for failure in &mut report.failures {
                if failure.path.starts_with(dir.path()) {
                    failure.path = failure.path.file_name().unwrap_or_default().into();
                }
            }
        }
        report.failures.append(&mut self.failures);
    }
}
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else {
        None
    }
}
pub async fn import(
    app: AppHandle,
    mut staged: Staged,
    ball: bool,
) -> Result<ImportReport, String> {
    if staged.paths.is_empty()
        && let Some(failure) = staged.failures.first()
    {
        return Err(failure.message.clone());
    }
    let paths = std::mem::take(&mut staged.paths);
    crate::imports::run_with_report(app, paths, ball, move |report| staged.amend(report)).await
}
pub fn sweep(root: &Path, now: SystemTime, max_age: Duration) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("import-") {
            continue;
        }
        if let Ok(meta) = fs::symlink_metadata(entry.path())
            && meta
                .modified()
                .ok()
                .and_then(|m| now.duration_since(m).ok())
                .is_some_and(|age| age > max_age)
        {
            if meta.is_dir() {
                let _ = fs::remove_dir_all(entry.path());
            } else {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}
#[derive(Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ExternalDrop {
    Enter,
    Leave,
    Receiving,
    Rejected { message: String },
    Failed { message: String },
}
pub fn emit(app: &AppHandle, event: ExternalDrop) {
    let _ = app.emit_to("main", "external-drop", event);
}
pub async fn complete(app: AppHandle, staged: Result<Staged, String>, ball: bool) {
    let result = match staged {
        Ok(staged) => import(app.clone(), staged, ball).await,
        Err(e) => Err(e),
    };
    match result {
        Ok(report) if ball => {
            let _ = app.emit_to("ball", "external-drop-result", report);
        }
        Ok(_) => {}
        Err(error) if ball => crate::ball::notify(&app, format!("导入失败：{error}")),
        Err(message) => emit(&app, ExternalDrop::Failed { message }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn format_names_and_lifetime() {
        let root = tempfile::tempdir().unwrap();
        let mut s = Staging::new(root.path(), Origin::Clipboard).unwrap();
        assert!(s.add_bytes(b"invalid").is_err());
        for (data, ext) in [
            (b"GIF87a".as_slice(), "gif"),
            (b"GIF89a".as_slice(), "gif"),
            (b"RIFF....WEBP", "webp"),
            (b"\x89PNG\r\n\x1a\n", "png"),
            (&[255, 216, 255], "jpg"),
        ] {
            let path = s.add_bytes(data).unwrap();
            assert_eq!(path.extension().unwrap(), ext);
            assert_eq!(fs::read(path).unwrap(), data);
        }
        let staged = s.finish();
        let paths = staged.paths.clone();
        drop(staged);
        assert!(paths.iter().all(|p| !p.exists()));

        let existing_file = root.path().join("keep.png");
        fs::write(&existing_file, b"\x89PNG\r\n\x1a\n").unwrap();
        let existing = Staged::existing(vec![existing_file.clone()]);
        drop(existing);
        assert!(existing_file.exists());
    }
    #[test]
    fn quotas_are_checked_without_charging_invalid_data() {
        let root = tempfile::tempdir().unwrap();
        let mut s = Staging::new(root.path(), Origin::Drop).unwrap();
        s.bytes = MAX_BYTES - 6;
        assert!(s.add_bytes(b"not a picture").is_err());
        assert!(s.add_bytes(b"GIF89a").is_ok());
        assert!(s.add_bytes(b"GIF89a").is_err());
        let mut s = Staging::new(root.path(), Origin::Drop).unwrap();
        for _ in 0..MAX_ITEMS {
            s.add_bytes(b"GIF89a").unwrap();
        }
        assert!(s.add_bytes(b"GIF89a").is_err());
    }
    #[test]
    fn sweep_removes_only_old_staging_entries() {
        let root = tempfile::tempdir().unwrap();
        let expired = root.path().join("import-expired");
        let other = root.path().join("unrelated");
        fs::create_dir(&expired).unwrap();
        fs::create_dir(&other).unwrap();
        sweep(root.path(), SystemTime::now(), Duration::from_secs(3600));
        assert!(expired.exists());
        sweep(
            root.path(),
            SystemTime::now() + Duration::from_secs(7200),
            Duration::from_secs(3600),
        );
        assert!(!expired.exists());
        assert!(other.exists());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn promised_path_cannot_escape_staging_or_be_symlink() {
        let root = tempfile::tempdir().unwrap();
        let mut s = Staging::new(root.path(), Origin::Drop).unwrap();
        let external = root.path().join("external.gif");
        fs::write(&external, b"GIF89a").unwrap();
        assert!(s.adopt(&external).is_err());
        let link = s.path().join("link.gif");
        std::os::unix::fs::symlink(&external, &link).unwrap();
        assert!(s.adopt(&link).is_err());
        let local = s.path().join("local.gif");
        fs::write(&local, b"GIF89a").unwrap();
        s.adopt(&local).unwrap();
        assert_eq!(s.finish().paths, [local]);
    }
    #[test]
    fn report_amends_temp_path_and_preserves_failure_counts() {
        let root = tempfile::tempdir().unwrap();
        let mut s = Staging::new(root.path(), Origin::Drop).unwrap();
        let path = s.add_named("folder/bad.gif", b"bad").unwrap();
        s.fail("未收到的文件", "读取失败", 3);
        let mut staged = s.finish();
        let mut report = ImportReport {
            failures: vec![ImportFailure {
                path,
                message: "坏文件".into(),
                count: 1,
            }],
            ..ImportReport::default()
        };
        staged.amend(&mut report);
        assert_eq!(report.failures[0].path, PathBuf::from("bad.gif"));
        assert_eq!(report.failures[1].count, 3);
    }
    #[test]
    fn direct_staged_file_is_uncategorized() {
        let root = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let mut library = sticker_core::Library::open(library.path()).unwrap();
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(1, 1)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let mut s = Staging::new(root.path(), Origin::Clipboard).unwrap();
        s.add_bytes(bytes.get_ref()).unwrap();
        let report = library.import_paths(&s.finish().paths);
        assert_eq!(report.added, 1);
        assert!(library.categories().unwrap().is_empty());
    }

    #[test]
    fn staged_animated_gif_preserves_original_bytes_and_deduplicates() {
        const TWO_FRAME_GIF: &[u8] = &[
            b'G', b'I', b'F', b'8', b'9', b'a', // Header
            1, 0, 1, 0, 0x80, 0, 0, // Logical Screen Descriptor (1x1, 2-color GCT)
            0xff, 0, 0, 0, 0, 0xff, // Global Color Table: red, blue
            0x21, 0xf9, 4, 0, 10, 0, 0, 0, // GCE frame 1
            0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, // Image Descriptor frame 1
            2, 2, 0x44, 0x01, 0, // Image Data frame 1 (index 0)
            0x21, 0xf9, 4, 0, 10, 0, 0, 0, // GCE frame 2
            0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, // Image Descriptor frame 2
            2, 2, 0x4c, 0x01, 0,    // Image Data frame 2 (index 1)
            0x3b, // Trailer
        ];
        let root = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let mut library = sticker_core::Library::open(library.path()).unwrap();

        let mut s = Staging::new(root.path(), Origin::Drop).unwrap();
        let staged_path = s.add_bytes(TWO_FRAME_GIF).unwrap();
        let staged = s.finish();
        let report = library.import_paths(&staged.paths);
        assert_eq!((report.added, report.skipped), (1, 0));
        assert!(report.failures.is_empty());
        let sticker = library.sticker(&report.sticker_ids[0]).unwrap();
        drop(staged);
        assert!(!staged_path.exists());
        assert!(sticker.animated);
        assert_eq!(fs::read(&sticker.object_path).unwrap(), TWO_FRAME_GIF);

        let mut second = Staging::new(root.path(), Origin::Clipboard).unwrap();
        second.add_bytes(TWO_FRAME_GIF).unwrap();
        let duplicate = library.import_paths(&second.finish().paths);
        assert_eq!((duplicate.added, duplicate.existing), (0, 1));
    }
}
