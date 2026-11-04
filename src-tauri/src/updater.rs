//! 检查更新：读 GitHub Releases 上最新版本附带的 latest.json，用户确认后下载更新包、
//! 校验签名、替换应用并重启。签名用的是 Tauri 更新密钥，与 Apple 签名无关。
use serde::Serialize;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, Updater, UpdaterExt};

use crate::settings::{self, Settings, SettingsState};

/// 启动后等一会儿再检查，不和仓库维护、同步抢启动时间。
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(10);
/// 自动检查的间隔按墙上时间算；线程每小时醒一次，电脑睡眠醒来后也能及时补查。
const CHECK_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;
const TICK: Duration = Duration::from_secs(60 * 60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// reqwest 的超时管整个请求，下载更新包要留足时间。
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// 开发构建里设置了这个环境变量，就改从它指向的地址检查更新并打开自动检查，便于本地验证。
#[cfg(debug_assertions)]
const ENDPOINT_OVERRIDE: &str = "STICKER_UPDATE_ENDPOINT";

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Idle,
    Checking,
    Downloading,
    Installing,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    /// 发布时间，毫秒时间戳。
    pub date: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub phase: Phase,
    pub downloaded: u64,
    pub total: Option<u64>,
    /// 最近一次成功检查的时间（毫秒），自动和手动检查都算。
    pub last_checked: Option<i64>,
    pub available: Option<UpdateInfo>,
    /// 可用版本被用户忽略过：提示条和托盘不提示，偏好设置里照常显示。
    pub ignored: bool,
    /// 只记手动检查和安装的错误；自动检查失败只写日志。
    pub error: Option<String>,
}

