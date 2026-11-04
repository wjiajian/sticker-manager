#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("同步失败：{0}")]
    Sync(String),

    #[error("需要更新应用：{0}")]
    SyncUpgrade(String),

    #[error("设备身份冲突：同步文件夹中存在本机未记录的操作。请勿在两台电脑上使用复制的仓库目录")]
    DeviceConflict,

    #[error("同步文件格式错误：{0}")]
    Json(#[from] serde_json::Error),
    #[error("读写仓库文件失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("无法读取图片：{0}")]
    Image(#[from] image::ImageError),

    #[error("本机设备信息损坏，请恢复 local/device.json，避免重新生成设备 ID")]
    InvalidDevice,

    #[error("无法读取电脑名：{0}")]
    DeviceName(String),

    #[error("表情不存在或已在最近删除中")]
    StickerUnavailable,

    #[error("表情不在最近删除中，可能已经恢复或彻底删除")]
    StickerNotDeleted,

    #[error("{0}名称不能为空")]
    EmptyName(&'static str),

    #[error("已存在同名{0}，请使用其他名称")]
    DuplicateName(&'static str),

    #[error("分类不存在")]
    CategoryUnavailable,

    #[error("“未分类”是保留名称，请使用其他名称")]
    ReservedCategoryName,

    #[error("分类名称不能以点开头，也不能包含 .. 或斜杠")]
    InvalidCategoryName,

    #[error("请选择八种分类颜色之一")]
    InvalidCategoryColor,

    #[error("图片没有可读取的帧")]
    EmptyImage,

    #[error("系统时间早于 Unix 起点，请检查系统时间")]
    InvalidTime,

    #[error("文件过大，无法记录文件大小")]
    FileTooLarge,

    #[error("数据库出错：{0}")]
    Db(#[from] rusqlite::Error),

    #[error(
        "仓库由更新版本的应用创建（数据版本 {found}，本应用最高支持 {supported}），请先升级应用"
    )]
    SchemaTooNew { found: u32, supported: u32 },
}

pub type Result<T> = std::result::Result<T, Error>;
