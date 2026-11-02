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
    pub ball_position: BallPosition,
    pub autostart_initialized: bool,
    pub sync: Option<SyncSettings>,
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
            ball_position: BallPosition {
                display: String::new(),
                edge: Edge::Right,
                y: 0.4,
            },
            autostart_initialized: false,
            sync: None,
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
    let state = app.state::<SettingsState>();
    let mut current = state.value.lock().unwrap();
    let mut next = current.clone();
    change(&mut next);
    let ball_changed = current.ball_visible != next.ball_visible
        || current.ball_auto_hide_seconds != next.ball_auto_hide_seconds
        || current.ball_position != next.ball_position;
    let visibility_changed = current.ball_visible != next.ball_visible;
    let appearance_changed = current.appearance != next.appearance;
    next.save(&state.root)?;
    *current = next.clone();
    drop(current);
    // 三处入口共用这个路径。即使某个系统操作失败，其余订阅者仍收到已保存的实际值。
    // 同步设置由后台保存，不应触碰只能在主线程操作的 NSPanel。
    let ball = if ball_changed {
        crate::ball::settings_changed(app, &next)
    } else {
        Ok(())
    };
    let tray = if visibility_changed {
        crate::tray::set_ball_checked(app, next.ball_visible)
    } else {
        Ok(())
    };
    let event = app
        .emit("settings-changed", &next)
        .map_err(|e| e.to_string());
    let appearance = if appearance_changed {
        crate::appearance::apply(app, next.appearance).map_err(|e| e.to_string())
    } else {
        Ok(())
    };
    ball.and(tray).and(appearance).and(event)?;
    Ok(next)
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
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}
#[derive(Serialize)]
pub struct AutostartResult {
    enabled: Option<bool>,
    error: Option<String>,
}
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> AutostartResult {
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
    #[cfg(not(debug_assertions))]
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
    #[cfg(debug_assertions)]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;
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