pub struct UpdateState {
    status: Mutex<UpdateStatus>,
    update: Mutex<Option<Update>>,
    open_pending: AtomicBool,
}
impl UpdateState {
    pub fn new(current_version: String) -> Self {
        Self {
            status: Mutex::new(UpdateStatus {
                current_version,
                phase: Phase::Idle,
                downloaded: 0,
                total: None,
                last_checked: None,
                available: None,
                ignored: false,
                error: None,
            }),
            update: Mutex::new(None),
            open_pending: AtomicBool::new(false),
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// 时钟被往回调过（上次检查时间在将来）也重新检查。
fn auto_check_due(enabled: bool, last_checked: Option<i64>, now: i64) -> bool {
    enabled && last_checked.is_none_or(|last| now < last || now - last >= CHECK_INTERVAL_MS)
}

fn info(update: &Update) -> UpdateInfo {
    UpdateInfo {
        version: update.version.clone(),
        notes: update
            .body
            .as_deref()
            .map(str::trim)
            .filter(|notes| !notes.is_empty())
            .map(str::to_owned),
        date: update
            .date
            .map(|date| (date.unix_timestamp_nanos() / 1_000_000) as i64),
    }
}

fn describe(error: tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error;
    match error {
        Error::ReleaseNotFound => "没有找到更新信息".into(),
        Error::Reqwest(error) if error.is_timeout() => "连接超时".into(),
        Error::Reqwest(error) if error.is_connect() => "无法连接更新服务器".into(),
        Error::Minisign(error) => format!("更新包签名校验失败：{error}"),
        Error::SignedVersionMismatch { signed, announced } => {
            format!("更新包签名对应的是 {signed}，与更新信息里的 {announced} 不符，已拒绝安装")
        }
        Error::MissingSignedVersion => "更新包签名里没有版本号，已拒绝安装".into(),
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) => {
            "新版本没有适用于这台电脑的安装包".into()
        }
        Error::Io(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            "没有权限替换应用，或取消了管理员授权".into()
        }
        error => error.to_string(),
    }
}

fn status_with_settings(status: UpdateStatus, settings: &Settings) -> UpdateStatus {
    let ignored = status
        .available
        .as_ref()
        .is_some_and(|update| settings.ignored_update.as_deref() == Some(update.version.as_str()));
    UpdateStatus { ignored, ..status }
}

pub fn status(app: &AppHandle) -> UpdateStatus {
    let status = app.state::<UpdateState>().status.lock().unwrap().clone();
    status_with_settings(status, &app.state::<SettingsState>().get())
}

/// 状态变了：通知主窗口，并按是否该提示更新托盘菜单。
fn changed(app: &AppHandle) {
    let status = status(app);
    let notify = status
        .available
        .as_ref()
        .filter(|_| !status.ignored)
        .map(|update| update.version.clone());
    crate::tray::update_menu_changed(app, notify);
    if let Err(error) = app.emit("update-status", &status) {
        eprintln!("通知更新状态失败：{error}");
    }
}

fn modify(app: &AppHandle, change: impl FnOnce(&mut UpdateStatus)) {
    change(&mut app.state::<UpdateState>().status.lock().unwrap());
    changed(app);
}

fn updater(app: &AppHandle) -> Result<Updater, String> {
    let builder = app.updater_builder().timeout(CHECK_TIMEOUT);
    #[cfg(debug_assertions)]
    let builder = match std::env::var(ENDPOINT_OVERRIDE) {
        Ok(url) if !url.is_empty() => {
            let url = url
                .parse()
                .map_err(|e| format!("{ENDPOINT_OVERRIDE} 不是有效地址：{e}"))?;
            builder.endpoints(vec![url]).map_err(describe)?
        }
        _ => builder,
    };
    builder.build().map_err(describe)
}

/// 正在检查、下载或安装时直接返回，不重复发起。
async fn check(app: &AppHandle, manual: bool) -> Result<(), String> {
    {
        let state = app.state::<UpdateState>();
        let mut status = state.status.lock().unwrap();
        if status.phase != Phase::Idle {
            return Ok(());
        }
        status.phase = Phase::Checking;
        if manual {
            status.error = None;
        }
    }
    changed(app);
    let result = match updater(app) {
        Ok(updater) => updater.check().await.map_err(describe),
        Err(error) => Err(error),
    };
    {
        let state = app.state::<UpdateState>();
        let mut status = state.status.lock().unwrap();
        status.phase = Phase::Idle;
        match &result {
            Ok(update) => {
                status.last_checked = Some(now_ms());
                status.available = update.as_ref().map(info);
                status.error = None;
                *state.update.lock().unwrap() = update.clone();
            }
            Err(error) if manual => status.error = Some(format!("检查更新失败：{error}")),
            Err(_) => {}
        }
    }
    changed(app);
    result.map(|_| ())
}

async fn install(app: &AppHandle) -> Result<(), String> {
    let update = {
        let state = app.state::<UpdateState>();
        let mut status = state.status.lock().unwrap();
        if status.phase != Phase::Idle {
            return Err("正在检查或安装更新".into());
        }
        let update = state
            .update
            .lock()
            .unwrap()
            .clone()
            .ok_or("没有可安装的更新，请先检查更新")?;
        status.phase = Phase::Downloading;
        status.downloaded = 0;
        status.total = None;
        status.error = None;
        update
    };
    changed(app);
    let result = download_and_install(app, update).await;
    if let Err(error) = &result {
        modify(app, |status| {
            status.phase = Phase::Idle;
            status.error = Some(format!("安装更新失败：{error}"));
        });
    }
    result
}

async fn download_and_install(app: &AppHandle, mut update: Update) -> Result<(), String> {
    // 开发构建不在 .app 里，插件会把 target/debug 当成应用目录替换掉，所以只下载和校验。
    #[cfg(all(target_os = "macos", not(debug_assertions)))]
    if !crate::autostart::running_from_installed_bundle()? {
        return Err("请先把表情管理器拖进“应用程序”文件夹，从那里打开后再安装更新".into());
    }
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    let mut downloaded = 0u64;
    let mut reported = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                // 大约每 1% 通知一次界面，不为每个网络分块发事件。
                let step = total.map_or(256 * 1024, |total| (total / 100).max(64 * 1024));
                if downloaded - reported >= step || total == Some(downloaded) {
                    reported = downloaded;
                    modify(app, |status| {
                        status.downloaded = downloaded;
                        status.total = total;
                    });
                }
            },
            || {},
        )
        .await
        .map_err(describe)?;
    if cfg!(debug_assertions) {
        return Err("开发版本不安装更新（下载和签名校验已通过）".into());
    }
    modify(app, |status| status.phase = Phase::Installing);
    // 解压和替换应用是阻塞的文件操作；需要管理员授权时插件会在主线程弹系统授权框并等待结果，
    // 所以不能在主线程上调用。
    tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|e| e.to_string())?
        .map_err(describe)?;
    restart_without_autostart(app)
}

