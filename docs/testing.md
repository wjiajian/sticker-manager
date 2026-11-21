# 测试

## 日常检查

应用 PR 会触发「验证应用」GitHub Actions，在 Windows 上执行下列检查并构建 Debug 应用；macOS 当前只做编译、格式、Clippy 和前端检查，不运行 Rust 测试或真实软件拖入测试。工作流也支持手动触发。需要完整安装包时使用「构建 Windows 安装包」或「构建 macOS 安装包」工作流，保持「发布到 Release」关闭。

本地具备构建环境时也可运行：

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm --dir frontend typecheck
node --test frontend/tests/ballSizePreview.test.mjs
pnpm --dir frontend build
pnpm --dir website build
git diff --check
```

Windows Actions 还会启动 Debug 应用，以临时 `STICKER_LIBRARY_DIR` 和独立 WebView2 数据目录运行 `.github/scripts/ball-window.test.mjs`，验证悬浮球大小预览不写盘、松手保存、键盘调节、恢复默认、Esc 取消和重启保持，并上传截图与日志。OLE 合成来源测试覆盖 PNG、DIB、虚拟文件流的接收与入库；这些测试不代表微信、飞书真实客户端或多显示器 DPI 拖动已验收。

## 网络同步的容器集成测试

WebDAV 和 S3 兼容后端有默认忽略的集成测试（`crates/core/src/sync/tests/network_integration.rs`），要连本机容器跑。地址和凭据都从环境变量读，下面的账号只是专用容器的测试值。必须串行执行（`--test-threads=1`），断线测试会停止再启动同一个容器，所以容器不能加 `--rm`。

### WebDAV

```bash
docker run -d --name sticker-sync-webdav -p 127.0.0.1:18080:8080 \
  rclone/rclone:latest serve webdav /data --addr :8080 --user sync-test --pass sync-test-only

SYNC_WEBDAV_URL=http://127.0.0.1:18080 \
SYNC_WEBDAV_USER=sync-test SYNC_WEBDAV_PASSWORD=sync-test-only \
SYNC_WEBDAV_CONTAINER=sticker-sync-webdav \
cargo test -p sticker-core sync::tests::network_integration::webdav_ \
  -- --ignored --nocapture --test-threads=1
```

### S3（MinIO）

任何 S3 兼容服务都行。用 MinIO 的话，先起一个容器并建好桶，再跑测试：

```bash
docker run -d --name sticker-sync-minio -p 127.0.0.1:19000:9000 \
  -e MINIO_ROOT_USER=sync-test -e MINIO_ROOT_PASSWORD=sync-test-only \
  <MinIO 镜像> server /data
curl -fsS --aws-sigv4 'aws:amz:us-east-1:s3' \
  --user sync-test:sync-test-only -X PUT http://127.0.0.1:19000/stickers

SYNC_S3_ENDPOINT=http://127.0.0.1:19000 SYNC_S3_REGION=us-east-1 \
SYNC_S3_BUCKET=stickers SYNC_S3_ACCESS_KEY_ID=sync-test \
SYNC_S3_SECRET_ACCESS_KEY=sync-test-only SYNC_S3_CONTAINER=sticker-sync-minio \
cargo test -p sticker-core sync::tests::network_integration::s3_ \
  -- --ignored --nocapture --test-threads=1
```

## 真实窗口测试

- 测试实例一律用 `STICKER_LIBRARY_DIR` 指向临时目录（绝对路径），不要动真实仓库。发布版忽略这个变量，试运行打包版的做法见[打包和发布](packaging.md#试运行发布版)。
- 优先在后台操作目标窗口，不抢前台、不移动用户的鼠标；只有后台做不了的步骤（比如跨应用拖放）才短暂接管屏幕，并在报告里写明。
- 不修改系统设置（低数据模式、减少动态效果、外观等）。需要的话请用户自己切换；用户不切换就把这项记为跳过。

完整约定见 [AGENTS.md](../AGENTS.md)。
