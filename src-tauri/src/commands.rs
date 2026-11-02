//! 暴露给界面的命令，负责系统操作、参数转换及仓库加锁。
//!
//! 命令都写成 `async`，文件读写、解码和数据库操作交给阻塞线程池。

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Serialize, Serializer};
use sticker_core::{
    ImportReport, Library, LibraryOverview, LibraryView, Sticker, Tag, TagGroup, TagSidebar,
};
use tauri::{AppHandle, Emitter, Manager};

use crate::{clipboard, export::ExportDirectory};

/// 交给界面的错误，序列化成一句可以直接显示的话。
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error(transparent)]
    Core(#[from] sticker_core::Error),
    #[error("无法准备表情文件：{0}")]
    Export(#[from] std::io::Error),
    #[error("复制失败：{0}")]
    Clipboard(String),
    #[error("无法显示表情文件：{0}")]
    Reveal(#[from] tauri_plugin_opener::Error),
    #[error("操作已完成，但使用记录未保存：{0}")]
    Usage(sticker_core::Error),
    #[error("仓库暂时无法访问，请重新启动应用")]
    Unavailable,
    #[error("仓库操作未完成：{0}")]
    Task(#[from] tauri::Error),
}

impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

type CommandResult<T> = Result<T, CommandError>;

async fn with_library<T: Send + 'static>(
    app: AppHandle,
    operation: impl FnOnce(&mut Library) -> CommandResult<T> + Send + 'static,
) -> CommandResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Mutex<Library>>();
        let mut library = state.lock().map_err(|_| CommandError::Unavailable)?;
        operation(&mut library)
    })
    .await?
}

#[tauri::command]
pub async fn library_overview(app: AppHandle) -> CommandResult<LibraryOverview> {
    with_library(app, |library| Ok(library.overview()?)).await
}

#[tauri::command]
pub async fn list_stickers(app: AppHandle, view: LibraryView) -> CommandResult<Vec<Sticker>> {
    with_library(app, move |library| Ok(library.list_stickers(view)?)).await
}

#[tauri::command]
pub async fn search_stickers(app: AppHandle, query: String) -> CommandResult<Vec<Sticker>> {
    with_library(app, move |library| Ok(library.search_stickers(&query)?)).await
}

#[tauri::command]
pub async fn sticker(app: AppHandle, id: String) -> CommandResult<Option<Sticker>> {
    with_library(app, move |library| match library.sticker(&id) {
        Ok(sticker) => Ok(Some(sticker)),
        // 自动清理后，界面可以清空选中；其他读取错误仍正常报告。
        Err(sticker_core::Error::StickerUnavailable) => Ok(None),
        Err(error) => Err(error.into()),
    })
    .await
}

#[tauri::command]
pub async fn delete_sticker(app: AppHandle, id: String) -> CommandResult<()> {
    with_library(app, move |library| Ok(library.delete_sticker(&id)?)).await
}

#[tauri::command]
pub async fn restore_sticker(app: AppHandle, id: String) -> CommandResult<()> {
    with_library(app, move |library| Ok(library.restore_sticker(&id)?)).await
}

#[tauri::command]
pub async fn purge_sticker(app: AppHandle, id: String) -> CommandResult<()> {
    with_library(app, move |library| Ok(library.purge_sticker(&id)?)).await
}

#[tauri::command]
pub async fn tag_sidebar(app: AppHandle) -> CommandResult<TagSidebar> {
    with_library(app, |library| Ok(library.tag_sidebar()?)).await
}

#[tauri::command]
pub async fn create_tag(
    app: AppHandle,
    name: String,
    color: Option<String>,
    group_id: Option<String>,
) -> CommandResult<Tag> {
    with_library(app, move |library| {
        Ok(library.create_tag(&name, color.as_deref(), group_id.as_deref())?)
    })
    .await
}

#[tauri::command]
pub async fn rename_tag(app: AppHandle, id: String, name: String) -> CommandResult<()> {
    with_library(app, move |library| Ok(library.rename_tag(&id, &name)?)).await
}

#[tauri::command]
pub async fn set_tag_color(app: AppHandle, id: String, color: String) -> CommandResult<()> {
    with_library(app, move |library| Ok(library.set_tag_color(&id, &color)?)).await
}

#[tauri::command]
pub async fn move_tag(app: AppHandle, id: String, group_id: Option<String>) -> CommandResult<()> {
    with_library(app, move |library| {
        Ok(library.move_tag(&id, group_id.as_deref())?)
    })
    .await
}

