//! 单后台工作线程；命令只排队，文件后端从不在主线程执行。
use crate::settings::{self, SettingsState, SyncBackend, SyncSettings};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use sticker_core::{
    Library,
    sync::{
        self as core, FolderInfo, Manifest, RemoteDevice,
        storage::{FileEntry, FolderStorage, Storage},
    },
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub phase: Phase,
    pub last_sync_at: Option<i64>,
    pub error: Option<String>,
    pub folder: Option<PathBuf>,
    pub device_name: String,
    pub storage_bytes: i64,
    pub online_devices: usize,
    /// 已收到但还在等原图或其他设备记录送达的改动，属于正常的过渡状态，不算出错。
    pub waiting: usize,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    #[default]
    Unconfigured,
    Syncing,
    Synced,
    Error,
}
struct Runtime {
    generation: u64,
    schedule: Schedule,
    status: SyncStatus,
    devices: Vec<RemoteDevice>,
}
pub struct SyncState {
    inner: Mutex<Runtime>,
    wake: Condvar,
    run: Mutex<()>,
}
struct Schedule {
    requested: bool,
    dirty: Option<Instant>,
    periodic: Instant,
    last_wall: SystemTime,
}
impl Schedule {
    fn new(now: Instant, wall: SystemTime) -> Self {
        Self {
            requested: true,
            dirty: None,
            periodic: now,
            last_wall: wall,
        }
    }
    fn local_change(&mut self, now: Instant) {
        self.dirty = Some(now + Duration::from_secs(10));
    }
    fn take_due(&mut self, now: Instant, wall: SystemTime) -> bool {
        // 线程每秒醒一次；睡眠导致墙钟相邻样本超过 15 秒，即视为唤醒。
        let woke = wall
            .duration_since(self.last_wall)
            .is_ok_and(|d| d > Duration::from_secs(15));
        self.last_wall = wall;
        if self.requested || woke || now >= self.periodic || self.dirty.is_some_and(|d| now >= d) {
            self.requested = false;
            self.dirty = None;
            self.periodic = now + Duration::from_secs(300);
            true
        } else {
            false
        }
    }
}
pub fn start(app: AppHandle) {
    app.manage(Arc::new(SyncState {
        inner: Mutex::new(Runtime {
            generation: 0,
            schedule: Schedule::new(Instant::now(), SystemTime::now()),
            status: SyncStatus::default(),
            devices: Vec::new(),
        }),
        wake: Condvar::new(),
        run: Mutex::new(()),
    }));
    std::thread::spawn(move || {
        let state = app.state::<Arc<SyncState>>().inner().clone();
        loop {
            let generation = {
                let mut runtime = state.inner.lock().unwrap();
                while !runtime.schedule.take_due(Instant::now(), SystemTime::now()) {
                    runtime = state
                        .wake
                        .wait_timeout(runtime, Duration::from_secs(1))
                        .unwrap()
                        .0;
                }
                runtime.generation
            };
            let _running = state.run.lock().unwrap();
            if state.inner.lock().unwrap().generation != generation {
                continue;
            }
            let config = app.state::<SettingsState>().get().sync;
            if let Some(config) = config {
                publish(&app, &state, generation, Phase::Syncing, None, None);
                let result = FolderStorage::new(&config.folder).and_then(|inner| {
                    let storage = ActiveStorage {
                        inner,
                        state: &state,
                        generation,
                    };
                    core::synchronize(&app.state::<Mutex<Library>>(), &storage, &config.library_id)
                });
                // 失败前可能已应用其他设备的一段操作，仍然通知两个窗口刷新。
                let _ = app.emit("library-changed", ());
                match result {
                    Ok(report) => {
                        let mut runtime = state.inner.lock().unwrap();
                        if runtime.generation == generation {
                            runtime.devices = report.devices;
                            runtime.status.waiting = report.pending;
                        }
                        drop(runtime);
                        publish(
                            &app,
                            &state,
                            generation,
                            Phase::Synced,
                            None,
                            Some(report.last_sync_at),
                        );
                    }
                    Err(error) => publish(
                        &app,
                        &state,
                        generation,
                        Phase::Error,
                        Some(error.to_string()),
                        None,
                    ),
                }
            } else {
                publish(&app, &state, generation, Phase::Unconfigured, None, None);
            }
            // 同步 I/O 自己耗时很长不当作一次唤醒；运行中收到的 requested/dirty 保留。
            state.inner.lock().unwrap().schedule.last_wall = SystemTime::now();
        }
    });
}
fn publish(
    app: &AppHandle,
    state: &SyncState,
    generation: u64,
    phase: Phase,
    error: Option<String>,
    last_sync_at: Option<i64>,
) {
    let (device_name, storage_bytes) = {
        let lib = app.state::<Mutex<Library>>();
        let Ok(lib) = lib.lock() else { return };
        (lib.device().name.clone(), lib.storage_bytes().unwrap_or(0))
    };
    let config = app.state::<SettingsState>().get().sync;
    let mut runtime = state.inner.lock().unwrap();
    if runtime.generation != generation {
        return;
    }
    runtime.status.phase = phase;
    runtime.status.error = error;
    if let Some(time) = last_sync_at {
        runtime.status.last_sync_at = Some(time);
    }
    runtime.status.folder = config.map(|s| s.folder);
    runtime.status.device_name = device_name;
    runtime.status.storage_bytes = storage_bytes;
    runtime.status.online_devices = online(&runtime.devices);
    let status = runtime.status.clone();
    drop(runtime);
    let _ = app.emit("sync-status-changed", status);
}
fn online(devices: &[RemoteDevice]) -> usize {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    devices
        .iter()
        .filter(|d| (now - 600_000..=now + 60_000).contains(&d.last_sync_at))
        .count()
}
fn request(app: &AppHandle, changed: bool) {
    let Some(state) = app.try_state::<Arc<SyncState>>() else {
        return;
    };
    let mut runtime = state.inner.lock().unwrap();
    if changed {
        runtime.schedule.local_change(Instant::now());
    } else {
        runtime.schedule.requested = true;
    }
    state.wake.notify_one();
}
pub fn local_changed(app: &AppHandle) {
    request(app, true);
}
fn configuration_changed(app: &AppHandle) {
    let state = app.state::<Arc<SyncState>>();
    let mut runtime = state.inner.lock().unwrap();
    runtime.generation += 1;
    runtime.schedule.requested = true;
    runtime.status = SyncStatus::default();
    runtime.devices.clear();
    let generation = runtime.generation;
    drop(runtime);
    state.wake.notify_one();
    let phase = if app.state::<SettingsState>().get().sync.is_some() {
        Phase::Syncing
    } else {
        Phase::Unconfigured
    };
    publish(app, &state, generation, phase, None, None);
}
struct ActiveStorage<'a> {
    inner: FolderStorage,
    state: &'a SyncState,
    generation: u64,
}
impl ActiveStorage<'_> {
    fn check(&self) -> sticker_core::Result<()> {
        if self.state.inner.lock().unwrap().generation != self.generation {
            Err(sticker_core::Error::Sync(
                "同步设置已改变，本次同步已停止".into(),
            ))
        } else {
            Ok(())
        }
    }
}
impl Storage for ActiveStorage<'_> {
    fn list(&self, p: &str) -> sticker_core::Result<Vec<FileEntry>> {
        self.check()?;
        self.inner.list(p)
    }
    fn read(&self, k: &str) -> sticker_core::Result<Option<Vec<u8>>> {
        self.check()?;
        self.inner.read(k)
    }
    fn write(&self, k: &str, b: &[u8]) -> sticker_core::Result<()> {
        self.check()?;
        self.inner.write(k, b)
    }
    fn delete(&self, k: &str) -> sticker_core::Result<()> {
        self.check()?;
        self.inner.delete(k)
    }
}

