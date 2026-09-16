#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

flutter build macos --release --no-pub
mkdir -p dist
# ditto preserves executable permissions, framework symlinks and bundle metadata.
ditto -c -k --sequesterRsrc --keepParent \
  build/macos/Build/Products/Release/sticker_manager.app \
  dist/sticker-manager-macos.zip
