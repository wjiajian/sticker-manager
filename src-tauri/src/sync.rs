//! 单后台工作线程；命令只排队，文件后端从不在主线程执行。
use crate::credentials::{self, CredentialCache, CredentialStore, SystemCredentials};
use crate::settings::{self, SettingsState, SyncSettings};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use sticker_core::{
    Library,
    sync::{
        self as core, FolderInfo, Manifest, RemoteDevice, SyncProgress, TargetInfo,
        network::{CancelCheck, Credentials, NetworkStorage, StorageConfig, probe},
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
    pub target: Option<TargetDescription>,
    pub progress: Option<SyncProgress>,
    pub device_name: String,
    pub storage_bytes: i64,
    pub online_devices: usize,
    /// 已收到但还在等原图或其他设备记录送达的改动，属于正常的过渡状态，不算出错。
    pub waiting: usize,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetDescription {
    pub backend: &'static str,
    pub name: &'static str,
    pub address: String,
    pub short_address: String,
    pub credentials_saved: bool,
}
fn describe(config: &StorageConfig, state: &SyncState) -> TargetDescription {
    let address = config.address();
    let (backend, short_address) = match config {
        StorageConfig::Folder { folder } => (
            "folder",
            folder
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        ),
        StorageConfig::Webdav { .. } => ("webdav", address.clone()),
        StorageConfig::S3 { .. } => ("s3", address.clone()),
        StorageConfig::R2 { .. } => ("r2", address.clone()),
    };
    TargetDescription {
        backend,
        name: config.label(),
        address,
        short_address,
        // 正在读钥匙串（可能停在授权弹窗上）时不等它，按已保存算。
        credentials_saved: config.is_network()
            && state.credentials.try_lock().map_or(true, |c| !c.failed()),
    }
}
#[derive(Clone)]
struct PendingConnection {
    id: String,
    target: StorageConfig,
    credentials: Credentials,
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
    connection: Option<PendingConnection>,
    cancelled_connection: Option<String>,
    progress_event: Instant,
}
pub struct SyncState {
    inner: Mutex<Runtime>,
    wake: Condvar,
    run: Mutex<()>,
    credentials: Mutex<CredentialCache>,
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
            connection: None,
            cancelled_connection: None,
            progress_event: Instant::now(),
        }),
        wake: Condvar::new(),
        run: Mutex::new(()),
        credentials: Mutex::new(CredentialCache::default()),
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
                let result =
                    storage_for(&app, &state, generation, &config.target).and_then(|inner| {
                        let storage = ActiveStorage {
                            inner,
                            state: &state,
                            generation,
                        };
                        core::synchronize_with_progress(
                            &app.state::<Mutex<Library>>(),
                            &storage,
                            &config.library_id,
                            &|progress| {
                                let mut runtime = state.inner.lock().unwrap();
                                if runtime.generation != generation {
                                    return;
                                }
                                let send = runtime.progress_event.elapsed()
                                    >= Duration::from_millis(150)
                                    || progress.total == Some(progress.completed);
                                runtime.status.progress = Some(progress);
                                if send {
                                    runtime.progress_event = Instant::now();
                                    let status = runtime.status.clone();
                                    drop(runtime);
                                    let _ = app.emit("sync-status-changed", status);
                                }
                            },
                        )
                    });
                // 失败前可能已应用其他设备的一段操作，仍然通知两个窗口刷新。
                let _ = app.emit("library-changed", ());
                match result {
                    Ok(report) => {
                        let mut runtime = state.inner.lock().unwrap();
                        if runtime.generation == generation {
                            if report.last_sync_at.is_some() {
                                runtime.devices = report.devices;
                            }
                            runtime.status.waiting = report.pending;
                        }
                        drop(runtime);
                        publish(
                            &app,
                            &state,
                            generation,
                            Phase::Synced,
                            None,
                            report.last_sync_at,
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
    runtime.status.progress = None;
    runtime.status.error = error;
    if let Some(time) = last_sync_at {
        runtime.status.last_sync_at = Some(time);
    }
    runtime.status.target = config.map(|s| describe(&s.target, state));
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
    runtime.connection = None;
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
fn cancellation(state: &Arc<SyncState>, generation: u64) -> CancelCheck {
    let state = Arc::downgrade(state);
    Arc::new(move || {
        state
            .upgrade()
            .is_none_or(|s| s.inner.lock().unwrap().generation != generation)
    })
}
fn credential_account(app: &AppHandle, target: &StorageConfig) -> Result<String, String> {
    let library = app.state::<Mutex<Library>>();
    let id = library
        .lock()
        .map_err(|_| "仓库暂时无法访问")?
        .device()
        .id
        .clone();
    credentials::account(&id, target)
}
fn storage_for(
    app: &AppHandle,
    state: &Arc<SyncState>,
    generation: u64,
    target: &StorageConfig,
) -> sticker_core::Result<Box<dyn Storage>> {
    if let StorageConfig::Folder { folder } = target {
        return Ok(Box::new(FolderStorage::new(folder)?));
    }
    let account = credential_account(app, target).map_err(sticker_core::Error::Sync)?;
    let credentials = state
        .credentials
        .lock()
        .unwrap()
        .read(&SystemCredentials, &account)
        .map_err(sticker_core::Error::Sync)?;
    Ok(Box::new(NetworkStorage::new(
        target,
        &credentials,
        cancellation(state, generation),
    )?))
}
fn generation_check(state: &SyncState, generation: u64) -> Result<(), String> {
    if state.inner.lock().unwrap().generation != generation {
        Err("连接已取消".into())
    } else {
        Ok(())
    }
}

struct ActiveStorage<'a> {
    inner: Box<dyn Storage>,
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
        status.target = app
            .state::<SettingsState>()
            .get()
            .sync
            .map(|s| describe(&s.target, &state));
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
            let manifest = match info.target.manifest {
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
                    target: StorageConfig::Folder { folder: info.path },
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
        {
            let mut runtime = state.inner.lock().unwrap();
            runtime.generation += 1;
            runtime.connection = None;
        }
        let _running = state.run.lock().unwrap();
        let result = (|| {
            let account = match app.state::<SettingsState>().get().sync {
                Some(config) if config.target.is_network() => {
                    Some(credential_account(&app, &config.target)?)
                }
                _ => None,
            };
            settings::update(&app, |s| s.sync = None)?;
            state.credentials.lock().unwrap().clear();
            // 先停同步再删凭据：钥匙串拒绝访问时，用户也不能停不下来。
            match account.map(|account| SystemCredentials.delete(&account)) {
                Some(Err(_)) => Err(
                    "同步已停止，但没能删除钥匙串里保存的凭据，可以在系统的钥匙串或凭据管理器里手动删除"
                        .into(),
                ),
                _ => Ok(()),
            }
        })();
        configuration_changed(&app);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
/// 连接尝试的 id 由界面生成，因此请求尚未返回时也能取消。
#[tauri::command]
pub async fn inspect_network_target(
    app: AppHandle,
    connection_id: String,
    target: StorageConfig,
    credentials: Credentials,
) -> Result<TargetInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if app.state::<SettingsState>().get().sync.is_some() {
            return Err("更换地址前请先停止同步".into());
        }
        let target = target.normalized().map_err(|e| e.to_string())?;
        if !target.is_network() {
            return Err("请选择网络存储后端".into());
        }
        let state = app.state::<Arc<SyncState>>();
        let generation = {
            let mut runtime = state.inner.lock().unwrap();
            if runtime.cancelled_connection.as_deref() == Some(&connection_id) {
                return Err("连接已取消".into());
            }
            runtime.generation += 1;
            runtime.connection = Some(PendingConnection {
                id: connection_id.clone(),
                target: target.clone(),
                credentials: credentials.clone(),
            });
            runtime.generation
        };
        let _running = state.run.lock().unwrap();
        let result = (|| {
            generation_check(&state, generation)?;
            let storage =
                NetworkStorage::new(&target, &credentials, cancellation(&state, generation))
                    .map_err(|e| e.to_string())?;
            probe(&storage).map_err(|e| e.to_string())?;
            let info = core::inspect(&storage).map_err(|e| e.to_string())?;
            generation_check(&state, generation)?;
            Ok(info)
        })();
        if result.is_err() {
            clear_connection(&state, &connection_id);
        }
        result
    })
    .await
    .map_err(|_| "连接任务未完成".to_string())?
}
fn clear_connection(state: &SyncState, id: &str) {
    let mut runtime = state.inner.lock().unwrap();
    if runtime.connection.as_ref().is_some_and(|c| c.id == id) {
        runtime.connection = None;
    }
}
#[tauri::command]
pub fn cancel_sync_connection(app: AppHandle, connection_id: String) {
    let state = app.state::<Arc<SyncState>>();
    let mut runtime = state.inner.lock().unwrap();
    runtime.cancelled_connection = Some(connection_id.clone());
    if runtime
        .connection
        .as_ref()
        .is_some_and(|c| c.id == connection_id)
    {
        runtime.generation += 1;
        runtime.connection = None;
        runtime.schedule.requested = true;
        state.wake.notify_one();
    }
}
#[tauri::command]
pub async fn join_network_target(
    app: AppHandle,
    connection_id: String,
    expected_id: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Arc<SyncState>>();
        let (connection, generation) = {
            let mut runtime = state.inner.lock().unwrap();
            let connection = runtime
                .connection
                .clone()
                .filter(|c| c.id == connection_id)
                .ok_or("连接已取消，请重新连接")?;
            runtime.generation += 1;
            (connection, runtime.generation)
        };
        let _running = state.run.lock().unwrap();
        let result = (|| {
            generation_check(&state, generation)?;
            let storage = NetworkStorage::new(
                &connection.target,
                &connection.credentials,
                cancellation(&state, generation),
            )
            .map_err(|e| e.to_string())?;
            let info = core::inspect(&storage).map_err(|e| e.to_string())?;
            if info.manifest.as_ref().map(|m| &m.id) != expected_id.as_ref() {
                return Err("目标中的仓库已变化，请重新连接并确认".into());
            }
            let manifest = match info.manifest {
                Some(m) => m,
                None => Manifest::create(&storage).map_err(|e| e.to_string())?,
            };
            core::join(&app.state::<Mutex<Library>>(), &storage, &manifest.id)
                .map_err(|e| e.to_string())?;
            generation_check(&state, generation)?;
            let account = credential_account(&app, &connection.target)?;
            // 快照加入成功后才保存凭据；设置写入失败则撤销本次保存。
            SystemCredentials.save(&account, &connection.credentials)?;
            let save = {
                let mut runtime = state.inner.lock().unwrap();
                if runtime.generation != generation {
                    Err("连接已取消".into())
                } else {
                    let result = settings::update(&app, |s| {
                        s.sync = Some(SyncSettings {
                            target: connection.target,
                            library_id: manifest.id,
                        })
                    })
                    .map(|_| ());
                    if result.is_ok() {
                        runtime.connection = None;
                    }
                    result
                }
            };
            if let Err(error) = save {
                let cleanup = SystemCredentials.delete(&account);
                return Err(match cleanup {
                    Ok(()) => error,
                    Err(cleanup) => format!("{error}；{cleanup}"),
                });
            }
            state
                .credentials
                .lock()
                .unwrap()
                .remember(account, connection.credentials);
            Ok(())
        })();
        clear_connection(&state, &connection_id);
        if state.inner.lock().unwrap().generation == generation {
            configuration_changed(&app);
        }
        result
    })
    .await
    .map_err(|_| "连接任务未完成".to_string())?
}
#[tauri::command]
pub async fn update_sync_credentials(
    app: AppHandle,
    connection_id: String,
    credentials: Credentials,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = app
            .state::<SettingsState>()
            .get()
            .sync
            .ok_or("请先设置同步")?;
        if !config.target.is_network() {
            return Err("同步文件夹不需要密码".into());
        }
        let state = app.state::<Arc<SyncState>>();
        let generation = {
            let mut runtime = state.inner.lock().unwrap();
            if runtime.cancelled_connection.as_deref() == Some(&connection_id) {
                return Err("连接已取消".into());
            }
            runtime.generation += 1;
            runtime.connection = Some(PendingConnection {
                id: connection_id.clone(),
                target: config.target.clone(),
                credentials: credentials.clone(),
            });
            runtime.generation
        };
        let _running = state.run.lock().unwrap();
        let result = (|| {
            generation_check(&state, generation)?;
            let storage = NetworkStorage::new(
                &config.target,
                &credentials,
                cancellation(&state, generation),
            )
            .map_err(|e| e.to_string())?;
            probe(&storage).map_err(|e| e.to_string())?;
            let manifest = Manifest::read(&storage).map_err(|e| e.to_string())?;
            if manifest.id != config.library_id {
                return Err("目标中的仓库已变化，请先停止同步再重新加入".into());
            }
            generation_check(&state, generation)?;
            let account = credential_account(&app, &config.target)?;
            // 保存这一小段是提交点；取消不会在钥匙串已更新后把成功结果改写成失败。
            let mut runtime = state.inner.lock().unwrap();
            if runtime.generation != generation {
                return Err("连接已取消".into());
            }
            SystemCredentials.save(&account, &credentials)?;
            runtime.connection = None;
            drop(runtime);
            state
                .credentials
                .lock()
                .unwrap()
                .remember(account, credentials);
            Ok(())
        })();
        clear_connection(&state, &connection_id);
        if state.inner.lock().unwrap().generation == generation {
            configuration_changed(&app);
        }
        result
    })
    .await
    .map_err(|_| "凭据更新任务未完成".to_string())?
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
