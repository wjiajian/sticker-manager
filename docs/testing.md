# 测试

## 日常检查

应用代码没有自动触发的 CI，提交前在本地跑：

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm --dir frontend typecheck
pnpm --dir frontend build
pnpm --dir website build
git diff --check
```

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

### Windows 悬浮球大小回归

在仓库根目录执行：

```powershell
pnpm --dir frontend tauri build --debug --no-bundle
node .github/scripts/ball-window.test.mjs
```

脚本使用临时仓库与独立 WebView2 数据目录，通过仅本机可用的调试端口在后台操作隐藏的主窗口，不移动系统鼠标；测试结束时关闭测试进程。覆盖实时预览、松手保存、键盘、恢复默认、Esc 取消、重启持久化，并延迟一次预览请求验证连续提交和后续取消不会覆盖已排队的保存。结果和截图写入 `target/ball-window-results/`。

### Windows 拖入格式诊断

Debug 构建可设置 `STICKER_DROP_DIAGNOSTICS=1`，在标准错误输出中记录 OLE 格式名称、编号、介质和 HRESULT；不记录文件名、路径或图片内容。仍须将 `STICKER_LIBRARY_DIR` 设为临时目录。Release 构建忽略这个诊断开关。

应分别测试文件路径、虚拟文件、PNG/DIB，以及聊天软件只允许内部拖动的表情。`QueryGetData` 必须返回 `S_OK` 才算支持；`S_FALSE` 不能显示“松开导入”。来源未允许复制时也不能接受拖入。

对应回归测试：`cargo test -p sticker-manager windows_drop::tests --locked`。测试涵盖格式协商、后台读取、临时文件生命周期、图片像素、动画原文件和导入去重；真实聊天来源的导出限制仍需人工验证。
