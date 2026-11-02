# 直播预览开发验收

生产模式：预览由独立的 PreviewManager 管理，通过只读的随机本机媒体端口向原生 WebKit 提供 HLS。站点客户端处理 API、CDN 和音视频适配；预览不使用录制目录或录制任务。Tauri 开始/停止命令仅作用于已添加的房间。

## 本地模拟流集成

```sh
cargo test --release --manifest-path backend/Cargo.toml --lib --test live_preview --test public_hls_recording -- --include-ignored
cargo test --manifest-path modules/pp_utils/Cargo.toml
```

需要 FFmpeg / ffprobe 在 PATH 中。覆盖四路同时预览、关闭一路、取消待连接请求、移除房间、最多三次重试、音频窗口错位、缓存清理，以及预览与实际录制互不影响。测试使用独立临时目录，不读取应用用户配置。

## 原生窗口验收

设置 `STR_PREVIEW_HARNESS=1` 构建 desktop 前端，再用 `cargo build --release --manifest-path desktop/src-tauri/Cargo.toml --features tauri/custom-protocol --example preview_harness` 编译专用验收程序。将其放入单独的 macOS app 包，提供独立标识、Info.plist 的本机媒体访问配置，并将随包工具复制到 `Contents/Resources/bin/`。验收程序默认在系统临时目录创建自己的配置、媒体和缓存。

验收页面直接使用生产卡片、预览状态管理和 Tauri 命令，额外显示 HTMLMediaElement 的播放时间、解码帧数、有声数量及后端会话数。可在窗口内观察四路画面和声音互斥，测试离开/返回列表、快速开关，并输入公开房间链接进行真实站点验收。

该程序和页面仅为显式开发验收使用；正式打包脚本会移除 STR_PREVIEW_HARNESS 环境标志，不将测试入口和测试命令放入发行应用。真实房间诊断亦可使用 `backend/examples/preview_probe.rs`，它使用隔离配置且不输出签名源地址。
