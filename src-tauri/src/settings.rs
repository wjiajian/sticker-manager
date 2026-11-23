//! 本机偏好；系统自启登记是唯一的自启开关来源。
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
use sticker_core::sync::network::StorageConfig;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Left,
    Right,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct BallPosition {
    pub display: String,
    pub edge: Edge,
    pub y: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: u32,
    pub appearance: crate::appearance::Appearance,
    pub ball_visible: bool,
    pub ball_auto_hide_seconds: Option<u32>,
    /// 悬浮球逻辑直径（px），范围 40–80，步进 4，默认 56。
    #[serde(deserialize_with = "deserialize_ball_size")]
    pub ball_size: u32,
    pub ball_position: BallPosition,
    pub autostart_initialized: bool,
    pub sync: Option<SyncSettings>,
    pub auto_check_updates: bool,
    /// 用户在提示条上选了“忽略这个版本”的版本号；自动检查找到它时不再提示。
    pub ignored_update: Option<String>,
    /// 主窗口表情格的最小边长（px），窗口宽度决定实际列数。
    pub sticker_tile_size: u32,
    /// None = 已停用；字段缺失 = 默认值。反序列化容错，不加入 validate。
    #[serde(deserialize_with = "deserialize_panel_shortcut")]
    pub panel_shortcut: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    #[serde(flatten)]
    pub target: StorageConfig,
    pub library_id: String,
    #[serde(default)]
    pub paused: bool,
    #[serde(default = "enabled")]
    pub pause_on_metered: bool,
}
fn enabled() -> bool {
    true
}
/// 宽容反序列化：遇到负数、超大数、非数字或非步进值时仅规范化/回退本字段，不让整份设置失效。
fn deserialize_ball_size<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value
        .as_u64()
        .map(|n| crate::geometry::normalize_ball_size(u32::try_from(n).unwrap_or(u32::MAX)))
        .unwrap_or(crate::geometry::BALL_SIZE_DEFAULT))
}
fn deserialize_panel_shortcut<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(None);
    }
    match value
        .as_str()
        .ok_or_else(|| "快捷键必须是字符串或 null".to_string())
        .and_then(crate::shortcut::normalize_stored)
    {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            eprintln!("无法读取呼出快捷键，使用默认值：{error}");
            Ok(Some(crate::shortcut::DEFAULT.into()))
        }
    }
}
impl SyncSettings {
    pub fn new(target: StorageConfig, library_id: String) -> Self {
        Self {
            target,
            library_id,
            paused: false,
            pause_on_metered: true,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            appearance: Default::default(),
            ball_visible: true,
            ball_auto_hide_seconds: Some(3),
            ball_size: crate::geometry::BALL_SIZE_DEFAULT,
            ball_position: BallPosition {
                display: String::new(),
                edge: Edge::Right,
                y: 0.4,
            },
            autostart_initialized: false,
            sync: None,
            auto_check_updates: true,
            ignored_update: None,
            sticker_tile_size: 104,
            panel_shortcut: Some(crate::shortcut::DEFAULT.into()),
        }
    }
}
impl Settings {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!("不支持的设置版本 {}", self.version));
        }
        if let Some(sync) = &self.sync {
            sync.target.normalized().map_err(|e| e.to_string())?;
            if sync.library_id.is_empty() {
                return Err("同步仓库 ID 无效".into());
            }
        }
        if !matches!(self.ball_auto_hide_seconds, None | Some(3 | 10)) {
            return Err("收起时间必须是 3 秒、10 秒或不收起".into());
        }
        if !self.ball_position.y.is_finite() || !(0.0..=1.0).contains(&self.ball_position.y) {
            return Err("悬浮球高度必须在 0 到 1 之间".into());
        }
        Ok(())
    }
    pub fn load(root: &Path) -> Self {
        let path = root.join("local/settings.json");
        let result = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Self>(&bytes)
                .map_err(|e| e.to_string())
                .and_then(|s| {
                    s.validate()?;
                    Ok(s)
                }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(e) => Err(e.to_string()),
        };
        result.unwrap_or_else(|error| {
            eprintln!("无法读取本机设置 {}，使用默认值：{error}", path.display());
            Self::default()
        })
    }
    pub fn save(&self, root: &Path) -> Result<(), String> {
        self.validate()?;
        let directory = root.join("local");
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let mut file = tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|()| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        file.persist(directory.join("settings.json"))
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub struct SettingsState {
    pub root: PathBuf,
    value: Mutex<Settings>,
}
impl SettingsState {
    pub fn new(root: PathBuf) -> Self {
        let value = Mutex::new(Settings::load(&root));
        Self { root, value }
    }
    pub fn get(&self) -> Settings {
        self.value.lock().unwrap().clone()
    }
}

pub fn update(app: &AppHandle, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
    update_inner(app, change, true)
}

/// 快捷键状态机持锁期间只保存，调用者解锁后广播 settings-changed。
pub(crate) fn update_without_event(
    app: &AppHandle,
    change: impl FnOnce(&mut Settings),
) -> Result<Settings, String> {
    update_inner(app, change, false)
}

fn update_inner(
    app: &AppHandle,
    change: impl FnOnce(&mut Settings),
    emit_event: bool,
) -> Result<Settings, String> {
    let state = app.state::<SettingsState>();
    let mut current = state.value.lock().unwrap();
    let mut next = current.clone();
    change(&mut next);
    let ball_changed = current.ball_visible != next.ball_visible
        || current.ball_auto_hide_seconds != next.ball_auto_hide_seconds
        || current.ball_size != next.ball_size
        || current.ball_position != next.ball_position;
    let visibility_changed = current.ball_visible != next.ball_visible;
    let appearance_changed = current.appearance != next.appearance;
    next.save(&state.root)?;
    *current = next.clone();
    drop(current);
    // 三处入口共用这个路径。即使某个系统操作失败，其余订阅者仍收到已保存的实际值。
    // 同步设置由后台保存，不应触碰只能在主线程操作的 NSPanel。
    let latest = state.get();
    let ball = if ball_changed {
        crate::ball::settings_changed(app, &latest)
    } else {
        Ok(())
    };
    let tray = if visibility_changed {
        crate::tray::set_ball_checked(app, latest.ball_visible)
    } else {
        Ok(())
    };
    let event = if emit_event {
        app.emit("settings-changed", &latest)
            .map_err(|e| e.to_string())
    } else {
        Ok(())
    };
    let appearance = if appearance_changed {
        crate::appearance::apply(app, latest.appearance).map_err(|e| e.to_string())
    } else {
        Ok(())
    };
    ball.and(tray).and(appearance).and(event)?;
    Ok(latest)
}

#[tauri::command]
pub fn get_settings(state: tauri::State<SettingsState>) -> Settings {
    state.get()
}
#[tauri::command]
pub fn set_appearance(
    app: AppHandle,
    appearance: crate::appearance::Appearance,
) -> Result<Settings, String> {
    update(&app, |s| s.appearance = appearance)
}
#[tauri::command]
pub fn set_ball_visible(app: AppHandle, visible: bool) -> Result<Settings, String> {
    update(&app, |s| s.ball_visible = visible)
}
#[tauri::command]
pub fn set_ball_auto_hide(app: AppHandle, seconds: Option<u32>) -> Result<Settings, String> {
    update(&app, |s| s.ball_auto_hide_seconds = seconds)
}
/// 不放进 `validate`：反序列化时已容错并规范化，不该让整份设置（含同步配置）回到默认值。
#[tauri::command]
pub fn set_ball_size(app: AppHandle, size: u32) -> Result<Settings, String> {
    let size = crate::geometry::validate_ball_size(size)?;
    update(&app, |s| s.ball_size = size)
}
/// 不放进 `validate`：读到范围外的值时界面夹回范围内就够了，不该让整份设置（含同步配置）回到默认值。
#[tauri::command]
pub fn set_sticker_tile_size(app: AppHandle, size: u32) -> Result<Settings, String> {
    if !(80..=200).contains(&size) {
        return Err("表情大小必须在 80 到 200 之间".into());
    }
    update(&app, |s| s.sticker_tile_size = size)
}
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool, String> {
    if crate::portable::enabled(&app) {
        return Ok(false);
    }
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}
#[derive(Serialize)]
pub struct AutostartResult {
    enabled: Option<bool>,
    error: Option<String>,
}
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> AutostartResult {
    if crate::portable::enabled(&app) {
        return AutostartResult {
            enabled: Some(false),
            error: enabled.then(|| "便携版不登记开机自启".into()),
        };
    }
    let result = if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    };
    let actual = app.autolaunch().is_enabled();
    let error = result
        .err()
        .map(|e| e.to_string())
        .or_else(|| actual.as_ref().err().map(|e| e.to_string()));
    AutostartResult {
        enabled: actual.ok(),
        error,
    }
}
#[tauri::command]
pub fn library_directory(state: tauri::State<SettingsState>) -> String {
    state.root.to_string_lossy().into_owned()
}
#[tauri::command]
pub fn open_library_directory(app: AppHandle) -> Result<(), String> {
    app.opener()
        .open_path(
            app.state::<SettingsState>().root.to_string_lossy(),
            None::<&str>,
        )
        .map_err(|e| e.to_string())
}

