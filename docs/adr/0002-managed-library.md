# 表情由应用托管，按内容去重

导入时把文件复制进应用自己的仓库，按内容哈希存放，和用户原来的文件不再关联；不采用"原地索引用户文件夹"的做法。原地索引的问题是：用户移动或删除文件后，记录就会失效；不同设备上的路径也对不上，没法同步。按内容存放后，同一张图只存一份，再次导入时会自动合并，同步时也不会出现同名文件冲突。

仓库固定放在系统数据目录下以应用标识符 `com.shaojian.sticker-manager` 命名的文件夹里：macOS 是 `~/Library/Application Support/com.shaojian.sticker-manager/`，Windows 是 `%APPDATA%\com.shaojian.sticker-manager\`。用标识符而不用显示名，改应用名时仓库位置不会变。不放在应用安装目录里，因为 macOS 的 `.app` 包在自动更新时会被整个替换，而 Windows 安装目录在卸载或更新时可能被清空。偏好设置里提供"打开仓库所在位置"按钮。
