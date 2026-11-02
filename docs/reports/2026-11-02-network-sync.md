# 网络同步实现与验证记录

> 历史记录：代理交回的实现与验收报告，原样保留，对应提交 7e48402。审查后的修复改动过其中部分内容，现状以 [design/README.md](../../design/README.md)、[docs/tech-stack.md](../tech-stack.md)、[ADR](../adr/) 和 git 历史为准。

验证日期：2026-11-02，macOS arm64。改动留在工作区，未提交。测试只使用 `/tmp/sticker-sync-ui-ct8vhuag` 下的两个本机仓库和专用容器，没有打开或修改真实仓库。原有未跟踪文件 `docs/panel-implementation-report.md` 未改动。

## 实现结果

同步文件夹、通用 WebDAV、Cloudflare R2、S3 兼容存储共用原有 `Storage` 接口、操作日志和合并规则。WebDAV 不绑定具体供应商。Cloudflare R2 在偏好设置中有独立入口，使用 S3 协议，内部生成账号 endpoint，region 固定为 `auto`。

网络连接先探测写入、读回、删除权限，再检查目标。已有仓库通过原生确认框展示设备数和表情数；非空且没有清单的路径拒绝加入。密码及访问密钥只写入系统钥匙串，IPC 不返回凭据，设置文件只保存目标配置。已配置目标可重新输入凭据，换地址需先停止同步。

### 文件

| 文件 | 改动 |
| --- | --- |
| `crates/core/Cargo.toml`、`Cargo.lock` | OpenDAL、rustls HTTP 客户端及运行时依赖 |
| `crates/core/src/sync/network.rs` | 后端配置、异步到阻塞桥接、HTTP 错误分类、超时/重试/取消、读写删除探测及单测 |
| `crates/core/src/sync/storage.rs` | 共用 key 校验，准确说明部分写入的可见性 |
| `crates/core/src/sync/mod.rs` | 通用检查、半截清单等待、原图进度回调、未完成 PUT 的大小检查 |
| `crates/core/src/sync/tests.rs` | 复用两设备端到端测试，补充半截文件、进度锁和未完成原图恢复测试 |
| `crates/core/src/sync/tests/network_integration.rs` | 默认忽略的两种容器后端及断线恢复测试 |
| `src-tauri/Cargo.toml`、`src-tauri/src/credentials.rs` | keyring、按设备与目标隔离的条目、凭据缓存、mock 测试 |
| `src-tauri/src/settings.rs` | 按后端保存配置，兼容旧 folder JSON，未知后端沿用回退行为 |
| `src-tauri/src/sync.rs` | 后端选择、连接/加入/取消/更新凭据命令、状态与进度事件 |
| `src-tauri/src/lib.rs`、`src-tauri/permissions/main.toml` | 注册命令与权限 |
| `src/lib/ipc.ts` | 通用目标、进度、网络配置与命令类型 |
| `src/windows/main/SyncPreferences.tsx` | 四种后端表单、原生确认、失败保留输入、取消、重新输入凭据及进度 |
| `src/windows/main/SyncCard.tsx`、`TitleBar.tsx`、`useSyncStatus.ts` | 后端名称、简短地址、进度和等待提示 |
| `docs/adr/0005-sync-format-and-merge.md`、`docs/tech-stack.md`、`design/README.md`、`GLOSSARY.md` | 实际后端能力、写入语义、凭据与剩余范围 |

## 依赖和取舍