pub fn initialize_autostart(app: &AppHandle) {
    if crate::portable::enabled(app) {
        return;
    }
    #[cfg(target_os = "macos")]
    if let Err(error) = crate::autostart::initialize(app) {
        eprintln!("初始化自启失败：{error}");
    }
    #[cfg(all(not(debug_assertions), not(target_os = "macos")))]
    if !app.state::<SettingsState>().get().autostart_initialized {
        let result = app
            .autolaunch()
            .enable()
            .map_err(|e| e.to_string())
            .and_then(|()| update(app, |s| s.autostart_initialized = true).map(|_| ()));
        if let Err(error) = result {
            eprintln!("默认开启自启失败：{error}");
        }
    }
    #[cfg(all(debug_assertions, not(target_os = "macos")))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panel_shortcut_missing_null_normalized_and_invalid() {
        assert_eq!(
            serde_json::from_str::<Settings>("{}")
                .unwrap()
                .panel_shortcut
                .as_deref(),
            Some(crate::shortcut::DEFAULT)
        );
        let root = tempfile::tempdir().unwrap();
        let shared = serde_json::to_string(&root.path().join("shared")).unwrap();
        fs::create_dir_all(root.path().join("local")).unwrap();
        for (raw, expected) in [
            ("null", None),
            (r#""alt+ctrl+k""#, Some("Ctrl+Alt+KeyK")),
            (r#""invalid""#, Some(crate::shortcut::DEFAULT)),
            ("42", Some(crate::shortcut::DEFAULT)),
            ("{}", Some(crate::shortcut::DEFAULT)),
            ("false", Some(crate::shortcut::DEFAULT)),
            (r#""KeyE""#, Some(crate::shortcut::DEFAULT)),
        ] {
            fs::write(root.path().join("local/settings.json"),format!(r#"{{"panelShortcut":{raw},"ballVisible":false,"sync":{{"backend":"folder","folder":{shared},"libraryId":"id"}}}}"#)).unwrap();
            let settings = Settings::load(root.path());
            assert_eq!(settings.panel_shortcut.as_deref(), expected, "{raw}");
            assert!(!settings.ball_visible);
            assert!(settings.sync.is_some());
            settings.save(root.path()).unwrap();
            assert_eq!(Settings::load(root.path()), settings);
            assert_eq!(
                serde_json::to_value(&settings).unwrap()["panelShortcut"],
                serde_json::to_value(expected).unwrap()
            );
        }
    }
    #[test]
    fn settings_saved_before_tile_size_existed_keep_their_values() {
        let settings: Settings = serde_json::from_str(r#"{"ballVisible":false}"#).unwrap();
        assert!(!settings.ball_visible);
        assert_eq!(settings.sticker_tile_size, 104);
        assert_eq!(settings.ball_size, crate::geometry::BALL_SIZE_DEFAULT);
    }
    #[test]
    fn ball_size_round_trip_and_invalid_values_preserve_other_settings() {
        let root = tempfile::tempdir().unwrap();
        let settings = Settings {
            ball_size: 72,
            ..Settings::default()
        };
        settings.save(root.path()).unwrap();
        assert_eq!(Settings::load(root.path()).ball_size, 72);

        let shared = serde_json::to_string(&root.path().join("shared")).unwrap();
        for (raw, expected) in [
            ("200", crate::geometry::BALL_SIZE_MAX),
            ("55", 56),
            ("-10", crate::geometry::BALL_SIZE_DEFAULT),
            ("999999999999999", crate::geometry::BALL_SIZE_MAX),
            (r#""invalid""#, crate::geometry::BALL_SIZE_DEFAULT),
        ] {
            fs::write(
                root.path().join("local/settings.json"),
                format!(
                    r#"{{"ballVisible":false,"ballSize":{raw},"sync":{{"backend":"folder","folder":{shared},"libraryId":"id"}}}}"#
                ),
            )
            .unwrap();
            let loaded = Settings::load(root.path());
            assert!(!loaded.ball_visible, "raw={raw}");
            assert_eq!(loaded.ball_size, expected, "raw={raw}");
            assert!(loaded.sync.is_some(), "raw={raw}");
        }
    }
    #[test]
    fn sync_settings_round_trip_and_corrupt_backend_falls_back() {
        let root = tempfile::tempdir().unwrap();
        let settings = Settings {
            sync: Some(SyncSettings {
                target: StorageConfig::Folder {
                    folder: root.path().join("shared"),
                },
                library_id: "library-id".into(),
                paused: true,
                pause_on_metered: false,
            }),
            ..Settings::default()
        };
        settings.save(root.path()).unwrap();
        assert_eq!(Settings::load(root.path()), settings);
        fs::write(
            root.path().join("local/settings.json"),
            r#"{"sync":{"backend":"unknown","folder":"/tmp","libraryId":"id"}}"#,
        )
        .unwrap();
        assert_eq!(Settings::load(root.path()), Settings::default());
    }
    #[test]
    fn legacy_folder_json_and_network_settings_contain_no_credentials() {
        let old = r#"{"backend":"folder","folder":"/tmp/shared","libraryId":"id"}"#;
        let config: SyncSettings = serde_json::from_str(old).unwrap();
        assert_eq!(
            config.target,
            StorageConfig::Folder {
                folder: "/tmp/shared".into()
            }
        );
        assert_eq!(config.library_id, "id");
        assert!(!config.paused);
        assert!(config.pause_on_metered);
        let root = tempfile::tempdir().unwrap();
        let settings = Settings {
            sync: Some(SyncSettings {
                target: StorageConfig::R2 {
                    account_id: "0123456789abcdef0123456789abcdef".into(),
                    bucket: "stickers".into(),
                    path: "one".into(),
                },
                library_id: "library-id".into(),
                paused: true,
                pause_on_metered: false,
            }),
            ..Settings::default()
        };
        settings.save(root.path()).unwrap();
        assert_eq!(Settings::load(root.path()), settings);
        let bytes = fs::read_to_string(root.path().join("local/settings.json")).unwrap();
        for forbidden in ["password", "secret", "accessKey", "username"] {
            assert!(!bytes.contains(forbidden));
        }
    }
    #[test]
    fn missing_file_uses_defaults_without_writing() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(root.path()), Settings::default());
        assert!(!root.path().join("local/settings.json").exists());
    }
    #[test]
    fn corrupt_file_is_recovered_on_next_save() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("local")).unwrap();
        fs::write(root.path().join("local/settings.json"), "乱码{").unwrap();
        let settings = Settings::load(root.path());
        assert_eq!(settings, Settings::default());
        settings.save(root.path()).unwrap();
        assert_eq!(Settings::load(root.path()), settings);
    }
    #[test]
    fn round_trip_and_atomic_replacement_leave_no_temp_file() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = Settings {
            appearance: crate::appearance::Appearance::Dark,
            ball_visible: false,
            ball_auto_hide_seconds: None,
            ..Settings::default()
        };
        settings.ball_position = BallPosition {
            display: "display-2".into(),
            edge: Edge::Left,
            y: 0.72,
        };
        settings.save(root.path()).unwrap();
        settings.ball_auto_hide_seconds = Some(10);
        settings.save(root.path()).unwrap();
        assert_eq!(Settings::load(root.path()), settings);
        assert_eq!(fs::read_dir(root.path().join("local")).unwrap().count(), 1);
        let json = fs::read_to_string(root.path().join("local/settings.json")).unwrap();
        assert!(json.contains("ballVisible"));
        assert!(!json.contains("autostartEnabled"));
    }
    #[test]
    fn invalid_values_fall_back_and_cannot_be_saved() {
        let root = tempfile::tempdir().unwrap();
        let settings = Settings {
            ball_auto_hide_seconds: Some(5),
            ..Settings::default()
        };
        assert!(settings.save(root.path()).is_err());
        fs::create_dir_all(root.path().join("local")).unwrap();
        fs::write(root.path().join("local/settings.json"), r#"{"version":9}"#).unwrap();
        assert_eq!(Settings::load(root.path()), Settings::default());
    }
}
