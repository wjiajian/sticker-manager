# 测试

## 日常检查

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

测试实例一律用 `STICKER_LIBRARY_DIR` 指向临时目录，不要动真实仓库。发布版忽略这个变量，试运行打包版的做法见[打包说明](packaging.md)。拖拽相关的功能靠日常使用来发现问题。