- OpenDAL **0.59.5**：关闭默认 features，启用 `services-webdav`、`services-s3`、`executors-tokio`。实现依据该版本实际 API；不使用历史版本的 BlockingOperator。参见 [OpenDAL 文档](https://docs.rs/opendal/0.59.5/opendal/)。
- keyring **4.2.0**：关闭默认 features，启用 `v1`。该版本按目标平台接入 macOS 钥匙串和 Windows 凭据管理器，不再使用旧版 `apple-native` / `windows-native` feature 名。参见 [keyring 文档](https://docs.rs/keyring/4.2.0/keyring/)。
- reqwest **0.13.5**（Cargo 约束为 `0.13`）：关闭默认 features，启用 `rustls`、`system-proxy`。正常目标依赖树没有 `openssl` 或 `native-tls`。
- Tokio 使用 `rt-multi-thread`、`time`、`net`。独立运行时接收任务，调用方通过通道等待，不在 Tauri 的 `spawn_blocking` 中嵌套 `block_on`。运行时有两个线程，但单个同步轮次仍串行传输。
- 使用自定义 OpenDAL HTTP transport，把错误脱敏、响应体超时、Retry-After 和取消放在同一边界。连接超时 5 秒，普通请求 20 秒，原图请求 120 秒；整个响应体包含在请求时限内。响应缓冲到内存，与现有 `Storage::read -> Vec<u8>` 一致，没有新增流式接口。
- 429、503、其他可重试的 5xx 和网络失败最多重试 3 次，默认等待 1、2、4 秒。有 Retry-After 时遵守秒数或 HTTP 日期；等待和正在进行的请求都每 100 毫秒检查取消。认证、证书和 507 不重试。错误只输出固定中文类别，不附带底层响应、密钥或签名 URL。
- WebDAV 使用直接 PUT，父目录由 OpenDAL 创建；不依赖 MOVE。半截日志、设备文件、清单和原图由读取方校验。清单缺失或半截时同步返回等待，不推进上次同步时间。续传比较远端文件大小，重新上传未完成的原图；完整但内容错误的同大小对象仍由接收端哈希校验阻止应用，本轮未新增远端损坏修复协议。
- 进度回调在仓库锁外执行，界面事件最多约每 150 毫秒一次，完成时立即发出；进度表示当前处理的一批原图，不是整个仓库的永久累计计数。
- 钥匙串 account 为设备 id 加规范化目标的 SHA-256 摘要，避免平台条目名称长度限制。debug service 为 `com.shaojian.sticker-manager.sync.debug`，release service 为 `com.shaojian.sticker-manager.sync`。缓存成功和失败的读取结果，避免周期调度重复请求钥匙串访问。重新输入或停止时更新/清除缓存。

## 自动检查

| 检查 | 结果 |
| --- | --- |
| `cargo fmt --all --check` | 通过 |
| `cargo test --workspace` | 103 项通过；5 项默认忽略，其中 4 项是下述容器测试，另 1 项为原有搜索性能测量 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 |
| `pnpm typecheck` | 通过 |
| `pnpm build` | 通过 |
| WebDAV 存储行为与原有两设备端到端场景 | 通过 |
| MinIO 存储行为与原有两设备端到端场景 | 通过 |
| WebDAV / MinIO 断线恢复 | 两项通过 |

四项容器测试最后一次串行运行全部通过，耗时 19.35 秒。覆盖中文/空格路径、自动父目录、重复写入、相对 key 与大小、排序/目录过滤、不存在的文件与前缀、权限探测、两设备收敛、原图上传下载、彻底删除远端原图。

断线测试在上传第 3 张原图的回调里 `docker stop -t 0`，本轮报错后检查本地逻辑状态与断线前一致；启动容器，确认已有 3 张原图，再同步到 20 张，最后两台设备收敛。

普通单测还覆盖旧设置格式、凭据缓存与隔离、错误脱敏、404、429/503 重试、Retry-After 解析、请求超时、在 `spawn_blocking` 内调用阻塞存储。慢请求取消在 1 秒内返回；另用 `Retry-After: 600` 响应验证等待期间也能在取消后 1 秒内返回。单测凭据存储使用自建 trait 的 mock，不访问系统钥匙串。

另用本机 Python HTTPS 服务和临时自签名证书实际请求，确认请求被拒绝并返回“存储后端的证书无效，请检查服务地址和证书”。生成测试证书使用系统 openssl 命令，应用依赖没有链接 OpenSSL；测试辅助源码随后移除。

## Docker 复现

以下凭据均为专用容器的测试值。不要替换成真实服务密钥后把命令存入报告。

### WebDAV

使用 `rclone/rclone:latest`，本次拉取摘要：
`sha256:2687085f718d3c628f7fdfb77c52a1d344332aed543110bb88088f2c70d43eb5`。

```sh
docker run -d --name sticker-sync-webdav \
  -p 127.0.0.1:18080:8080 \
  rclone/rclone@sha256:2687085f718d3c628f7fdfb77c52a1d344332aed543110bb88088f2c70d43eb5 \
  serve webdav /data --addr :8080 --user sync-test --pass sync-test-only

SYNC_WEBDAV_URL=http://127.0.0.1:18080 \
SYNC_WEBDAV_USER=sync-test SYNC_WEBDAV_PASSWORD=sync-test-only \
SYNC_WEBDAV_CONTAINER=sticker-sync-webdav \
cargo test -p sticker-core sync::tests::network_integration::webdav_ \
  -- --ignored --nocapture --test-threads=1
```

不能给容器加 `--rm`，因为断线测试会停止后再启动同一个容器。UI 取消测试最初直接从容器文件系统清理测试对象，碰到了 rclone 的目录缓存；重启容器清空缓存后完成复测。测试程序本身始终通过 WebDAV API 清理文件。

### MinIO

本机拉取 `minio/minio` 返回 access denied，Quay 镜像也未成功；因此从 [MinIO 官方发布标签](https://github.com/minio/minio/tree/RELEASE.2025-09-07T16-13-09Z) 构建本地测试镜像。源提交为 `07c3a429bfed433e49018cb0f78a52145d4bedeb`，镜像名 `sticker-sync-minio:test`，本次镜像 ID 为 `sha256:85c30c00a6eb7048ac34b294e5b999ea4eca8c8a089558eeaa9fa2e8400f428c`。

```sh
git clone --depth 1 --branch RELEASE.2025-09-07T16-13-09Z \
  https://github.com/minio/minio.git /tmp/sticker-sync-minio-source
```

在该源码目录创建 `Dockerfile.codex`：

```dockerfile
FROM golang:1.24-alpine AS build
WORKDIR /src
COPY . .
RUN CGO_ENABLED=0 go build -buildvcs=false -trimpath -o /minio .
FROM rclone/rclone:latest
COPY --from=build /minio /usr/local/bin/minio
ENTRYPOINT ["/usr/local/bin/minio"]
CMD ["server", "/data", "--console-address", ":9001"]
```

```sh
docker build -f /tmp/sticker-sync-minio-source/Dockerfile.codex \
  -t sticker-sync-minio:test /tmp/sticker-sync-minio-source
docker run -d --name sticker-sync-minio -p 127.0.0.1:19000:9000 \
  -e MINIO_ROOT_USER=sync-test -e MINIO_ROOT_PASSWORD=sync-test-only \
  sticker-sync-minio:test
curl -fsS --aws-sigv4 'aws:amz:us-east-1:s3' \
  --user sync-test:sync-test-only -X PUT http://127.0.0.1:19000/stickers

SYNC_S3_ENDPOINT=http://127.0.0.1:19000 SYNC_S3_REGION=us-east-1 \
SYNC_S3_BUCKET=stickers SYNC_S3_ACCESS_KEY_ID=sync-test \
SYNC_S3_SECRET_ACCESS_KEY=sync-test-only SYNC_S3_CONTAINER=sticker-sync-minio \
cargo test -p sticker-core sync::tests::network_integration::s3_ \
  -- --ignored --nocapture --test-threads=1
```

每个测试在独立 UUID 路径中运行；断线测试必须使用专用容器、串行执行。

## 真实 macOS 窗口

Vite 由 `pnpm dev` 提供。将调试二进制分别复制进临时 `StickerSyncA.app` / `StickerSyncB.app`，bundle id 分别为 `com.shaojian.sticker-manager.devtesta` / `devtestb`；通过 `open -g -n ... --env STICKER_LIBRARY_DIR=...` 启动，仓库为临时根目录中的 `library-A` / `library-B`。

使用本机 Accessibility API 进行后台输入、点击及窗口检查，截图限定到测试应用窗口。只有测试原生文件选择框的“前往文件夹”快捷键时短暂激活过测试窗口一次，随后立即还原前台应用；该快捷键尝试没有成功，已取消文件选择框，改用临时 core 导入程序生成测试数据。Return 可以在后台发送；其余连接、编辑、原生确认和停止步骤没有全屏接管。

实际通过的步骤：

1. A 输入错误凭据，显示认证/写入探测失败；内容保留，密码仍为遮蔽输入；HTTP 地址下显示“密码会以明文传输”。失败后尚无该目标的钥匙串条目。
2. A 用测试凭据连接空目标并创建仓库。B 连接同一目标，原生确认显示 1 台设备、0 张表情，确认后加入。
3. 导入 10 张临时图片，A 在真实界面修改标签“网络同步测试”、配文“后台窗口同步验证”和收藏；B 同步后可见这些修改。
4. A 重启同一个 `.app` 二进制，不需重新输入凭据，继续同步。两实例加入、同一二进制重启、后续同步及停止，**实际观察到钥匙串授权弹窗共 0 次**。重新编译改变签名后访问已有条目的场景未验证，不能据此保证调试版永远不弹窗；代码缓存也包括读取失败。
5. 再导入 800 张临时图片。真实窗口出现“正在上传原图 309/800”。在后续 810 张原图续传轮次中点击停止并确认，至设置中的 sync 清空共 **0.987 秒**（包含确认操作）。停止后本地数据库仍有 810 张表情。
6. `security find-generic-password -s com.shaojian.sticker-manager.sync.debug -a <设备id:目标摘要>` 不带 `-w`：停止 A 后其查找退出码为 44，B 仍为 0，确认只删除自己的条目。随后也停止 B，清理测试凭据。
7. R2 表单实际展示 Account ID、Bucket、路径（可选）、Access Key ID、遮蔽的 Secret Access Key 和单一“连接”按钮；四项后端在当前窗口使用分段控件。

本机截图留在临时目录：`sync-connected.png`、`r2-preferences.png`。测试辅助程序没有加入项目源码。

测试完成后关闭两个临时实例与 Vite，删除本次创建的两个测试容器；临时仓库、截图和本地镜像保留供复查。

## 未验证与已知范围

- 没有使用真实 Cloudflare R2、坚果云或其他公网存储账号；R2 endpoint / region 映射有单测，S3 协议实际接入 MinIO。真实账号凭据应只由用户在应用表单中填写。
- 系统代理：显式启用 reqwest 的 `system-proxy`，保留客户端默认系统代理读取；检查了当前版本实现。此 macOS 的 HTTP、HTTPS、SOCKS 和 PAC 均未启用，因此未实际验证通过某个代理转发，也未修改用户代理设置。
- 未进行 Windows 编译和运行。没有实现按流量计费暂停、悬浮球/托盘暂停、独立同步设置页、同步详情、端到端加密、日志压缩或并发传输。
- 原有整份模型/全表投影的锁持有和日志积累问题未改动。
