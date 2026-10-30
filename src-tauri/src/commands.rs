//! 暴露给界面的命令。这里只做参数转换和加锁，逻辑都在 `sticker_core` 里。
//!
//! 命令都写成 `async`：同步命令会跑在主线程上，读写数据库时会卡住窗口。

use std::sync::Mutex;

use serde::{Serialize, Serializer};
use sticker_core::{Library, LibraryOverview};
use tauri::State;

/// 交给界面的错误，序列化成一句可以直接显示的话。
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct CommandError(#[from] sticker_core::Error);

impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

type CommandResult<T> = Result<T, CommandError>;

#[tauri::command]
pub async fn library_overview(
    library: State<'_, Mutex<Library>>,
) -> CommandResult<LibraryOverview> {
    Ok(library.lock().unwrap().overview()?)
}
