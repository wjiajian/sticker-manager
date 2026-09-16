#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Match the sizes in AppIcon.appiconset/Contents.json using macOS's built-in tool.
for size in 16 32 64 128 256 512 1024; do
  /usr/bin/sips --setProperty format png --resampleHeightWidth "$size" "$size" \
    windows/runner/resources/app_icon.ico \
    --out "macos/Runner/Assets.xcassets/AppIcon.appiconset/app_icon_${size}.png" \
    > /dev/null
done
