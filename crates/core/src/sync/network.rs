//! OpenDAL 的异步操作在独立运行时执行，现有同步协议仍使用阻塞 Storage。
//! HTTP 边界统一处理重试、取消和脱敏，不让服务响应或签名 URL 进入应用错误。
use super::storage::{FileEntry, Storage, validate_key};
use crate::{Error, Result};
use futures_util::stream;
use http::{Request, Response, StatusCode};
use opendal::{Buffer, ErrorKind, HttpBody, HttpTransport, HttpTransporter, Operator, services};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    path::PathBuf,
    sync::{Arc, LazyLock},
    time::{Duration, SystemTime},
};
use url::Url;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "backend", rename_all = "camelCase")]
pub enum StorageConfig {
    Folder {
        folder: PathBuf,
    },
    Webdav {
        url: String,
    },
    S3 {
        endpoint: String,
        region: String,
        bucket: String,
        #[serde(default)]
        path: String,
    },
    R2 {
        #[serde(rename = "accountId")]
        account_id: String,
        bucket: String,
        #[serde(default)]
        path: String,
    },
}

/// 只用于输入和钥匙串；刻意不实现 Debug/Serialize，不能作为 IPC 返回值或设置持久化。
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}
impl Credentials {
    pub fn encode(&self) -> Result<String> {
        Ok(serde_json::to_string(&serde_json::json!({
            "username": self.username, "password": self.password,
            "accessKeyId": self.access_key_id, "secretAccessKey": self.secret_access_key,
        }))?)
    }
    pub fn decode(value: &str) -> Result<Self> {
        serde_json::from_str(value)
            .map_err(|_| Error::Sync("保存的凭据无法读取，请重新输入密码".into()))
    }
}

impl StorageConfig {
    pub fn normalized(&self) -> Result<Self> {
        fn endpoint(value: &str) -> Result<String> {
            let mut url = Url::parse(value.trim()).map_err(|_| {
                Error::Sync("地址无效，请填写完整的 http:// 或 https:// 地址".into())
            })?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(Error::Sync(
                    "地址须使用 HTTP 或 HTTPS，不能包含用户名、密码、查询参数或片段".into(),
                ));
            }
            let path = format!("{}/", url.path().trim_end_matches('/'));
            url.set_path(&path);
            Ok(url.to_string())
        }
        fn path(value: &str) -> Result<String> {
            let value = value.trim().trim_matches('/');
            if !value.is_empty() {
                validate_key(value)?;
            }
            Ok(value.to_owned())
        }
        fn bucket(value: &str) -> Result<String> {
            let value = value.trim();
            if value.is_empty()
                || value.contains(['/', '\\'])
                || value.chars().any(char::is_whitespace)
            {
                return Err(Error::Sync("Bucket 名称无效".into()));
            }
            Ok(value.to_owned())
        }
        Ok(match self {
            Self::Folder { folder } => {
                if !folder.is_absolute() {
                    return Err(Error::Sync("同步文件夹须使用绝对路径".into()));
                }
                self.clone()
            }
            Self::Webdav { url } => Self::Webdav {
                url: endpoint(url)?,
            },
            Self::S3 {
                endpoint: e,
                region,
                bucket: b,
                path: p,
            } => {
                if region.trim().is_empty() {
                    return Err(Error::Sync("请填写 Region".into()));
                }
                Self::S3 {
                    endpoint: endpoint(e)?,
                    region: region.trim().into(),
                    bucket: bucket(b)?,
                    path: path(p)?,
                }
            }
            Self::R2 {
                account_id,
                bucket: b,
                path: p,
            } => {
                let id = account_id.trim();
                if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(Error::Sync(
                        "Cloudflare Account ID 应为 32 位十六进制字符".into(),
                    ));
                }
                Self::R2 {
                    account_id: id.to_ascii_lowercase(),
                    bucket: bucket(b)?,
                    path: path(p)?,
                }
            }
        })
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Folder { .. } => "同步文件夹",
            Self::Webdav { .. } => "WebDAV",
            Self::R2 { .. } => "Cloudflare R2",
            Self::S3 { .. } => "S3 兼容",
        }
    }
    pub fn address(&self) -> String {
        match self {
            Self::Folder { folder } => folder.to_string_lossy().into_owned(),
            Self::Webdav { url } => Url::parse(url)
                .map(|u| {
                    format!(
                        "{}{}{}",
                        u.host_str().unwrap_or_default(),
                        u.port().map(|p| format!(":{p}")).unwrap_or_default(),
                        percent_encoding::percent_decode_str(u.path().trim_end_matches('/'))
                            .decode_utf8_lossy()
                    )
                })
                .unwrap_or_default(),
            Self::S3 { bucket, path, .. } | Self::R2 { bucket, path, .. } => {
                if path.is_empty() {
                    bucket.clone()
                } else {
                    format!("{bucket}/{path}")
                }
            }
        }
    }
    pub fn is_network(&self) -> bool {
        !matches!(self, Self::Folder { .. })
    }
    fn s3_endpoint(&self) -> Option<(String, String, &str, &str)> {
        match self {
            Self::S3 {
                endpoint,
                region,
                bucket,
                path,
            } => Some((endpoint.clone(), region.clone(), bucket, path)),
            Self::R2 {
                account_id,
                bucket,
                path,
            } => Some((
                format!("https://{account_id}.r2.cloudflarestorage.com"),
                "auto".into(),
                bucket,
                path,
            )),
            _ => None,
        }
    }
}

