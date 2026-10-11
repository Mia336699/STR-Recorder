# STR Recorder

![STR Recorder icon](assets/str-recorder-merged-logo-v1.png)

面向 Apple Silicon Mac 的独立直播录制桌面应用，基于 [ChanTrail/StripchatRecorder](https://github.com/ChanTrail/StripchatRecorder) 二次开发。支持 Stripchat 与 Chaturbate 公开直播，保留原有录制、自动监测与后处理功能。

## 安装

在本仓库 Releases 下载 `STR-Recorder-0.3.5-Chaturbate-merged-logo-arm64.dmg`，将 STR Recorder 拖到 Applications 后启动。

- Apple Silicon，macOS 26 或以上；暂不支持 Intel Mac。
- 随包提供 FFmpeg、ffprobe 和四个后处理模块，运行时不需要 Node.js、Rust、Homebrew 或网页服务。
- 首次启动选择自己的录制目录，在添加主播时选择平台，或直接粘贴公开房间链接。
- 当前构建使用本地 ad-hoc 签名，尚未进行 Apple Developer ID 签名和公证。

## 功能

- Stripchat 原有录制功能与 Chaturbate 公开房间录制。
- 手动录制与上线自动录制，支持首选分辨率、分段和停止后的合并。
- Chaturbate 独立音视频 HLS 轨道复用，平台同名主播使用独立文件夹。
- 卡片缩略图使用当前站点 CDN，图片失败时显示占位并在更新后重试。
- 通过 ⌘Q、应用菜单或关闭主窗口退出时，等待录制停止、合并和后处理完成。
- 合并失败保留源分片，启动时恢复未完成的录制。
- 分开配置 API 和 CDN 代理；Stripchat 镜像设置继续仅影响 Stripchat。

仅处理无需登录即可访问的公开直播；私密、付费、密码房间以及访问验证不会绕过。

## 从源码构建

需要 Apple Silicon Mac、Xcode Command Line Tools、Rust stable、Node.js 22 和 Homebrew FFmpeg。FFmpeg 的最低系统版本必须与打包目标一致；当前发布目标为 macOS 26。

```sh
npm ci --prefix desktop
scripts/package-macos.sh
```

安装包输出到 `build/desktop-macos/`。构建脚本将 FFmpeg 和依赖库放入应用资源，重写动态库路径并进行本地签名。用户配置与日志位于 `~/Library/Application Support/com.mia.str-recorder/`，更新应用不会覆盖用户数据。

## 验证

```sh
npm run build --prefix desktop
cargo test --release --manifest-path backend/Cargo.toml --lib --test public_hls_recording -- --include-ignored
```

显式媒体测试需要 FFmpeg 和 ffprobe。2026-10-11 的本机验收完成 12 项测试、约 41.5 秒 Chaturbate 1080p H.264/AAC 公开直播录制及完整解码，并验证退出合并和启动恢复。融合图标已生成 macOS 多尺寸 ICNS；安装包经过签名与磁盘映像校验。未完成长时间压力测试或通知发送测试。

技术说明见 [Chaturbate 适配](docs/CHATABURATE.md) 和 [macOS 安装说明](docs/MACOS.md)。

## 来源与许可

基于上游提交 `7519a9314e9e50e9ce17774299d9f76c7f5d8347`（0.3.5）。项目遵循 [GPL-3.0](LICENSE)，保留原作者署名与源码历史。内置 FFmpeg 的许可和构建来源包含在应用资源中的 `licenses/`。本仓库是二次开发版本，不代表上游官方发布。
