//! 单后台工作线程；命令只排队，文件后端从不在主线程执行。
use crate::credentials::{self, CredentialCache, CredentialStore, FileCredentials};
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
    pub pause_reason: Option<PauseReason>,
    pub manual_paused: bool,
    pub pause_on_metered: bool,
    pub devices: Vec<RemoteDevice>,
    pub device_id: String,
    pub unseen: usize,
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
        // 正在读凭据时不等它，按已保存算。
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
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    #[default]
    Unconfigured,
    Syncing,
    Synced,
    Paused,
    Error,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PauseReason {
    Manual,
    Metered,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct PausePolicy {
    manual: bool,
    metered: bool,
}
impl PausePolicy {
    fn reason(self) -> Option<PauseReason> {
        if self.manual {
            Some(PauseReason::Manual)
        } else if self.metered {
            Some(PauseReason::Metered)
        } else {
            None
        }
    }
    fn compaction_allowed(self, network: bool, metered: bool) -> bool {
        !self.manual && (!network || !metered)
    }
    fn for_config(config: Option<&SyncSettings>, metered: bool) -> Self {
        config
            .map(|s| Self {
                manual: s.paused,
                metered: s.target.is_network() && s.pause_on_metered && metered,
            })
            .unwrap_or_default()
    }
}
#[derive(Default)]
struct Unseen {
    count: usize,
    main_active: bool,
    panel_visible: bool,
}
impl Unseen {
    fn arrive(&mut self, count: usize) {
        if !self.main_active && !self.panel_visible {
            self.count = self.count.saturating_add(count);
        }
    }
    fn visibility(&mut self, main: Option<bool>, panel: Option<bool>) {
        if let Some(v) = main {
            self.main_active = v;
        }
        if let Some(v) = panel {
            self.panel_visible = v;
        }
        if self.main_active || self.panel_visible {
            self.count = 0;
        }
    }
}
struct Runtime {
    generation: u64,
    policy: PausePolicy,
    metered: bool,
    unseen: Unseen,
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
    manual: bool,
    requested: bool,
    dirty: Option<Instant>,
    periodic: Instant,
    last_wall: SystemTime,
}
impl Schedule {
    fn new(now: Instant, wall: SystemTime) -> Self {
        Self {
            manual: false,
            requested: true,
            dirty: None,
            periodic: now,
            last_wall: wall,
        }
    }
    fn local_change(&mut self, now: Instant) {
        self.dirty = Some(now + Duration::from_secs(10));
    }
    fn take_allowed(&mut self, now: Instant, wall: SystemTime, policy: PausePolicy) -> bool {
        let manual = std::mem::take(&mut self.manual);
        let due = self.take_due(now, wall);
        !policy.manual && (manual || (due && !policy.metered))
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
impl Runtime {
    fn policy_changed(&mut self, policy: PausePolicy) {
        if self.policy == policy {
            return;
        }
        self.generation += 1;
        self.policy = policy;
        self.status.progress = None;
        self.status.pause_reason = policy.reason();
        self.status.manual_paused = policy.manual;
        if policy.reason().is_some() {
            self.status.phase = Phase::Paused;
        } else {
            self.schedule.requested = true;
        }
    }
}
pub fn start(app: AppHandle) {
    let config = app.state::<SettingsState>().get().sync;
    let (devices, last_sync_at, device_id, device_name) = {
        let library = app.state::<Mutex<Library>>();
        let library = library.lock().unwrap();
        let (devices, time) = config
            .as_ref()
            .and_then(|s| library.cached_sync_devices(&s.library_id).ok())
            .unwrap_or_default();
        (
            devices,
            time,
            library.device().id.clone(),
            library.device().name.clone(),
        )
    };
    let metered = crate::metered::forced();
    let policy = PausePolicy::for_config(config.as_ref(), metered);
    app.manage(Arc::new(SyncState {
        inner: Mutex::new(Runtime {
            generation: 0,
            policy,
            metered,
            unseen: Unseen {
                main_active: app
                    .get_webview_window("main")
                    .is_some_and(|w| w.is_focused().unwrap_or(false)),
                ..Unseen::default()
            },
            schedule: Schedule::new(Instant::now(), SystemTime::now()),
            status: SyncStatus {
                phase: if policy.reason().is_some() {
                    Phase::Paused
                } else if config.is_some() {
                    Phase::Syncing
                } else {
                    Phase::Unconfigured
                },
                pause_reason: policy.reason(),
                manual_paused: policy.manual,
                pause_on_metered: config.as_ref().is_none_or(|s| s.pause_on_metered),
                devices: devices.clone(),
                last_sync_at,
                device_id,
                device_name,
                ..SyncStatus::default()
            },
            devices,
            connection: None,
            cancelled_connection: None,
            progress_event: Instant::now(),
        }),
        wake: Condvar::new(),
        run: Mutex::new(()),
        credentials: Mutex::new(CredentialCache::default()),
    }));
    // 暂停启动时 worker 不会发布一轮结果；先补齐缓存状态，避免焦点事件
    // 把缺少 target 的初始快照覆盖到标题栏与左栏卡片。
    let state = app.state::<Arc<SyncState>>();
    let phase = state.inner.lock().unwrap().status.phase.clone();
    publish(&app, &state, 0, phase, None, None);
    crate::metered::start(app.clone());
    std::thread::spawn(move || {
        let state = app.state::<Arc<SyncState>>().inner().clone();
        loop {
            let generation = {
                let mut runtime = state.inner.lock().unwrap();
                loop {
                    let policy = runtime.policy;
                    if runtime
                        .schedule
                        .take_allowed(Instant::now(), SystemTime::now(), policy)
                    {
                        break;
                    }
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
                        core::synchronize_with_maintenance(
                            &app.state::<Mutex<Library>>(),
                            &storage,
                            &config.library_id,
                            &|progress| {
                                let mut runtime = state.inner.lock().unwrap();
                                if runtime.generation != generation {
                                    return;
                                }
                                let send = runtime.status.progress.is_none()
                                    || runtime.progress_event.elapsed()
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
                            &|count| {
                                let mut runtime = state.inner.lock().unwrap();
                                if runtime.generation != generation {
                                    return;
                                }
                                runtime.unseen.arrive(count);
                                runtime.status.unseen = runtime.unseen.count;
                                let status = runtime.status.clone();
                                drop(runtime);
                                let _ = app.emit("sync-status-changed", status);
                            },
                            &|| {
                                let runtime = state.inner.lock().unwrap();
                                runtime.generation == generation
                                    && runtime.policy.compaction_allowed(
                                        config.target.is_network(),
                                        runtime.metered,
                                    )
                            },
                        )
                    });
                // 没有改动本机表的轮次（比如每 5 分钟的例行拉取）不让各窗口重读全部列表。
                // 失败前可能已应用其他设备的一段操作，出错时照常通知。
                if result.as_ref().map_or(true, |report| report.changed) {
                    crate::commands::notify_library_changed(&app);
                }
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
                if let Err(error) = app
                    .state::<Mutex<Library>>()
                    .lock()
                    .unwrap()
                    .maintain_sync(None)
                {
                    eprintln!("清理未开启同步的本地日志失败：{error}");
                }
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
    let reason = runtime.policy.reason();
    runtime.status.phase = if phase != Phase::Syncing && reason.is_some() {
        Phase::Paused
    } else {
        phase
    };
    runtime.status.pause_reason = reason;
    runtime.status.manual_paused = runtime.policy.manual;
    runtime.status.pause_on_metered = config.as_ref().is_none_or(|s| s.pause_on_metered);
    runtime.status.devices = runtime.devices.clone();
    runtime.status.unseen = runtime.unseen.count;
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
    let config = app.state::<SettingsState>().get().sync;
    let (devices, last_sync_at) = {
        let library = app.state::<Mutex<Library>>();
        let library = library.lock().unwrap();
        config
            .as_ref()
            .and_then(|s| library.cached_sync_devices(&s.library_id).ok())
            .unwrap_or_default()
    };
    let mut runtime = state.inner.lock().unwrap();
    runtime.generation += 1;
    runtime.schedule.requested = true;
    let device_id = runtime.status.device_id.clone();
    runtime.status = SyncStatus {
        device_id,
        last_sync_at,
        ..SyncStatus::default()
    };
    runtime.devices = devices;
    runtime.policy = PausePolicy::for_config(config.as_ref(), runtime.metered);
    runtime.connection = None;
    let generation = runtime.generation;
    drop(runtime);
    state.wake.notify_one();
    let phase = if config.is_some() {
        Phase::Synced
    } else {
        Phase::Unconfigured
    };
    publish(app, &state, generation, phase, None, None);
    crate::tray::sync_menu_changed(app);
}

pub fn network_changed(app: &AppHandle, metered: bool) {
    let Some(state) = app.try_state::<Arc<SyncState>>() else {
        return;
    };
    let config = app.state::<SettingsState>().get().sync;
    let mut runtime = state.inner.lock().unwrap();
    runtime.metered = metered;
    let policy = PausePolicy::for_config(config.as_ref(), metered);
    if runtime.policy == policy {
        return;
    }
    runtime.policy_changed(policy);
    let generation = runtime.generation;
    drop(runtime);
    state.wake.notify_one();
    publish(
        app,
        &state,
        generation,
        if config.is_some() {
            Phase::Synced
        } else {
            Phase::Unconfigured
        },
        None,
        None,
    );
}

// 两个设置命令都会通过 publish 等仓库锁；同步可能正在写库，不能放在主线程。
#[tauri::command]
pub async fn set_sync_paused(app: AppHandle, paused: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || apply_paused(&app, paused))
        .await
        .map_err(|e| e.to_string())?
}
pub fn apply_paused(app: &AppHandle, paused: bool) -> Result<(), String> {
    let settings = settings::update(app, |s| {
        if let Some(sync) = &mut s.sync {
            sync.paused = paused;
        }
    })?;
    let state = app.state::<Arc<SyncState>>();
    let mut runtime = state.inner.lock().unwrap();
    let policy = PausePolicy::for_config(settings.sync.as_ref(), runtime.metered);
    runtime.policy_changed(policy);
    if !paused {
        runtime.schedule.manual = true;
    }
    let generation = runtime.generation;
    drop(runtime);
    state.wake.notify_one();
    publish(app, &state, generation, Phase::Synced, None, None);
    crate::tray::sync_menu_changed(app);
    Ok(())
}
#[tauri::command]
pub async fn set_pause_on_metered(app: AppHandle, enabled: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        settings::update(&app, |s| {
            if let Some(sync) = &mut s.sync {
                sync.pause_on_metered = enabled;
            }
        })?;
        let metered = app.state::<Arc<SyncState>>().inner.lock().unwrap().metered;
        network_changed(&app, metered);
        let state = app.state::<Arc<SyncState>>();
        let status = {
            let mut runtime = state.inner.lock().unwrap();
            runtime.status.pause_on_metered = enabled;
            runtime.status.clone()
        };
        let _ = app.emit("sync-status-changed", status);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn visibility_changed(app: &AppHandle, main: Option<bool>, panel: Option<bool>) {
    let Some(state) = app.try_state::<Arc<SyncState>>() else {
        return;
    };
    let mut runtime = state.inner.lock().unwrap();
    runtime.unseen.visibility(main, panel);
    runtime.status.unseen = runtime.unseen.count;
    let status = runtime.status.clone();
    drop(runtime);
    let _ = app.emit("sync-status-changed", status);
}

#[tauri::command]
pub async fn sticker_sync_info(
    app: AppHandle,
    id: String,
) -> Result<Option<core::StickerSyncInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(config) = app.state::<SettingsState>().get().sync else {
            return Ok(None);
        };
        let devices = app
            .state::<Arc<SyncState>>()
            .inner
            .lock()
            .unwrap()
            .devices
            .clone();
        let library = app.state::<Mutex<Library>>();
        library
            .lock()
            .map_err(|e| e.to_string())?
            .sticker_sync_info(&id, &config.library_id, &devices)
            .map(Some)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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
        .read(&FileCredentials::of(app), &account)
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
    fn validate_write(&self, key: &str) -> sticker_core::Result<()> {
        self.inner.validate_write(key)
    }
    fn list(&self, p: &str) -> sticker_core::Result<Vec<FileEntry>> {
        self.check()?;
        let result = self.inner.list(p)?;
        self.check()?;
        Ok(result)
    }
    fn read(&self, k: &str) -> sticker_core::Result<Option<Vec<u8>>> {
        self.check()?;
        let result = self.inner.read(k)?;
        self.check()?;
        Ok(result)
    }
    fn write(&self, k: &str, b: &[u8]) -> sticker_core::Result<()> {
        self.check()?;
        self.inner.write(k, b)?;
        self.check()
    }
    fn delete(&self, k: &str) -> sticker_core::Result<()> {
        self.check()?;
        self.inner.delete(k)?;
        self.check()
    }
}

#[tauri::command]
pub async fn sync_status(app: AppHandle) -> Result<SyncStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Arc<SyncState>>();
        let library = app.state::<Mutex<Library>>();
        let library = library.lock().map_err(|e| e.to_string())?;
        let device_name = library.device().name.clone();
        let storage_bytes = library.storage_bytes().map_err(|e| e.to_string())?;
        drop(library);
        let mut status = state.inner.lock().unwrap().status.clone();
        status.device_name = device_name;
        status.storage_bytes = storage_bytes;
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
    let state = app.state::<Arc<SyncState>>();
    state.inner.lock().unwrap().schedule.manual = true;
    state.wake.notify_one();
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
                s.sync = Some(SyncSettings::new(
                    StorageConfig::Folder { folder: info.path },
                    manifest.id,
                ))
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
            // 先停同步再删凭据：凭据文件删不掉时，用户也不能停不下来。
            match account.map(|account| FileCredentials::of(&app).delete(&account)) {
                Some(Err(_)) => Err(
                    "同步已停止，但没能删除本机保存的凭据，可以退出应用后删掉仓库文件夹里 local/credentials.json"
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
            FileCredentials::of(&app).save(&account, &connection.credentials)?;
            let save = {
                let mut runtime = state.inner.lock().unwrap();
                if runtime.generation != generation {
                    Err("连接已取消".into())
                } else {
                    let result = settings::update(&app, |s| {
                        s.sync = Some(SyncSettings::new(connection.target, manifest.id))
                    })
                    .map(|_| ());
                    if result.is_ok() {
                        runtime.connection = None;
                    }
                    result
                }
            };
            if let Err(error) = save {
                let cleanup = FileCredentials::of(&app).delete(&account);
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
            // 保存这一小段是提交点；取消不会在凭据已更新后把成功结果改写成失败。
            let mut runtime = state.inner.lock().unwrap();
            if runtime.generation != generation {
                return Err("连接已取消".into());
            }
            FileCredentials::of(&app).save(&account, &credentials)?;
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

#[cfg(test)]
mod pause_tests {
    use super::*;
    fn runtime() -> Runtime {
        Runtime {
            generation: 7,
            policy: PausePolicy::default(),
            metered: false,
            unseen: Unseen::default(),
            schedule: Schedule::new(Instant::now(), SystemTime::now()),
            status: SyncStatus {
                phase: Phase::Synced,
                last_sync_at: Some(42),
                ..SyncStatus::default()
            },
            devices: vec![RemoteDevice {
                id: "device".into(),
                name: "Mac".into(),
                last_sync_at: 42,
                platform: "macos".into(),
                received: Default::default(),
            }],
            connection: None,
            cancelled_connection: None,
            progress_event: Instant::now(),
        }
    }
    #[test]
    fn compaction_respects_metered_even_when_automatic_pause_is_disabled() {
        let policy = PausePolicy {
            manual: false,
            metered: false,
        };
        assert!(!policy.compaction_allowed(true, true));
        assert!(policy.compaction_allowed(false, true));
        assert!(policy.compaction_allowed(true, false));
        assert!(
            !PausePolicy {
                manual: true,
                metered: false
            }
            .compaction_allowed(false, false)
        );
    }
    #[test]
    fn manual_metered_overlap_and_manual_one_round() {
        let mut r = runtime();
        let now = Instant::now();
        let wall = SystemTime::now();
        r.policy_changed(PausePolicy {
            manual: true,
            metered: false,
        });
        assert!(!r.schedule.take_allowed(now, wall, r.policy)); // 启动
        r.schedule.local_change(now - Duration::from_secs(20));
        assert!(!r.schedule.take_allowed(
            now + Duration::from_secs(600),
            wall + Duration::from_secs(600),
            r.policy
        )); // 本地、定时和唤醒
        r.policy_changed(PausePolicy {
            manual: true,
            metered: true,
        });
        assert_eq!(r.policy.reason(), Some(PauseReason::Manual));
        r.policy_changed(PausePolicy {
            manual: false,
            metered: true,
        });
        assert!(!r.schedule.take_allowed(now, wall, r.policy));
        r.schedule.manual = true;
        assert!(r.schedule.take_allowed(now, wall, r.policy));
        assert!(!r.schedule.take_allowed(now, wall, r.policy));
        r.policy_changed(PausePolicy::default());
        assert!(r.schedule.take_allowed(now, wall, r.policy)); // 网络恢复立即跑
        assert!(!r.schedule.take_allowed(now, wall, r.policy));
        assert_eq!(r.status.last_sync_at, Some(42));
        assert_eq!(r.devices.len(), 1);
    }
    #[test]
    fn folder_ignores_cost_and_network_setting_defaults_to_pausing() {
        let folder = SyncSettings::new(
            StorageConfig::Folder {
                folder: "/tmp/shared".into(),
            },
            "id".into(),
        );
        assert_eq!(
            PausePolicy::for_config(Some(&folder), true),
            PausePolicy::default()
        );
        let mut network = SyncSettings::new(
            StorageConfig::Webdav {
                url: "https://example.com/".into(),
            },
            "id".into(),
        );
        assert_eq!(
            PausePolicy::for_config(Some(&network), true).reason(),
            Some(PauseReason::Metered)
        );
        network.pause_on_metered = false;
        assert_eq!(PausePolicy::for_config(Some(&network), true).reason(), None);
    }
    #[test]
    fn pausing_invalidates_in_flight_generation_without_clearing_history() {
        struct PausingStorage {
            state: Arc<SyncState>,
            calls: std::sync::atomic::AtomicUsize,
        }
        impl Storage for PausingStorage {
            fn list(&self, _: &str) -> sticker_core::Result<Vec<FileEntry>> {
                unreachable!()
            }
            fn read(&self, _: &str) -> sticker_core::Result<Option<Vec<u8>>> {
                self.calls
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.state
                    .inner
                    .lock()
                    .unwrap()
                    .policy_changed(PausePolicy {
                        manual: true,
                        metered: false,
                    });
                Ok(Some(vec![1]))
            }
            fn write(&self, _: &str, _: &[u8]) -> sticker_core::Result<()> {
                unreachable!()
            }
            fn delete(&self, _: &str) -> sticker_core::Result<()> {
                unreachable!()
            }
        }
        let state = Arc::new(SyncState {
            inner: Mutex::new(runtime()),
            wake: Condvar::new(),
            run: Mutex::new(()),
            credentials: Mutex::new(CredentialCache::default()),
        });
        let cancelled = cancellation(&state, 7);
        let storage = ActiveStorage {
            inner: Box::new(PausingStorage {
                state: state.clone(),
                calls: Default::default(),
            }),
            state: &state,
            generation: 7,
        };
        assert!(storage.read("test").is_err());
        assert!(cancelled());
        assert!(storage.read("second").is_err());
        let r = state.inner.lock().unwrap();
        assert_eq!(r.status.phase, Phase::Paused);
        assert_eq!(r.status.last_sync_at, Some(42));
        assert_eq!(r.devices.len(), 1);
    }
    #[test]
    fn unseen_is_memory_only_and_clears_at_every_visible_entry() {
        let mut unseen = Unseen::default();
        unseen.arrive(4);
        assert_eq!(unseen.count, 4);
        unseen.visibility(None, Some(true));
        assert_eq!(unseen.count, 0);
        unseen.arrive(3);
        assert_eq!(unseen.count, 0);
        unseen.visibility(None, Some(false));
        unseen.arrive(6);
        assert_eq!(unseen.count, 6);
        unseen.visibility(Some(true), None);
        unseen.arrive(2);
        assert_eq!(unseen.count, 0);
        unseen.visibility(Some(false), None);
        unseen.arrive(1);
        assert_eq!(unseen.count, 1);
        assert_eq!(Unseen::default().count, 0);
    }
}