pub type CancelCheck = Arc<dyn Fn() -> bool + Send + Sync>;
const CANCELLED: &str = "同步设置已改变，本次同步已停止";
const LIMITED: &str = "存储后端限流，稍后会自动继续";
const NETWORK: &str = "网络不可用或请求超时，请检查网络和代理设置";
const CERTIFICATE: &str = "存储后端的证书无效，请检查服务地址和证书";
const AUTH: &str = "认证失败或权限不足，请检查账号、密钥和访问权限";
const NOT_FOUND: &str = "Bucket 或路径不存在，请检查存储地址";
const SPACE: &str = "存储空间不足，请清理空间后重试";
const SERVICE: &str = "存储服务暂时不可用，稍后会自动重试";

fn safe_error(error: opendal::Error) -> Error {
    // 只保留我们自己生成的固定文案，绝不格式化 OpenDAL 错误及其 source/context。
    let message = match error.message() {
        CANCELLED | LIMITED | NETWORK | CERTIFICATE | AUTH | NOT_FOUND | SPACE | SERVICE => {
            error.message()
        }
        _ => match error.kind() {
            ErrorKind::PermissionDenied => AUTH,
            ErrorKind::NotFound => NOT_FOUND,
            ErrorKind::RateLimited => LIMITED,
            ErrorKind::ConfigInvalid => "存储配置无效，请检查地址、Bucket 和 Region",
            _ => "存储请求失败，请检查后端配置和服务状态",
        },
    };
    Error::Sync(message.into())
}
/// PUT 到不存在的目录，RFC 4918 规定回 409（OpenDAL 归为 Unexpected），也有服务回 404。
/// 认证、限流、网络等已分类的错误换个写法也不会成功，不再重试。
fn missing_parent(error: &opendal::Error) -> bool {
    matches!(error.kind(), ErrorKind::NotFound | ErrorKind::Unexpected)
        && ![
            CANCELLED,
            LIMITED,
            NETWORK,
            CERTIFICATE,
            AUTH,
            NOT_FOUND,
            SPACE,
            SERVICE,
        ]
        .contains(&error.message())
}
fn transport_error(kind: ErrorKind, message: &'static str) -> opendal::Error {
    opendal::Error::new(kind, message)
}
fn check(cancelled: &CancelCheck) -> opendal::Result<()> {
    if cancelled() {
        Err(transport_error(ErrorKind::Unexpected, CANCELLED))
    } else {
        Ok(())
    }
}
fn http_error(status: StatusCode) -> Option<opendal::Error> {
    let (kind, message) = match status.as_u16() {
        401 | 403 => (ErrorKind::PermissionDenied, AUTH),
        429 | 503 => (ErrorKind::RateLimited, LIMITED),
        507 => (ErrorKind::Unexpected, SPACE),
        500..=599 => (ErrorKind::Unexpected, SERVICE),
        _ => return None,
    };
    Some(transport_error(kind, message))
}
fn request_error(error: &reqwest::Error) -> opendal::Error {
    // 只在内存中检查证书错误类型链；任何 source 都不会输出。
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    let mut certificate = false;
    while let Some(e) = source {
        let text = e.to_string().to_ascii_lowercase();
        certificate |= text.contains("certificate")
            || text.contains("invalid peer")
            || text.contains("unknownissuer");
        source = e.source();
    }
    transport_error(
        ErrorKind::Unexpected,
        if certificate { CERTIFICATE } else { NETWORK },
    )
}
fn retry_after(value: Option<&http::HeaderValue>) -> Option<Duration> {
    let text = value?.to_str().ok()?;
    text.parse::<u64>()
        .ok()
        .map(Duration::from_secs)
        .or_else(|| {
            httpdate::parse_http_date(text)
                .ok()?
                .duration_since(SystemTime::now())
                .ok()
        })
}

