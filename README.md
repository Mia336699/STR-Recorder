# STR Recorder

![STR Recorder icon](assets/str-recorder-merged-logo-v1.png)

面向 Apple Silicon Mac 的独立直播录制桌面应用，基于 [ChanTrail/StripchatRecorder](https://github.com/ChanTrail/StripchatRecorder) 二次开发。支持 Stripchat 与 Chaturbate 公开直播，保留原有录制、自动监测与后处理功能。

## 安装

在本仓库 Releases 下载 `STR-Recorder-0.3.8-arm64.dmg`，将 STR Recorder 拖到 Applications 后启动。

- Apple Silicon，macOS 26 或以上；暂不支持 Intel Mac。
- 随包提供 FFmpeg、ffprobe 和四个后处理模块，运行时不需要 Node.js、Rust、Homebrew 或网页服务。
- 首次启动选择自己的录制目录，在添加主播时选择平台，或直接粘贴公开房间链接。
- 当前构建使用本地 ad-hoc 签名，尚未进行 Apple Developer ID 签名和公证。

## 功能

- 深色桌面工作台，支持浅色和系统主题。主播搜索、状态筛选、在线优先、紧凑布局，录制文件按组整理。
- 内置录制组 / 所选片段顺序播放，媒体详情、实际开始时间和后处理结果独立查看。
- 设置按通用、录制、网络和平台分区，支持 macOS 目录选择；密钥默认遮蔽，修改失败可重试。

- 点击已添加的在线公开主播画面，在卡片内观看直播，再次点击关闭；可同时观看多个房间。默认静音，打开一路声音时其他房间自动静音。切换页面自动关闭观看，录制继续运行。

- 新录制采用 `用户名_YYYYMMDD_HHMMSS_part01.mp4`，同组固定时间前缀与两位分段编号，按名称升序即可顺序播放；旧文件不改名。

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

0.3.8 本地通过 8 项前端检查、22 项后端及后处理测试；已在独立原生窗口验证四路合成预览、关闭观看继续录制、顺序播放、保存失败重试、首次启动、980×600 布局及退出合并。新增验收范围与限制见[UI 验收记录](docs/qa/v0.3.8-ui-progress.md)。本轮名称查找访问成功；图片匹配只验证发送前取消，未发送测试图片。真实直播 CDN 的 0.3.7 记录属于历史结果，不据此声称本轮两站点实时验收通过。

0.3.7 通过 21 项本地测试，并在实际 Tauri 窗口验证四路同时预览、单路声音、快速开关和切页清理；关闭预览后模拟录制仍继续。真实 Chaturbate 公开直播在原生窗口持续播放超过一分钟，H.264/AAC 分段完整解码通过。Stripchat 后续对照测试确认隔离测试缺少现有取流密钥；加载相同配置后，两个公开房间取片及完整解码通过，原生窗口持续播放超过一分钟。详见[Stripchat 补测记录](docs/qa/v0.3.7-stripchat-followup.json)。详见[验收记录](docs/qa/v0.3.7-verification.json)。

0.3.6 的本地验收通过 15 项后端单元测试、2 项真实录制器模拟流集成测试和 1 项后处理命名兼容测试；实际拆分 16 段并验证逐段完整解码、09→10 排序、中断恢复与新旧文件混合显示。预览图模块已用新文件名实际生成图片；通知发送未执行。

```sh
npm run build --prefix desktop
cargo test --release --manifest-path backend/Cargo.toml --lib --test public_hls_recording -- --include-ignored
```

显式媒体测试需要 FFmpeg 和 ffprobe。2026-10-11 的本机验收完成 12 项测试、约 41.5 秒 Chaturbate 1080p H.264/AAC 公开直播录制及完整解码，并验证退出合并和启动恢复。融合图标已生成 macOS 多尺寸 ICNS；安装包经过签名与磁盘映像校验。未完成长时间压力测试或通知发送测试。

技术说明见 [Chaturbate 适配](docs/CHATABURATE.md) 和 [macOS 安装说明](docs/MACOS.md)。

## 来源与许可

基于上游提交 `7519a9314e9e50e9ce17774299d9f76c7f5d8347`（0.3.5）。项目遵循 [GPL-3.0](LICENSE)，保留原作者署名与源码历史。内置 FFmpeg 的许可和构建来源包含在应用资源中的 `licenses/`。本仓库是二次开发版本，不代表上游官方发布。
