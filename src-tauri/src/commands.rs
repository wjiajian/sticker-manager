//! 暴露给界面的命令。这里只做参数转换和加锁，逻辑都在 `sticker_core` 里。
//!
//! 命令都写成 `async`，文件读写、解码和数据库操作交给阻塞线程池。

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Serialize, Serializer};
use sticker_core::{ImportReport, Library, LibraryOverview, LibraryView, Sticker};
use tauri::{AppHandle, Manager};

/// 交给界面的错误，序列化成一句可以直接显示的话。
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error(transparent)]
    Core(#[from] sticker_core::Error),
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
    operation: impl FnOnce(&mut Library) -> sticker_core::Result<T> + Send + 'static,
) -> CommandResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Mutex<Library>>();
        let mut library = state.lock().map_err(|_| CommandError::Unavailable)?;
        Ok(operation(&mut library)?)
    })
    .await?
}

#[tauri::command]
pub async fn library_overview(app: AppHandle) -> CommandResult<LibraryOverview> {
    with_library(app, |library| library.overview()).await
}

#[tauri::command]
pub async fn list_stickers(app: AppHandle, view: LibraryView) -> CommandResult<Vec<Sticker>> {
    with_library(app, move |library| library.list_stickers(view)).await
}

#[tauri::command]
pub async fn import_stickers(app: AppHandle, paths: Vec<PathBuf>) -> CommandResult<ImportReport> {
    with_library(app, move |library| Ok(library.import_paths(&paths))).await
}