#[tauri::command]
pub async fn sync_status(app: AppHandle) -> Result<SyncStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Arc<SyncState>>();
        let mut status = state.inner.lock().unwrap().status.clone();
        let library = app.state::<Mutex<Library>>();
        let library = library.lock().map_err(|e| e.to_string())?;
        status.device_name = library.device().name.clone();
        status.storage_bytes = library.storage_bytes().map_err(|e| e.to_string())?;
        status.folder = app.state::<SettingsState>().get().sync.map(|s| s.folder);
        status.online_devices = online(&state.inner.lock().unwrap().devices);
        Ok(status)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn sync_now(app: AppHandle) {
    request(&app, false);
}
#[tauri::command]
pub async fn inspect_sync_folder(app: AppHandle, path: PathBuf) -> Result<FolderInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        core::inspect_folder(&path, &app.state::<SettingsState>().root).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn join_sync_folder(
    app: AppHandle,
    path: PathBuf,
    expected_id: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let info = core::inspect_folder(&path, &app.state::<SettingsState>().root)
            .map_err(|e| e.to_string())?;
        if info.manifest.as_ref().map(|m| &m.id) != expected_id.as_ref() {
            return Err("文件夹里的仓库已变化，请重新选择并确认".into());
        }
        let storage = FolderStorage::new(&info.path).map_err(|e| e.to_string())?;
        let state = app.state::<Arc<SyncState>>();
        // 等旧后端的当前数据库阶段结束，再拍新快照，防止漏掉刚到的远端改动。
        state.inner.lock().unwrap().generation += 1;
        let _running = state.run.lock().unwrap();
        let result = (|| {
            let manifest = match info.manifest {
                Some(expected) => {
                    let actual = Manifest::read(&storage).map_err(|e| e.to_string())?;
                    if actual.id != expected.id {
                        return Err("文件夹里的仓库已变化，请重新选择并确认".into());
                    }
                    actual
                }
                None => Manifest::create(&storage).map_err(|e| e.to_string())?,
            };
            core::join(&app.state::<Mutex<Library>>(), &storage, &manifest.id)
                .map_err(|e| e.to_string())?;
            settings::update(&app, |s| {
                s.sync = Some(SyncSettings {
                    backend: SyncBackend::Folder,
                    folder: info.path,
                    library_id: manifest.id,
                })
            })
            .map(|_| ())
        })();
        configuration_changed(&app);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn stop_sync(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Arc<SyncState>>();
        state.inner.lock().unwrap().generation += 1;
        let _running = state.run.lock().unwrap();
        let result = settings::update(&app, |s| s.sync = None).map(|_| ());
        configuration_changed(&app);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn set_device_name(app: AppHandle, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Mutex<Library>>()
            .lock()
            .map_err(|e| e.to_string())?
            .rename_device(&name)
            .map_err(|e| e.to_string())?;
        request(&app, false);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_debounce_periodic_and_wake() {
        let now = Instant::now();
        let wall = SystemTime::now();
        let mut s = Schedule::new(now, wall);
        assert!(s.take_due(now, wall));
        s.local_change(now);
        s.local_change(now + Duration::from_secs(5));
        assert!(!s.take_due(
            now + Duration::from_secs(10),
            wall + Duration::from_secs(10)
        ));
        assert!(s.take_due(
            now + Duration::from_secs(15),
            wall + Duration::from_secs(15)
        ));
        assert!(s.take_due(
            now + Duration::from_secs(315),
            wall + Duration::from_secs(16)
        ));
        assert!(s.take_due(
            now + Duration::from_secs(316),
            wall + Duration::from_secs(100)
        ));
    }
    #[test]
    fn triggers_during_run_collapse_to_one_followup() {
        let now = Instant::now();
        let wall = SystemTime::now();
        let mut s = Schedule::new(now, wall);
        assert!(s.take_due(now, wall));
        s.requested = true;
        s.requested = true;
        assert!(s.take_due(now, wall));
        assert!(!s.take_due(now, wall));
    }
}