#[tauri::command]
pub async fn delete_tag(app: AppHandle, id: String) -> CommandResult<()> {
    with_library(app, move |library| Ok(library.delete_tag(&id)?)).await
}

#[tauri::command]
pub async fn create_tag_group(app: AppHandle, name: String) -> CommandResult<TagGroup> {
    with_library(app, move |library| Ok(library.create_tag_group(&name)?)).await
}

#[tauri::command]
pub async fn rename_tag_group(app: AppHandle, id: String, name: String) -> CommandResult<()> {
    with_library(
        app,
        move |library| Ok(library.rename_tag_group(&id, &name)?),
    )
    .await
}

#[tauri::command]
pub async fn delete_tag_group(app: AppHandle, id: String) -> CommandResult<()> {
    with_library(app, move |library| Ok(library.delete_tag_group(&id)?)).await
}

#[tauri::command]
pub async fn add_sticker_tag(
    app: AppHandle,
    sticker_id: String,
    tag_id: String,
) -> CommandResult<()> {
    with_library(app, move |library| {
        Ok(library.add_sticker_tag(&sticker_id, &tag_id)?)
    })
    .await
}

#[tauri::command]
pub async fn remove_sticker_tag(
    app: AppHandle,
    sticker_id: String,
    tag_id: String,
) -> CommandResult<()> {
    with_library(app, move |library| {
        Ok(library.remove_sticker_tag(&sticker_id, &tag_id)?)
    })
    .await
}

#[tauri::command]
pub async fn create_tag_and_add(
    app: AppHandle,
    sticker_id: String,
    name: String,
) -> CommandResult<Tag> {
    with_library(app, move |library| {
        Ok(library.create_tag_and_add(&sticker_id, &name)?)
    })
    .await
}

#[tauri::command]
pub async fn set_sticker_caption(app: AppHandle, id: String, caption: String) -> CommandResult<()> {
    with_library(app, move |library| {
        Ok(library.set_sticker_caption(&id, &caption)?)
    })
    .await
}

#[tauri::command]
pub async fn import_stickers(app: AppHandle, paths: Vec<PathBuf>) -> CommandResult<ImportReport> {
    let report = with_library(app.clone(), move |library| Ok(library.import_paths(&paths))).await?;
    notify_usage(&app);
    Ok(report)
}

#[tauri::command]
pub async fn copy_sticker(app: AppHandle, id: String, file_only: bool) -> CommandResult<()> {
    let handle = app.clone();
    with_library(app.clone(), move |library| {
        let sticker = library.sticker_for_use(&id)?;
        let image = if file_only {
            None
        } else {
            Some(sticker.read_image()?)
        };
        let path = handle.state::<ExportDirectory>().prepare(&sticker)?;
        clipboard::write(&path, image.as_ref())
            .map_err(|error| CommandError::Clipboard(error.to_string()))?;
        library.record_usage(&id).map_err(CommandError::Usage)
    })
    .await?;
    notify_usage(&app);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedDrag {
    path: PathBuf,
    thumbnail_path: PathBuf,
}

#[tauri::command]
pub async fn prepare_sticker_drag(app: AppHandle, id: String) -> CommandResult<PreparedDrag> {
    let handle = app.clone();
    with_library(app, move |library| {
        let sticker = library.sticker_for_use(&id)?;
        Ok(PreparedDrag {
            path: handle.state::<ExportDirectory>().prepare(&sticker)?,
            thumbnail_path: sticker.thumbnail_path,
        })
    })
    .await
}

#[tauri::command]
pub async fn record_sticker_usage(app: AppHandle, id: String) -> CommandResult<()> {
    with_library(app.clone(), move |library| {
        library.record_usage(&id).map_err(CommandError::Usage)
    })
    .await?;
    notify_usage(&app);
    Ok(())
}

fn notify_usage(app: &AppHandle) {
    if let Err(error) = app.emit("library-changed", ()) {
        eprintln!("通知仓库更新失败：{error}");
    }
}

#[tauri::command]
pub async fn toggle_sticker_favorite(app: AppHandle, id: String) -> CommandResult<bool> {
    with_library(app, move |library| Ok(library.toggle_favorite(&id)?)).await
}

#[tauri::command]
pub async fn reveal_sticker(app: AppHandle, id: String) -> CommandResult<()> {
    with_library(app, move |library| {
        let sticker = library.sticker(&id)?;
        tauri_plugin_opener::reveal_item_in_dir(sticker.object_path)?;
        Ok(())
    })
    .await
}