/// 时限按传输量放宽：大原图在慢速上行上也能传完，卡住的连接仍会超时。
const SLOWEST_RATE: u64 = 32 * 1024;
fn budget(base: Duration, bytes: u64) -> Duration {
    base + Duration::from_secs(bytes / SLOWEST_RATE)
}
/// 服务要求等得更久时结束本轮，按正常触发规则再试，不让界面长时间停在“同步中”。
const LONGEST_WAIT: Duration = Duration::from_secs(60);

struct Transport {
    client: reqwest::Client,
    cancelled: CancelCheck,
    request_timeout: Duration,
    object_timeout: Duration,
}
impl Transport {
    async fn wait(&self, duration: Duration) -> opendal::Result<()> {
        let Some(end) = tokio::time::Instant::now().checked_add(duration) else {
            return Err(transport_error(ErrorKind::RateLimited, LIMITED));
        };
        while tokio::time::Instant::now() < end {
            check(&self.cancelled)?;
            tokio::time::sleep(
                end.saturating_duration_since(tokio::time::Instant::now())
                    .min(Duration::from_millis(100)),
            )
            .await;
        }
        check(&self.cancelled)
    }
    async fn send(&self, request: &Request<Buffer>) -> opendal::Result<Response<HttpBody>> {
        let base = if request.uri().path().contains("/objects/") {
            self.object_timeout
        } else {
            self.request_timeout
        };
        let body = request.body().to_bytes();
        let upload = budget(base, body.len() as u64);
        let request = self
            .client
            .request(request.method().clone(), request.uri().to_string())
            .headers(request.headers().clone())
            .body(body);
        let timed_out = |_| transport_error(ErrorKind::Unexpected, NETWORK);
        // 缓冲整个响应，与 Storage 的 Vec<u8> 接口一致；读响应体也有时限。
        let future = async {
            let response = tokio::time::timeout(upload, request.send())
                .await
                .map_err(timed_out)?
                .map_err(|e| request_error(&e))?;
            let status = response.status();
            let headers = response.headers().clone();
            let download = budget(base, response.content_length().unwrap_or(0));
            let bytes = tokio::time::timeout(download, response.bytes())
                .await
                .map_err(timed_out)?
                .map_err(|e| request_error(&e))?;
            let len = bytes.len() as u64;
            let body = HttpBody::new(stream::iter([Ok(Buffer::from(bytes))]), Some(len));
            let mut response = Response::new(body);
            *response.status_mut() = status;
            *response.headers_mut() = headers;
            Ok(response)
        };
        let mut future = std::pin::pin!(future);
        loop {
            check(&self.cancelled)?;
            match tokio::time::timeout(Duration::from_millis(100), &mut future).await {
                Ok(result) => return result,
                Err(_) => continue,
            }
        }
    }
}
impl HttpTransport for Transport {
    async fn fetch(&self, request: Request<Buffer>) -> opendal::Result<Response<HttpBody>> {
        for attempt in 0..=3 {
            check(&self.cancelled)?;
            let response = self.send(&request).await;
            let (error, delay) = match response {
                Ok(response) => {
                    let status = response.status();
                    let Some(error) = http_error(status) else {
                        return Ok(response);
                    };
                    if !matches!(status.as_u16(), 429 | 500..=506 | 508..=599) {
                        return Err(error);
                    }
                    (
                        error,
                        retry_after(response.headers().get(http::header::RETRY_AFTER)),
                    )
                }
                Err(error) if error.message() == NETWORK => (error, None),
                Err(error) => return Err(error),
            };
            let delay = delay.unwrap_or_else(|| Duration::from_secs(1 << attempt));
            // Retry-After 可比默认退避更长；等待期间仍每 100 ms 检查取消。
            if attempt == 3 || delay > LONGEST_WAIT {
                return Err(error);
            }
            self.wait(delay).await?;
        }
        unreachable!()
    }
}

