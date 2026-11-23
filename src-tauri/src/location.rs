use crate::{
    jobs::{Job, LibraryJobs},
    settings::SettingsState,
    sync::{self, SyncState},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use sticker_core::{
    Error, Library,
    relocate::{self, CopyOutcome, RelocateProgress, TargetRules, display_path},
};
use tauri::{AppHandle, Emitter, Manager};
const POINTER: &str = "library-location.json";
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum LocationMode {
    Default,
    Portable,
    Override,
}
pub struct LocationState {
    pub base: PathBuf,
    pub mode: LocationMode,
    pub guarded: Vec<PathBuf>,
}
#[derive(Serialize, Deserialize)]
struct Pointer {
    version: u32,
    path: PathBuf,
}
fn read_pointer(base: &Path) -> Result<Option<PathBuf>, String> {
    let data = match fs::read(base.join(POINTER)) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        r => r.map_err(|e| e.to_string())?,
    };
    let p: Pointer = serde_json::from_slice(&data).map_err(|_| "位置记录已损坏")?;
    if p.version > 1 {
        return Err("位置记录由更新版本的应用写入，请先升级应用".into());
    }
    if p.version != 1 || !p.path.is_absolute() {
        return Err("位置记录已损坏".into());
    }
    Ok((p.path != base).then_some(p.path))
}
fn sync_base(_base: &Path) {
    #[cfg(unix)]
    if let Err(e) = fs::File::open(_base).and_then(|f| f.sync_all()) {
        eprintln!("刷新位置记录目录失败：{e}")
    }
}
fn write_pointer(base: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(base)?;
    let mut f = tempfile::NamedTempFile::new_in(base)?;
    f.write_all(&serde_json::to_vec_pretty(&Pointer {
        version: 1,
        path: display_path(target),
    })?)?;
    f.as_file().sync_all()?;
    f.persist(base.join(POINTER))?;
    sync_base(base);
    Ok(())
}
fn clear_pointer(base: &Path) -> io::Result<()> {
    match fs::remove_file(base.join(POINTER)) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        r => {
            r?;
            sync_base(base);
            Ok(())
        }
    }
}
pub fn resolve_root(base: &Path, mode: LocationMode) -> Result<(PathBuf, bool), (PathBuf, String)> {
    if mode != LocationMode::Portable {
        let record = base.join(POINTER);
        let hint = format!("删除 {} 可改用默认位置。", record.display());
        if let Some(path) = read_pointer(base)
            .map_err(|e| (record, format!("无法读取仓库位置记录：{e}。{hint}")))?
        {
            if !path.is_dir() {
                return Err((
                    path,
                    format!("找不到仓库文件夹。请连接磁盘后重新打开应用。{hint}"),
                ));
            }
            if !path.join("library.db").is_file() {
                return Err((path, format!("这个位置没有仓库数据（library.db）。{hint}")));
            }
            return Ok((path, true));
        }
    }
    Ok((base.into(), false))
}
pub fn sync_folder(app: &AppHandle) -> Option<PathBuf> {
    match app.state::<SettingsState>().get().sync?.target {
        sticker_core::sync::network::StorageConfig::Folder { folder } => Some(folder),
        _ => None,
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryLocationInfo {
    path: PathBuf,
    default_path: PathBuf,
    is_custom: bool,
    can_move: bool,
    cannot_move_reason: Option<String>,
}
const PORTABLE: &str =
    "便携版的仓库固定在程序所在文件夹的 data 里；要换位置，把整个程序文件夹移走即可";
fn info(app: &AppHandle) -> LibraryLocationInfo {
    let state = app.state::<LocationState>();
    let path = app.state::<SettingsState>().root();
    let can_move = state.mode != LocationMode::Portable;
    LibraryLocationInfo {
        is_custom: path.canonicalize().ok() != state.base.canonicalize().ok(),
        path,
        default_path: state.base.clone(),
        can_move,
        cannot_move_reason: (!can_move).then(|| PORTABLE.into()),
    }
}
#[tauri::command]
pub async fn library_location(app: AppHandle) -> LibraryLocationInfo {
    info(&app)
}
fn validate(app: &AppHandle, path: &Path, source: &Path) -> Result<PathBuf, String> {
    let state = app.state::<LocationState>();
    if state.mode == LocationMode::Portable {
        return Err(PORTABLE.into());
    }
    let mut forbidden: Vec<_> = state
        .guarded
        .iter()
        .cloned()
        .map(|p| (p, "调试时不能把仓库移到真实仓库目录"))
        .collect();
    let cloud = "不能放在 iCloud 云盘、OneDrive 等网盘同步的文件夹里";
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        for d in ["Library/Mobile Documents", "Library/CloudStorage"] {
            forbidden.push((Path::new(&home).join(d), cloud))
        }
    }
    #[cfg(windows)]
    for key in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(p) = std::env::var_os(key) {
            forbidden.push((p.into(), cloud))
        }
    }
    relocate::check_relocation_target(
        path,
        &TargetRules {
            current_root: source,
            default_root: &state.base,
            pointer_name: POINTER,
            sync_folder: sync_folder(app).as_deref(),
            forbidden: &forbidden,
        },
    )
    .map_err(|e| e.to_string())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTargetInfo {
    path: PathBuf,
    is_default: bool,
    files: usize,
    bytes: u64,
}
#[tauri::command]
pub async fn inspect_library_target(
    app: AppHandle,
    path: PathBuf,
) -> Result<LibraryTargetInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if app.state::<LibraryJobs>().moving() {
            return Err("正在移动仓库".into());
        }
        let source = app.state::<SettingsState>().root();
        let target = validate(&app, &path, &source)?;
        let size = relocate::content_size(&source).map_err(|e| e.to_string())?;
        Ok(LibraryTargetInfo {
            is_default: Some(&target)
                == app
                    .state::<LocationState>()
                    .base
                    .canonicalize()
                    .ok()
                    .as_ref(),
            path: display_path(&target),
            files: size.files,
            bytes: size.bytes,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[derive(Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
enum MoveStage {
    Preparing,
    Copying,
    Switching,
    Cleaning,
}
#[derive(Clone, Serialize)]
pub struct MoveReport {
    path: PathBuf,
    previous: PathBuf,
    leftovers: Vec<PathBuf>,
}
#[derive(Clone, Serialize, Default)]
pub struct MoveStatus {
    running: bool,
    stage: Option<MoveStage>,
    progress: RelocateProgress,
    report: Option<MoveReport>,
    error: Option<String>,
}
#[derive(Default)]
pub struct MoveState {
    status: Mutex<MoveStatus>,
    cancelled: AtomicBool,
}
impl MoveState {
    fn update(&self, app: &AppHandle, f: impl FnOnce(&mut MoveStatus)) {
        let snapshot = {
            let mut s = self.status.lock().unwrap();
            f(&mut s);
            s.clone()
        };
        let _ = app.emit("library-move-status-changed", snapshot);
    }
}
#[tauri::command]
pub fn library_move_status(app: AppHandle) -> MoveStatus {
    app.state::<MoveState>().status.lock().unwrap().clone()
}
#[tauri::command]
pub fn cancel_library_move(app: AppHandle) {
    let state = app.state::<MoveState>();
    if state.status.lock().unwrap().stage == Some(MoveStage::Copying) {
        state.cancelled.store(true, Ordering::Relaxed)
    }
}
fn move_blocking(app: &AppHandle, path: &Path) -> Result<MoveReport, String> {
    let sync = app.state::<Arc<SyncState>>();
    let run = sync.pause_rounds(app);
    let settings = app.state::<SettingsState>();
    let source = settings.root();
    let location = app.state::<LocationState>();
    let state = app.state::<MoveState>();
    let result = (|| {
        let target = validate(app, path, &source)?;
        let default = location.base.canonicalize().map_err(|e| e.to_string())? == target;
        let mut owned = Vec::new();
        let swap = (|| -> Result<(), Error> {
            for d in ["objects", "thumbs"] {
                let dir = target.join(d);
                fs::create_dir(&dir)?;
                owned.push(dir.clone());
                app.asset_protocol_scope()
                    .allow_directory(dir, false)
                    .map_err(|e| Error::LibraryLocation(e.to_string()))?
            }
            state.update(app, |s| s.stage = Some(MoveStage::Copying));
            let mut last = Instant::now() - Duration::from_secs(1);
            if matches!(
                relocate::copy_content(
                    &source,
                    &target,
                    &|| state.cancelled.load(Ordering::Relaxed),
                    &mut |p| {
                        if last.elapsed() >= Duration::from_millis(150) || p.copied == p.total {
                            state.update(app, |s| s.progress = p);
                            last = Instant::now()
                        }
                    }
                )?,
                CopyOutcome::Cancelled
            ) {
                return Err(Error::LibraryLocation("已取消移动，仓库仍在原位置".into()));
            }
            state.update(app, |s| s.stage = Some(MoveStage::Switching));
            let lib = app.state::<Mutex<Library>>();
            let mut lib = lib.lock().map_err(|_| Error::LibraryUnavailable)?;
            let mut root = settings.lock_root();
            for d in ["library.db", "library.db-shm", "library.db-wal", "local"] {
                if target.join(d).try_exists()? {
                    return Err(Error::LibraryLocation(
                        "目标文件夹在移动期间发生变化".into(),
                    ));
                }
            }
            owned.extend(
                ["library.db", "library.db-shm", "library.db-wal", "local"].map(|d| target.join(d)),
            );
            lib.relocate(&target, &mut |p| {
                if default {
                    clear_pointer(&location.base)
                } else {
                    write_pointer(&location.base, p)
                }?;
                Ok(())
            })?;
            *root = display_path(&target);
            Ok(())
        })();
        if let Err(e) = swap {
            for p in owned {
                let r = if p.is_dir() {
                    fs::remove_dir_all(&p)
                } else {
                    fs::remove_file(&p)
                };
                if let Err(err) = r
                    && err.kind() != io::ErrorKind::NotFound
                {
                    eprintln!("清理 {} 失败：{err}", p.display())
                }
            }
            return Err(e.to_string());
        }
        Ok(target)
    })();
    drop(run);
    sync::resume_after_relocation(app);
    let target = result?;
    let _ = app.emit("library-location-changed", info(app));
    crate::commands::notify_library_changed(app);
    state.update(app, |s| s.stage = Some(MoveStage::Cleaning));
    Ok(MoveReport {
        path: display_path(&target),
        leftovers: relocate::remove_content(&source),
        previous: source,
    })
}
#[tauri::command]
pub async fn move_library(app: AppHandle, path: PathBuf) -> Result<MoveReport, String> {
    let jobs = app.state::<LibraryJobs>();
    let _job = jobs.begin(Job::Move)?;
    let state = app.state::<MoveState>();
    state.cancelled.store(false, Ordering::Relaxed);
    state.update(&app, |s| {
        *s = MoveStatus {
            running: true,
            stage: Some(MoveStage::Preparing),
            ..Default::default()
        }
    });
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || move_blocking(&handle, &path))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    state.update(&app, |s| {
        s.running = false;
        s.stage = None;
        match &result {
            Ok(r) => s.report = Some(r.clone()),
            Err(e) => s.error = Some(e.clone()),
        }
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_roundtrip_and_fail_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let custom = tmp.path().join("custom");
        clear_pointer(&base).unwrap();
        write_pointer(&base, &custom).unwrap();
        assert_eq!(fs::read_dir(&base).unwrap().count(), 1);
        assert_eq!(read_pointer(&base).unwrap(), Some(custom.clone()));
        assert!(
            resolve_root(&base, LocationMode::Default)
                .unwrap_err()
                .1
                .contains("找不到")
        );
        fs::create_dir(&custom).unwrap();
        assert!(resolve_root(&base, LocationMode::Default).is_err());
        assert!(Library::open_existing(&custom).is_err());
        assert!(!custom.join("library.db").exists());
        drop(Library::open(&custom).unwrap());
        assert_eq!(
            resolve_root(&base, LocationMode::Default).unwrap(),
            (custom, true)
        );
        for data in [
            r#"{"version":2,"path":"v2"}"#,
            r#"{"version":1,"path":"relative"}"#,
            "broken",
        ] {
            fs::write(base.join(POINTER), data).unwrap();
            assert!(read_pointer(&base).is_err());
            assert_eq!(
                resolve_root(&base, LocationMode::Portable).unwrap(),
                (base.clone(), false)
            )
        }
        clear_pointer(&base).unwrap();
        assert_eq!(read_pointer(&base).unwrap(), None)
    }
    #[cfg(windows)]
    #[test]
    fn windows_display_paths() {
        assert_eq!(display_path(Path::new(r"\\?\C:\lib")), Path::new(r"C:\lib"));
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share")),
            Path::new(r"\\server\share")
        )
    }
}