/// 与 AppHandle::restart 在主线程上的做法相同，只是去掉 --autostart：开机自启的进程更新后
/// 也要弹出主窗口，用户是在主窗口里点的安装，窗口消失会像是崩溃了。
fn restart_without_autostart(app: &AppHandle) -> Result<(), String> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let mut env = handle.env();
        env.args_os.retain(|arg| arg != "--autostart");
        handle.cleanup_before_exit();
        tauri::process::restart(&env);
    })
    .map_err(|e| format!("已安装新版本，但无法重新启动，请手动打开：{e}"))
}

pub fn start(app: AppHandle) {
    if crate::portable::enabled(&app) {
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var_os(ENDPOINT_OVERRIDE).is_none_or(|url| url.is_empty()) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        loop {
            let enabled = app.state::<SettingsState>().get().auto_check_updates;
            let last_checked = app
                .state::<UpdateState>()
                .status
                .lock()
                .unwrap()
                .last_checked;
            if auto_check_due(enabled, last_checked, now_ms())
                && let Err(error) = tauri::async_runtime::block_on(check(&app, false))
            {
                eprintln!("自动检查更新失败：{error}");
            }
            std::thread::sleep(TICK);
        }
    });
}

/// 托盘的“更新到 x.y.z…”：打开主窗口并显示更新说明。
pub fn open_dialog(app: &AppHandle) -> Result<(), String> {
    crate::panel::show_main_window(app.clone())?;
    app.state::<UpdateState>()
        .open_pending
        .store(true, Ordering::Relaxed);
    app.emit_to("main", "open-update", ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_status(app: AppHandle) -> UpdateStatus {
    status(&app)
}
/// 检查失败时错误记在返回的状态里。
#[tauri::command]
pub async fn check_update(app: AppHandle) -> UpdateStatus {
    if crate::portable::enabled(&app) {
        modify(&app, |status| {
            status.error = Some("便携版请下载新版 ZIP，退出后替换程序，保留 data 文件夹".into())
        });
        return status(&app);
    }
    let _ = check(&app, true).await;
    status(&app)
}
/// 成功时应用会重启，不会返回。
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    if crate::portable::enabled(&app) {
        return Err("便携版请退出后手动替换程序，保留 data 文件夹".into());
    }
    install(&app).await
}
#[tauri::command]
pub fn ignore_update(app: AppHandle, version: String) -> Result<(), String> {
    settings::update(&app, |s| s.ignored_update = Some(version))?;
    changed(&app);
    Ok(())
}
#[tauri::command]
pub fn set_auto_check_updates(app: AppHandle, enabled: bool) -> Result<Settings, String> {
    settings::update(&app, |s| s.auto_check_updates = enabled)
}
// 监听后再取 pending，覆盖主窗口尚未完成加载时从托盘打开的情况。
#[tauri::command]
pub fn take_update_request(app: AppHandle) -> bool {
    app.state::<UpdateState>()
        .open_pending
        .swap(false, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_check_runs_when_enabled_and_due() {
        let day = CHECK_INTERVAL_MS;
        assert!(auto_check_due(true, None, 1_000));
        assert!(!auto_check_due(false, None, 1_000));
        assert!(!auto_check_due(true, Some(1_000), 1_000 + day - 1));
        assert!(auto_check_due(true, Some(1_000), 1_000 + day));
        assert!(!auto_check_due(false, Some(1_000), 1_000 + day));
        // 时钟往回调了
        assert!(auto_check_due(true, Some(1_000 + day), 1_000));
    }

    #[test]
    fn only_the_ignored_version_is_marked_ignored() {
        let status = UpdateState::new("0.1.1".into())
            .status
            .into_inner()
            .unwrap();
        let available = UpdateStatus {
            available: Some(UpdateInfo {
                version: "0.1.2".into(),
                notes: None,
                date: None,
            }),
            ..status.clone()
        };
        let mut settings = Settings::default();
        assert!(!status_with_settings(available.clone(), &settings).ignored);
        settings.ignored_update = Some("0.1.2".into());
        assert!(status_with_settings(available.clone(), &settings).ignored);
        assert!(!status_with_settings(status, &settings).ignored);
        settings.ignored_update = Some("0.1.3".into());
        assert!(!status_with_settings(available, &settings).ignored);
    }

    #[test]
    fn settings_without_update_fields_default_to_auto_check() {
        let settings: Settings = serde_json::from_str(r#"{"version":1}"#).unwrap();
        assert!(settings.auto_check_updates);
        assert_eq!(settings.ignored_update, None);
    }
}
