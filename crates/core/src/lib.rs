//! 表情管理器的核心逻辑：仓库、标签、导入、同步与合并规则。
//!
//! 不依赖 Tauri，界面经由 `src-tauri` 里的命令调用这里，可以脱离界面单独测试。
//! 术语见仓库根目录的 GLOSSARY.md。

mod db;
mod device;
mod error;
mod import;
mod library;
mod usage;

pub use device::Device;

pub use error::{Error, Result};
pub use import::{ImportFailure, ImportReport};
pub use library::{Library, LibraryOverview, LibraryView, Sticker};
