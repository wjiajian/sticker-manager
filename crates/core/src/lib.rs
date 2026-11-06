//! 表情管理器的核心逻辑：仓库、分类、导入、同步与合并规则。
//!
//! 不依赖 Tauri，界面经由 `src-tauri` 里的命令调用这里，可以脱离界面单独测试。
//! 术语见docs/GLOSSARY.md。

mod db;
mod deletion;
mod device;
mod error;
mod import;
mod library;
mod organization;
mod search;
pub mod sync;
mod time;
mod usage;

pub use device::Device;

pub use error::{Error, Result};
pub use import::{ImportFailure, ImportProgress, ImportReport, import_paths_with_progress};
pub use library::{
    Library, LibraryOverview, LibraryView, Sticker, StickerCursor, StickerPage, StickerPageFilter,
};
pub use organization::{CATEGORY_COLORS, Category, CategorySummary, UNCATEGORIZED};
