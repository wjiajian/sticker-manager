#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("读写仓库文件失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("数据库出错：{0}")]
    Db(#[from] rusqlite::Error),

    #[error(
        "仓库由更新版本的应用创建（数据版本 {found}，本应用最高支持 {supported}），请先升级应用"
    )]
    SchemaTooNew { found: u32, supported: u32 },
}

pub type Result<T> = std::result::Result<T, Error>;
