# 表情由应用托管，按内容去重

导入时把文件复制进应用自己的仓库，按内容哈希存放，和用户原来的文件不再关联；不采用"原地索引用户文件夹"的做法。原地索引的问题是：用户移动或删除文件后，记录就会失效；不同设备上的路径也对不上，没法同步。按内容存放后，同一张图只存一份，再次导入时会自动合并，同步时也不会出现同名文件冲突。

安装版仓库放在系统数据目录：macOS 保持 `~/Library/Application Support/com.wjiajian.sticker-manager/`，Windows 使用 `%APPDATA%\sticker-manager\`，不跟随显示名称变化。Windows 早期曾使用 `%APPDATA%\com.wjiajian.sticker-manager\`；0.1.3 及以前的标识符为 `com.shaojian.sticker-manager`。

新目录不存在或为空时，Windows 优先迁移有内容的 `com.wjiajian.sticker-manager`，再查找 `com.shaojian.sticker-manager`；空的当前标识符目录不能遮住更早的仓库。迁移通过同一父目录内的原子改名完成，并用跨进程锁串行化并发启动。元数据、目录读取或改名失败时报告启动错误，不绕过不可访问的旧仓库打开空仓库。macOS 仍按原标识符迁移规则查找 `com.shaojian.sticker-manager`。偏好设置提供“打开仓库所在位置”。

安装版不把仓库放在安装目录，因为 macOS 的 `.app` 包在自动更新时会被整个替换，Windows 安装目录在卸载或更新时也可能被清空。Windows 便携版是明确的例外：程序旁存在 `portable.flag` 时，仓库放在同目录的 `data`，缓存放在 `webview`，不迁移安装版数据、不登记开机自启、不使用安装式更新；移动或升级时保留这些文件夹。