static NETWORK_RUNTIME: LazyLock<std::result::Result<tokio::runtime::Runtime, std::io::Error>> =
    LazyLock::new(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("sync-network")
            .build()
    });

pub struct NetworkStorage {
    operator: Operator,
    /// 只有 WebDAV 有：写入先直接 PUT，父目录不存在时才换这个会先 PROPFIND/MKCOL 的 operator。
    /// 默认每次 PUT 前都查一遍父目录，请求数翻倍，坚果云这类按请求次数限流的服务首次上传会慢一倍。
    creating: Option<Operator>,
}
impl NetworkStorage {
    pub fn new(
        config: &StorageConfig,
        credentials: &Credentials,
        cancelled: CancelCheck,
    ) -> Result<Self> {
        Self::with_timeouts(
            config,
            credentials,
            cancelled,
            Duration::from_secs(20),
            Duration::from_secs(120),
        )
    }
    fn with_timeouts(
        config: &StorageConfig,
        credentials: &Credentials,
        cancelled: CancelCheck,
        request_timeout: Duration,
        object_timeout: Duration,
    ) -> Result<Self> {
        let config = config.normalized()?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            // 保留系统代理默认行为；限制重定向，reqwest 会移除跨主机认证头。
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|_| Error::Sync(NETWORK.into()))?;
        let attach = |operator: opendal::Result<Operator>| {
            let operator = operator.map_err(safe_error)?;
            let context = operator
                .base_context()
                .with_http_transport(HttpTransporter::new(Transport {
                    client: client.clone(),
                    cancelled: cancelled.clone(),
                    request_timeout,
                    object_timeout,
                }));
            Ok::<_, Error>(operator.with_context(context))
        };
        let (operator, creating) = if let StorageConfig::Webdav { url } = &config {
            if credentials.username.is_empty() || credentials.password.is_empty() {
                return Err(Error::Sync("请填写 WebDAV 用户名和密码".into()));
            }
            // endpoint 可带编码后的路径；子目录由 creating 的 PROPFIND/MKCOL 按需创建。
            let parsed = Url::parse(url).map_err(|_| Error::Sync("WebDAV 地址无效".into()))?;
            let root = percent_encoding::percent_decode_str(parsed.path())
                .decode_utf8()
                .map_err(|_| Error::Sync("WebDAV 路径无效".into()))?;
            let webdav = |create_dir: bool| {
                attach(Operator::new(
                    services::Webdav::default()
                        .endpoint(&parsed[..url::Position::BeforePath])
                        .root(&root)
                        .username(&credentials.username)
                        .password(&credentials.password)
                        .disable_create_dir(!create_dir),
                ))
            };
            (webdav(false)?, Some(webdav(true)?))
        } else if let Some((endpoint, region, bucket, path)) = config.s3_endpoint() {
            if credentials.access_key_id.is_empty() || credentials.secret_access_key.is_empty() {
                return Err(Error::Sync(
                    "请填写 Access Key ID 和 Secret Access Key".into(),
                ));
            }
            let operator = attach(Operator::new(
                services::S3::default()
                    .endpoint(&endpoint)
                    .region(&region)
                    .bucket(bucket)
                    .root(path)
                    .access_key_id(&credentials.access_key_id)
                    .secret_access_key(&credentials.secret_access_key)
                    .disable_config_load()
                    .disable_ec2_metadata(),
            ))?;
            (operator, None)
        } else {
            return Err(Error::Sync("请选择网络存储后端".into()));
        };
        Ok(Self { operator, creating })
    }
    fn run<T: Send + 'static>(
        &self,
        future: impl Future<Output = opendal::Result<T>> + Send + 'static,
    ) -> Result<T> {
        let runtime = NETWORK_RUNTIME
            .as_ref()
            .map_err(|_| Error::Sync("无法启动网络同步线程".into()))?;
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        runtime.spawn(async move {
            let _ = tx.send(future.await);
        });
        // 不嵌套 block_on；从普通线程或 Tauri spawn_blocking 调用都不会启动嵌套运行时。
        rx.recv()
            .map_err(|_| Error::Sync("网络同步线程已停止".into()))?
            .map_err(safe_error)
    }
}
impl Storage for NetworkStorage {
    fn list(&self, prefix: &str) -> Result<Vec<FileEntry>> {
        if !prefix.is_empty() {
            validate_key(prefix)?;
        }
        let operator = self.operator.clone();
        let prefix = prefix.to_owned();
        self.run(async move {
            let entries = match operator.list_with(&prefix).recursive(true).await {
                Ok(entries) => entries,
                Err(e) if e.kind() == ErrorKind::NotFound && !prefix.is_empty() => {
                    return Ok(Vec::new());
                }
                Err(e) => return Err(e),
            };
            let mut result = entries
                .into_iter()
                .filter(|e| e.metadata().is_file() && e.path().starts_with(&prefix))
                .filter(|e| validate_key(e.path()).is_ok())
                .map(|e| FileEntry {
                    key: e.path().to_owned(),
                    size: e.metadata().content_length(),
                })
                .collect::<Vec<_>>();
            result.sort_by(|a, b| a.key.cmp(&b.key));
            Ok(result)
        })
    }
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>> {
        validate_key(key)?;
        let operator = self.operator.clone();
        let key = key.to_owned();
        self.run(async move {
            match operator.read(&key).await {
                Ok(bytes) => Ok(Some(bytes.to_vec())),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e),
            }
        })
    }
    fn write(&self, key: &str, bytes: &[u8]) -> Result<()> {
        validate_key(key)?;
        let operator = self.operator.clone();
        let creating = self.creating.clone();
        let key = key.to_owned();
        let bytes = bytes.to_vec();
        self.run(async move {
            match operator.write_with(&key, bytes.clone()).concurrent(1).await {
                Err(e) if creating.is_some() && missing_parent(&e) => {
                    let creating = creating.unwrap();
                    creating
                        .write_with(&key, bytes)
                        .concurrent(1)
                        .await
                        .map(|_| ())
                }
                result => result.map(|_| ()),
            }
        })
    }
    fn delete(&self, key: &str) -> Result<()> {
        validate_key(key)?;
        let operator = self.operator.clone();
        let key = key.to_owned();
        self.run(async move {
            match operator.delete(&key).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            }
        })
    }
}

