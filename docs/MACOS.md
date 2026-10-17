# macOS 安装说明

适用于 Apple Silicon，macOS 26 或以上。

下载 Releases 中的 DMG，将 STR Recorder 拖到 Applications，然后启动。首次启动选择自己的录制目录。在主播页选择 Stripchat 或 Chaturbate，可粘贴公开房间链接。

运行不需要启动网页服务，也不需要安装 Node.js、Rust、Homebrew 或 FFmpeg。用户设置、日志和模块位于 `~/Library/Application Support/com.mia.str-recorder/`，默认视频位于该目录的 recordings 子目录，可以在设置中更换。更新应用不会覆盖用户数据。

使用 ⌘Q、应用菜单中的“退出 STR Recorder”或关闭主窗口退出，会等待下载、合并及后处理完成。强制结束进程后，未完成的分片会在下次启动时尝试恢复。

当前应用采用 ad-hoc 签名，尚未经过 Apple Developer ID 签名和公证。从网络下载后 macOS 可能阻止首次启动，可在“系统设置 → 隐私与安全性”中查看针对该应用的提示。

仅支持可正常访问的公开直播。网站状态、网络连接和接口变化会影响录制；不绕过登录、访问验证或付费限制。Chaturbate 缩略图使用当前 CDN，并在状态轮询时更新；加载失败会显示用户名首字母。

项目 GPL-3.0；内置 FFmpeg 为 GPL 构建，源码来源及依赖信息见应用资源中的 licenses 目录。

## 录制文件命名（0.3.6）

新录制使用 `用户名_YYYYMMDD_HHMMSS_part01.mp4`，同一次录制的时间前缀固定，按设置的录制时长拆分后递增为 `part02`、`part03`，跨午夜不重置。播放器按文件名升序排列即可按顺序播放。Chaturbate 文件名去掉内部的 `chaturbate~` 前缀，两个平台仍使用各自目录。

超过 99 段时以新的时间前缀开始下一组；手动停止后重开也开始新组。同秒重开或已存在同名文件时，组标签选择下一个未占用的秒，不等待时钟，也不覆盖旧文件。元数据中的 `started_at` 保留各段真实开始时间，`group_started_at` 与 `part_index` 记录分组及编号。断流产生的时间缺口不会通过编号补齐。此规则仅对新录制生效，旧文件保持原名并继续兼容。

升级时，已知原版内置模块会自动更新为支持新命名的版本；同名但内容不同的自定义模块会保留。
