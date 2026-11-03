//! macOS LaunchAgent 的首次登记与搬移后的路径修复。
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::settings::{self, SettingsState};

// 运行时传入 canonicalize 后的可执行文件路径；临时目录也作为参数，便于纯函数测试。
fn is_stable_path(executable: &Path, temporary: &Path) -> bool {
    executable.is_absolute()
        && !executable.components().any(|c| c == Component::ParentDir)
        && !executable.to_string_lossy().contains("AppTranslocation")
        && ![
            "/Volumes",
            "/System/Volumes/Data/Volumes",
            "/tmp",
            "/private/tmp",
            "/var/tmp",
            "/private/var/tmp",
            "/var/folders",
            "/private/var/folders",
        ]
        .iter()
        .any(|root| executable.starts_with(root))
        && !(temporary.is_absolute() && executable.starts_with(temporary))
}

#[derive(Debug, PartialEq)]
enum Action {
    Keep,
    Initialize,
    Repair,
}

fn action(initialized: bool, enabled: bool, registered: Option<&Path>, current: &Path) -> Action {
    if !initialized {
        Action::Initialize
    } else if enabled && registered != Some(current) {
        Action::Repair
    } else {
        Action::Keep
    }
}

fn registered_executable(plist: &Path) -> Result<PathBuf, String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct LaunchAgent {
        program: Option<String>,
        #[serde(default)]
        program_arguments: Vec<String>,
    }
    let agent: LaunchAgent = plist::from_file(plist).map_err(|e| e.to_string())?;
    // launchd 优先使用 Program；当前插件写 ProgramArguments[0]。
    agent
        .program
        .or_else(|| agent.program_arguments.into_iter().next())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "自启登记中缺少可执行文件路径".into())
}

pub fn initialize(app: &AppHandle) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Ok(());
    }
    // 与 tauri-plugin-autostart 的 LaunchAgent 模式一致，登记的是规范化的二进制路径。
    let current = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|e| e.to_string())?;
    let temporary = std::env::temp_dir();
    let temporary = temporary.canonicalize().unwrap_or(temporary);
    if !is_stable_path(&current, &temporary) {
        // 不消耗首次登记机会，也不覆盖可能仍指向稳定位置的旧登记。
        return Ok(());
    }
    let initialized = app.state::<SettingsState>().get().autostart_initialized;
    let enabled = app.autolaunch().is_enabled().map_err(|e| e.to_string())?;
    let registered = if initialized && enabled {
        // 插件未公开已登记路径；auto-launch 使用 home/Library/LaunchAgents/<app_name>.plist。
        let file = app
            .path()
            .home_dir()
            .map_err(|e| e.to_string())?
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", app.package_info().name));
        Some(registered_executable(&file)?)
    } else {
        None
    };
    match action(initialized, enabled, registered.as_deref(), &current) {
        Action::Initialize => {
            app.autolaunch().enable().map_err(|e| e.to_string())?;
            settings::update(app, |s| s.autostart_initialized = true)?;
        }
        Action::Repair => app.autolaunch().enable().map_err(|e| e.to_string())?,
        Action::Keep => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_locations_and_temporary_path_boundaries() {
        let temporary = Path::new("/Users/test/custom-temp");
        for path in [
            "/Applications/表情管理器.app/Contents/MacOS/sticker-manager",
            "/Users/test/Applications/表情管理器.app/Contents/MacOS/sticker-manager",
            "/Users/Shared/release-check/Applications/表情管理器.app/Contents/MacOS/sticker-manager",
            "/Users/test/custom-temp-backup/App.app/Contents/MacOS/app",
            "/Volumes-backup/App.app/Contents/MacOS/app",
        ] {
            assert!(is_stable_path(Path::new(path), temporary), "{path}");
        }
        for path in [
            "/Volumes/表情管理器/表情管理器.app/Contents/MacOS/sticker-manager",
            "/System/Volumes/Data/Volumes/Image/App.app/Contents/MacOS/app",
            "/private/var/folders/ab/cd/T/AppTranslocation/uuid/d/App.app/Contents/MacOS/app",
            "/Users/test/AppTranslocation/uuid/App.app/Contents/MacOS/app",
            "/tmp/App.app/Contents/MacOS/app",
            "/private/tmp/App.app/Contents/MacOS/app",
            "/var/tmp/App.app/Contents/MacOS/app",
            "/private/var/tmp/App.app/Contents/MacOS/app",
            "/var/folders/ab/cd/T/App.app/Contents/MacOS/app",
            "/private/var/folders/ab/cd/T/App.app/Contents/MacOS/app",
            "/Users/test/custom-temp/App.app/Contents/MacOS/app",
            "/Applications/../Volumes/Image/App.app/Contents/MacOS/app",
            "App.app/Contents/MacOS/app",
        ] {
            assert!(!is_stable_path(Path::new(path), temporary), "{path}");
        }
    }

    #[test]
    fn initialize_repair_and_respect_disabled_registration() {
        let current = Path::new("/Applications/App.app/Contents/MacOS/app");
        let old = Path::new("/Volumes/Image/App.app/Contents/MacOS/app");
        assert_eq!(action(false, false, None, current), Action::Initialize);
        assert_eq!(action(false, true, Some(old), current), Action::Initialize);
        assert_eq!(action(true, true, Some(old), current), Action::Repair);
        assert_eq!(action(true, true, Some(current), current), Action::Keep);
        assert_eq!(action(true, false, None, current), Action::Keep);
    }

    #[test]
    fn read_registered_path_from_xml_and_binary_plists() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("agent.plist");
        let executable = "/Applications/表情 & Friends.app/Contents/MacOS/app";
        let mut dict = plist::Dictionary::new();
        dict.insert(
            "ProgramArguments".into(),
            plist::Value::Array(vec![
                plist::Value::String(executable.into()),
                plist::Value::String("--autostart".into()),
            ]),
        );
        let value = plist::Value::Dictionary(dict.clone());
        value.to_file_xml(&file).unwrap();
        assert_eq!(registered_executable(&file).unwrap(), Path::new(executable));
        value.to_file_binary(&file).unwrap();
        assert_eq!(registered_executable(&file).unwrap(), Path::new(executable));
        dict.insert(
            "Program".into(),
            plist::Value::String("/Applications/Other".into()),
        );
        plist::Value::Dictionary(dict).to_file_xml(&file).unwrap();
        assert_eq!(
            registered_executable(&file).unwrap(),
            Path::new("/Applications/Other")
        );
        std::fs::write(&file, "broken").unwrap();
        assert!(registered_executable(&file).is_err());
    }
}