fn probe_error(action: &str, error: Error) -> Error {
    let detail = match error {
        Error::Sync(message) => message,
        error => error.to_string(),
    };
    Error::Sync(format!("{action}探测失败，请检查{action}权限：{detail}"))
}

/// 探测三项权限；失败也尽力清理。文件名以点开头，不会影响目标检查。
pub fn probe(storage: &dyn Storage) -> Result<()> {
    let key = format!(".sticker-probe-{}", uuid::Uuid::new_v4());
    let bytes = uuid::Uuid::new_v4().to_string();
    let result = (|| {
        storage
            .write(&key, bytes.as_bytes())
            .map_err(|e| probe_error("写入", e))?;
        let actual = storage.read(&key).map_err(|e| probe_error("读取", e))?;
        if actual.as_deref() != Some(bytes.as_bytes()) {
            return Err(Error::Sync(
                "读取探测失败：写入内容尚不可读或与读回内容不一致".into(),
            ));
        }
        Ok(())
    })();
    let cleanup = storage.delete(&key).map_err(|e| probe_error("删除", e));
    result.and(cleanup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::atomic::{AtomicBool, Ordering},
        thread,
        time::Instant,
    };
    fn server(responses: Vec<(u16, &'static str, Duration)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            for (status, body, delay) in responses {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut buffer = [0; 4096];
                let _ = socket.read(&mut buffer);
                thread::spawn(move || {
                    thread::sleep(delay);
                    let _ = write!(
                        socket,
                        "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nRetry-After: 0\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });
        address
    }
    fn storage(url: String, cancelled: CancelCheck) -> NetworkStorage {
        NetworkStorage::new(
            &StorageConfig::Webdav { url },
            &Credentials {
                username: "test-account".into(),
                password: "do-not-disclose-secret".into(),
                ..Credentials::default()
            },
            cancelled,
        )
        .unwrap()
    }
    #[test]
    fn invalid_keys_and_urls_do_not_reach_the_network() {
        let s = storage("http://127.0.0.1:1".into(), Arc::new(|| false));
        for key in ["", "..", "../a", "a/../b", "/a", "a\\b", "./a"] {
            assert!(s.read(key).is_err());
            assert!(s.write(key, b"x").is_err());
            assert!(s.delete(key).is_err());
        }
        for url in [
            "ftp://example.com",
            "https://user:secret@example.com",
            "https://example.com/?secret=secret",
            "https://example.com/#secret",
        ] {
            let error = StorageConfig::Webdav { url: url.into() }
                .normalized()
                .unwrap_err()
                .to_string();
            assert!(!error.contains("secret"));
        }
    }
    #[test]
    fn webdav_puts_directly_and_creates_parents_only_when_missing() {
        // 父目录已存在：只有一次 PUT，服务端只准备了一个响应。
        let url = server(vec![(201, "", Duration::ZERO)]);
        storage(url, Arc::new(|| false))
            .write("objects/a.png", b"x")
            .unwrap();
        // 父目录不存在：PUT 409 → PROPFIND 父目录 404 → MKCOL → 再 PUT。
        let url = server(vec![
            (409, "", Duration::ZERO),
            (404, "", Duration::ZERO),
            (201, "", Duration::ZERO),
            (201, "", Duration::ZERO),
        ]);
        storage(url, Arc::new(|| false))
            .write("objects/a.png", b"x")
            .unwrap();
        // 认证失败不再走建目录的路径。
        let url = server(vec![(401, "", Duration::ZERO)]);
        let error = storage(url, Arc::new(|| false))
            .write("objects/a.png", b"x")
            .unwrap_err();
        assert!(error.to_string().contains(AUTH));
    }
    #[test]
    fn missing_files_and_directories_are_idempotent() {
        let url = server(vec![(404, "missing", Duration::ZERO); 3]);
        let s = storage(url, Arc::new(|| false));
        assert_eq!(s.read("missing").unwrap(), None);
        s.delete("missing").unwrap();
        assert!(s.list("ops/").unwrap().is_empty());
    }
    #[test]
    fn http_errors_are_classified_without_response_or_credentials() {
        for (status, text) in [
            (401, AUTH),
            (403, AUTH),
            (429, LIMITED),
            (503, LIMITED),
            (507, SPACE),
            (500, SERVICE),
        ] {
            let attempts = if matches!(status, 429 | 500 | 503) {
                4
            } else {
                1
            };
            let url = server(vec![
                (
                    status,
                    "do-not-disclose-secret https://host/?X-Amz-Signature=secret",
                    Duration::ZERO
                );
                attempts
            ]);
            let error = storage(url, Arc::new(|| false))
                .read("test")
                .unwrap_err()
                .to_string();
            assert!(error.contains(text), "{status}: {error}");
            assert!(!error.contains("secret"));
            assert!(!error.contains("Signature"));
            assert!(!error.contains("test-account"));
        }
        let error = safe_error(opendal::Error::new(
            ErrorKind::Unexpected,
            "secret https://host/?signature=secret",
        ));
        assert!(!error.to_string().contains("secret"));
        assert!(
            safe_error(transport_error(ErrorKind::Unexpected, CERTIFICATE))
                .to_string()
                .contains("证书无效")
        );
    }
    #[test]
    fn retries_then_succeeds_and_parses_retry_after() {
        let url = server(vec![
            (503, "busy", Duration::ZERO),
            (200, "complete", Duration::ZERO),
        ]);
        assert_eq!(
            storage(url, Arc::new(|| false))
                .read("test")
                .unwrap()
                .unwrap(),
            b"complete"
        );
        assert_eq!(
            retry_after(Some(&http::HeaderValue::from_static("12"))),
            Some(Duration::from_secs(12))
        );
        let date = httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(20));
        let duration = retry_after(Some(&http::HeaderValue::from_str(&date).unwrap())).unwrap();
        assert!(duration > Duration::from_secs(18));
    }
    #[test]
    fn blocking_bridge_works_inside_tokio_spawn_blocking() {
        let url = server(vec![(200, "works", Duration::ZERO)]);
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                tokio::task::spawn_blocking(move || {
                    assert_eq!(
                        storage(url, Arc::new(|| false))
                            .read("test")
                            .unwrap()
                            .unwrap(),
                        b"works"
                    );
                })
                .await
                .unwrap();
            });
    }
    #[test]
    fn cancellation_aborts_slow_request_within_one_second() {
        let url = server(vec![(200, "late", Duration::from_secs(2))]);
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let s = storage(url, Arc::new(move || flag.load(Ordering::SeqCst)));
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            cancelled.store(true, Ordering::SeqCst);
        });
        let start = Instant::now();
        assert!(
            s.read("objects/test")
                .unwrap_err()
                .to_string()
                .contains("停止")
        );
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn cancellation_interrupts_long_retry_after() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (sent, received) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            assert!(socket.read(&mut buffer).unwrap() > 0);
            socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nRetry-After: 30\r\nConnection: close\r\n\r\n").unwrap();
            sent.send(()).unwrap();
        });
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let s = storage(url, Arc::new(move || flag.load(Ordering::SeqCst)));
        let (done, result) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let _ = done.send(s.read("test"));
        });
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        thread::sleep(Duration::from_millis(150));
        assert!(result.try_recv().is_err());
        cancelled.store(true, Ordering::SeqCst);
        let error = result
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().contains("停止"));
    }
    #[test]
    fn retry_after_beyond_the_cap_ends_the_round_without_waiting() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer);
            socket.write_all(b"HTTP/1.1 503 Busy\r\nContent-Length: 0\r\nRetry-After: 600\r\nConnection: close\r\n\r\n").unwrap();
        });
        let start = Instant::now();
        let error = storage(url, Arc::new(|| false)).read("test").unwrap_err();
        assert!(error.to_string().contains(LIMITED), "{error}");
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn time_budget_grows_with_transfer_size() {
        let base = Duration::from_secs(120);
        assert_eq!(budget(base, 1024), base);
        // 20 MB 的动图按最慢 32 KB/s 估算，约 12 分钟。
        assert_eq!(
            budget(base, 20 * 1024 * 1024),
            base + Duration::from_secs(640)
        );
    }
    #[test]
    fn timeout_covers_response_body_and_finishes_with_network_error() {
        let url = server(vec![(200, "late", Duration::from_secs(1)); 4]);
        let s = NetworkStorage::with_timeouts(
            &StorageConfig::Webdav { url },
            &Credentials {
                username: "u".into(),
                password: "p".into(),
                ..Credentials::default()
            },
            Arc::new(|| false),
            Duration::from_millis(50),
            Duration::from_millis(50),
        )
        .unwrap();
        let start = Instant::now();
        assert!(s.read("test").unwrap_err().to_string().contains(NETWORK));
        assert!(start.elapsed() < Duration::from_secs(9));
    }
    #[test]
    fn r2_maps_to_s3_and_paths_are_distinct() {
        let config = StorageConfig::R2 {
            account_id: "0123456789abcdef0123456789abcdef".into(),
            bucket: "stickers".into(),
            path: "one".into(),
        }
        .normalized()
        .unwrap();
        let (endpoint, region, bucket, path) = config.s3_endpoint().unwrap();
        assert_eq!(
            endpoint,
            "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com"
        );
        assert_eq!((region.as_str(), bucket, path), ("auto", "stickers", "one"));
        assert_eq!(config.address(), "stickers/one");
        let other = StorageConfig::R2 {
            account_id: "0123456789abcdef0123456789abcdef".into(),
            bucket: "stickers".into(),
            path: "two".into(),
        };
        assert_ne!(config, other);
    }
    #[test]
    fn probe_distinguishes_write_read_and_delete_permissions() {
        struct Limited {
            inner: super::super::storage::MemoryStorage,
            denied: &'static str,
        }
        impl Storage for Limited {
            fn list(&self, p: &str) -> Result<Vec<FileEntry>> {
                self.inner.list(p)
            }
            fn read(&self, k: &str) -> Result<Option<Vec<u8>>> {
                if self.denied == "读取" {
                    Err(Error::Sync(AUTH.into()))
                } else {
                    self.inner.read(k)
                }
            }
            fn write(&self, k: &str, b: &[u8]) -> Result<()> {
                if self.denied == "写入" {
                    Err(Error::Sync(AUTH.into()))
                } else {
                    self.inner.write(k, b)
                }
            }
            fn delete(&self, k: &str) -> Result<()> {
                if self.denied == "删除" {
                    Err(Error::Sync(AUTH.into()))
                } else {
                    self.inner.delete(k)
                }
            }
        }
        for denied in ["写入", "读取", "删除"] {
            let s = Limited {
                inner: Default::default(),
                denied,
            };
            assert!(
                probe(&s)
                    .unwrap_err()
                    .to_string()
                    .contains(&format!("{denied}探测失败"))
            );
            if denied != "删除" {
                assert!(s.list("").unwrap().is_empty());
            }
        }
    }
}
