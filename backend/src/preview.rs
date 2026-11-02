//! Ephemeral, independent HLS previews for desktop WebKit. No recording paths or side effects.
use crate::{config::settings::AppState, core::emitter::{Emitter, EmitterExt}, recording::hls::{get_url_prefix, parse_playlist}, streaming::platform::StreamApi};
use axum::{Router, body::Body, extract::{Path, State}, http::{HeaderMap, StatusCode, header}, response::{IntoResponse, Response}, routing::get};
use parking_lot::Mutex;
use serde::Serialize;
use std::{collections::{HashMap, HashSet, VecDeque}, future::Future, path::PathBuf, pin::Pin, process::Stdio, sync::{Arc, Weak, atomic::{AtomicBool, Ordering}}, time::Duration};
use tokio::{io::AsyncWriteExt, sync::{watch, Notify}, task::JoinHandle};

pub type SourceFuture = Pin<Box<dyn Future<Output = Result<(StreamApi, String), SourceError>> + Send>>;
/// Injectable for deterministic offline integration and native-window tests; never exposed by IPC.
pub type SourceResolver = Arc<dyn Fn(String) -> SourceFuture + Send + Sync>;
#[derive(Debug)]
pub struct SourceError { pub message: String, pub terminal: bool }
impl SourceError {
    pub fn retry(message: impl Into<String>) -> Self { Self { message: message.into(), terminal: false } }
    pub fn terminal(message: impl Into<String>) -> Self { Self { message: message.into(), terminal: true } }
}
#[derive(Clone, Serialize)]
pub struct PreviewStatus {
    pub preview_id: String, pub username: String, pub state: String, pub message: Option<String>,
}
#[derive(Serialize)]
pub struct PreviewStarted {
    pub preview_id: String, pub playlist_url: String, pub state: String,
}
struct Session {
    id: String, username: String, token: String, dir: PathBuf,
    cancel: watch::Sender<bool>, status: Mutex<PreviewStatus>, done: Notify, finished: AtomicBool, task: Mutex<Option<JoinHandle<()>>>,
}
#[derive(Default)]
struct Sessions { entries: HashMap<String, Arc<Session>>, cancelled: VecDeque<String>, closing: bool }
pub struct PreviewManager {
    sessions: Mutex<Sessions>, root: PathBuf, port: u16, emitter: Arc<dyn Emitter>, resolver: SourceResolver,
    server_stop: watch::Sender<bool>, server_task: Mutex<Option<JoinHandle<()>>>,
}
impl PreviewManager {
    pub fn public_resolver(state: Arc<AppState>) -> SourceResolver {
        let resolver: SourceResolver = Arc::new(move |username| {
            let state = state.clone();
            Box::pin(async move {
                if !state.get_streamers().iter().any(|s| s.username == username) {
                    return Err(SourceError::terminal("该主播已被移除"));
                }
                let settings = state.get_settings();
                let api = StreamApi::new_api_only(settings.api_proxy_url.as_deref(), settings.cdn_proxy_url.as_deref(), settings.sc_mirror_url.as_deref())
                    .map_err(|_| SourceError::retry("无法初始化直播连接"))?
                    .with_channel(&username).with_mouflon_keys(state.get_mouflon_keys()).with_resolution_selection(480, "lower");
                let info = api.get_stream_info(&username, true).await.map_err(|_| SourceError::retry("暂时无法获取直播地址"))?;
                if !info.is_recordable { return Err(SourceError::terminal("直播已结束或当前房间不可公开播放")); }
                let url = info.playlist_url.ok_or_else(|| SourceError::terminal("当前房间没有可播放的直播源"))?;
                let api = api.with_audio_playlist(&url, info.audio_playlist_url);
                Ok((api, url))
            })
        });
        resolver
    }
    pub async fn new(root: PathBuf, state: Arc<AppState>, emitter: Arc<dyn Emitter>) -> std::io::Result<Arc<Self>> {
        Self::with_resolver(root, emitter, Self::public_resolver(state)).await
    }
    pub async fn with_resolver(root: PathBuf, emitter: Arc<dyn Emitter>, resolver: SourceResolver) -> std::io::Result<Arc<Self>> {
        if root.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(std::io::Error::other("Preview cache must not be a symbolic link"));
        }
        if root.exists() { tokio::fs::remove_dir_all(&root).await?; }
        tokio::fs::create_dir_all(&root).await?;
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let port = listener.local_addr()?.port();
        let (server_stop, mut stopped) = watch::channel(false);
        let manager = Arc::new(Self { sessions: Mutex::new(Sessions::default()), root, port, emitter, resolver, server_stop, server_task: Mutex::new(None) });
        let router = Router::new().route("/preview/{token}/{file}", get(media)).with_state(Arc::downgrade(&manager));
        *manager.server_task.lock() = Some(tokio::spawn(async move {
            let _ = axum::serve(listener, router).with_graceful_shutdown(async move { let _ = stopped.changed().await; }).await;
        }));
        Ok(manager)
    }
    pub fn start(self: &Arc<Self>, username: &str, request_id: &str) -> Result<PreviewStarted, String> {
        let username = crate::streaming::identity::normalize(username).map_err(|e| e.to_string())?;
        if request_id.len() != 36 || !request_id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
            return Err("Invalid preview request identifier".into());
        }
        let mut sessions = self.sessions.lock();
        if sessions.closing || sessions.cancelled.iter().any(|id| id == request_id) { return Err("Preview cancelled".into()); }
        if sessions.entries.contains_key(request_id) { return Err("Preview request already exists".into()); }
        for old in sessions.entries.values().filter(|s| s.username == username) { old.cancel.send_replace(true); }
        let token = format!("{:032x}{:032x}", rand::random::<u128>(), rand::random::<u128>());
        let (cancel, rx) = watch::channel(false);
        let session = Arc::new(Session {
            id: request_id.into(), username: username.clone(), token: token.clone(), dir: self.root.join(request_id), cancel,
            status: Mutex::new(PreviewStatus { preview_id: request_id.into(), username, state: "connecting".into(), message: None }),
            done: Notify::new(), finished: AtomicBool::new(false), task: Mutex::new(None),
        });
        sessions.entries.insert(request_id.into(), session.clone());
        let manager = self.clone();
        let worker_session = session.clone();
        *session.task.lock() = Some(tokio::spawn(async move { manager.worker(worker_session, rx).await; }));
        Ok(PreviewStarted { preview_id: request_id.into(), playlist_url: format!("http://127.0.0.1:{}/preview/{token}/index.m3u8", self.port), state: "connecting".into() })
    }
    pub fn status(&self, id: &str) -> Option<PreviewStatus> { self.sessions.lock().entries.get(id).map(|s| s.status.lock().clone()) }
    pub fn active_count(&self) -> usize { self.sessions.lock().entries.len() }
    fn publish(&self, session: &Session, state: &str, message: Option<String>) {
        let event = PreviewStatus { preview_id: session.id.clone(), username: session.username.clone(), state: state.into(), message };
        *session.status.lock() = event.clone(); self.emitter.emit("live-preview-state", &event);
    }
    pub async fn stop(&self, id: &str) {
        let session = {
            let mut sessions = self.sessions.lock();
            if !sessions.cancelled.iter().any(|old| old == id) { sessions.cancelled.push_back(id.into()); }
            while sessions.cancelled.len() > 4096 { sessions.cancelled.pop_front(); }
            sessions.entries.get(id).cloned()
        };
        if let Some(session) = session { session.cancel.send_replace(true); self.wait(&session).await; }
    }
    async fn wait(&self, session: &Session) {
        let finished = tokio::time::timeout(Duration::from_secs(4), async {
            while !session.finished.load(Ordering::Acquire) {
                tokio::select! { _ = session.done.notified() => {}, _ = tokio::time::sleep(Duration::from_millis(25)) => {} }
            }
        }).await;
        if finished.is_err() {
            let task = session.task.lock().take();
            if let Some(task) = task { task.abort(); let _ = task.await; }
            self.sessions.lock().entries.remove(&session.id);
            let _ = tokio::fs::remove_dir_all(&session.dir).await;
            self.publish(session, "closed", None);
            session.finished.store(true, Ordering::Release);
        }
    }
    pub async fn stop_username(&self, username: &str) {
        let ids: Vec<_> = self.sessions.lock().entries.values().filter(|s| s.username == username).map(|s| s.id.clone()).collect();
        for id in ids { self.stop(&id).await; }
    }
    pub async fn stop_all(&self) {
        let sessions: Vec<_> = self.sessions.lock().entries.values().cloned().collect();
        for session in &sessions { session.cancel.send_replace(true); }
        for session in sessions { self.wait(&session).await; }
    }
    pub async fn shutdown(&self) {
        self.sessions.lock().closing = true;
        self.stop_all().await; self.server_stop.send_replace(true);
        let task = self.server_task.lock().take();
        if let Some(task) = task { let _ = task.await; }
    }
    async fn worker(self: Arc<Self>, session: Arc<Session>, mut cancel: watch::Receiver<bool>) {
        let result = async {
            tokio::fs::create_dir_all(&session.dir).await.map_err(|_| SourceError::terminal("无法创建直播缓存"))?;
            let mut retries = 0;
            loop {
                if *cancel.borrow() { return Ok(()); }
                let resolved = tokio::select! {
                    biased; _ = cancel.changed() => return Ok(()),
                    result = tokio::time::timeout(Duration::from_secs(20), (self.resolver)(session.username.clone())) =>
                        result.unwrap_or_else(|_| Err(SourceError::retry("连接直播超时"))),
                };
                let result = match resolved {
                    Ok((api, url)) => self.stream(&session, &api, &url, &mut cancel).await,
                    Err(error) => Err(error),
                };
                match result {
                    Ok(()) => return Ok(()),
                    Err(error) if error.terminal || retries == 3 => return Err(error),
                    Err(_) => {
                        retries += 1; self.publish(&session, "retrying", Some(format!("连接中断，正在重试（{retries}/3）")));
                        tokio::select! { _ = cancel.changed() => return Ok(()), _ = tokio::time::sleep(Duration::from_secs(1 << (retries - 1))) => {} }
                        // A new source may restart timestamps. Remove old segments before re-opening WebKit.
                        let _ = tokio::fs::remove_dir_all(&session.dir).await;
                        tokio::fs::create_dir_all(&session.dir).await.map_err(|_| SourceError::terminal("无法创建直播缓存"))?;
                    }
                }
            }
        }.await;
        if let Err(error) = result { self.publish(&session, "error", Some(error.message)); }
        else { self.publish(&session, "closed", None); }
        self.sessions.lock().entries.remove(&session.id);
        let _ = tokio::fs::remove_dir_all(&session.dir).await;
        session.finished.store(true, Ordering::Release); session.done.notify_waiters();
    }
    async fn stream(&self, session: &Session, api: &StreamApi, url: &str, cancel: &mut watch::Receiver<bool>) -> Result<(), SourceError> {
        let mut child = tokio::process::Command::new(crate::core::runtime::tool("ffmpeg"))
            .args(["-hide_banner", "-loglevel", "error", "-fflags", "+genpts", "-i", "pipe:0", "-map", "0:v:0", "-map", "0:a:0?", "-c", "copy", "-f", "hls", "-hls_time", "2", "-hls_list_size", "6", "-hls_delete_threshold", "1", "-hls_flags", "delete_segments+temp_file+omit_endlist+independent_segments", "-hls_segment_filename"])
            .arg(session.dir.join("seg%09d.ts")).arg(session.dir.join("index.m3u8"))
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).kill_on_drop(true).spawn()
            .map_err(|_| SourceError::terminal("无法启动直播播放器"))?;
        let mut input = child.stdin.take().ok_or_else(|| SourceError::terminal("无法连接播放器"))?;
        let stderr = child.stderr.take().ok_or_else(|| SourceError::terminal("无法读取播放器状态"))?;
        let error_task = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut output = Vec::new(); let _ = stderr.take(16384).read_to_end(&mut output).await;
            String::from_utf8_lossy(&output).into_owned()
        });
        let observing_cancel = cancel.clone();
        let feeder = async {
            let mut downloaded = HashSet::new(); let mut init = Vec::new(); let mut init_url = None;
            let mut last_progress = tokio::time::Instant::now(); let mut last_status = tokio::time::Instant::now();
            loop {
                if *observing_cancel.borrow() { return Ok(()); }
                if last_status.elapsed() >= Duration::from_secs(10) {
                    let _ = (self.resolver)(session.username.clone()).await?;
                    last_status = tokio::time::Instant::now();
                }
                api.prefetch_audio(url).await.map_err(|_| SourceError::retry("直播音轨连接中断"))?;
                let playlist = api.fetch_playlist(url).await.map_err(|_| SourceError::retry("直播连接中断"))?;
                let (segments, header) = parse_playlist(&playlist, &get_url_prefix(url), api.mouflon_keys()).map_err(|_| SourceError::terminal("当前直播格式暂不支持预览"))?;
                if header != init_url {
                    init = if let Some(ref header) = header { api.download_segment(header).await.map_err(|_| SourceError::retry("直播初始化失败"))? } else { Vec::new() };
                    init_url = header;
                }
                // Keep only the current upstream window's deduplication history.
                let sequences: HashSet<_> = segments.iter().map(|s| s.sequence).collect();
                downloaded.retain(|seq| sequences.contains(seq));
                // Preview starts near the live edge; old video segments may already have
                // fallen outside a separately fetched audio playlist's moving window.
                if downloaded.is_empty() {
                    downloaded.extend(segments.iter().take(segments.len().saturating_sub(3)).map(|s| s.sequence));
                }
                for segment in &segments {
                    if downloaded.contains(&segment.sequence) { continue; }
                    let mut bytes = init.clone();
                    bytes.extend(api.download_segment(&segment.url).await.map_err(|_| SourceError::retry("直播片段下载失败"))?);
                    let bytes = match api.prepare_av_segment(url, segment.sequence, bytes).await {
                        Ok(bytes) => bytes,
                        Err(error) if error.to_string().contains("not in live window") => {
                            downloaded.insert(segment.sequence);
                            continue;
                        }
                        Err(error) => {
                            let detail = regex::Regex::new(r"https?://\S+").unwrap().replace_all(&error.to_string(), "<stream-url>").into_owned();
                            tracing::warn!("Preview AV preparation: {}", detail);
                            return Err(SourceError::retry("直播音视频处理失败"));
                        }
                    };
                    input.write_all(&bytes).await.map_err(|_| SourceError::terminal("当前直播编码无法播放"))?;
                    downloaded.insert(segment.sequence); last_progress = tokio::time::Instant::now();
                }
                if last_progress.elapsed() > Duration::from_secs(20) { return Err(SourceError::retry("直播长时间没有新画面")); }
                // Bound each preview cache even when the upstream has huge keyframe intervals.
                if cache_size(&session.dir) > 64 * 1024 * 1024 { return Err(SourceError::terminal("当前直播缓冲过大，请重新观看")); }
                tokio::time::sleep(Duration::from_millis(350)).await;
            }
        };
        let readiness = async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
            loop {
                if tokio::fs::read_to_string(session.dir.join("index.m3u8")).await.is_ok_and(|s| s.contains("#EXTINF:")) {
                    self.publish(session, "ready", None);
                    return std::future::pending::<()>().await;
                }
                if tokio::time::Instant::now() >= deadline { return; }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        // Cancellation always reaches the FFmpeg owner; do not drop its supervising future.
        let result = tokio::select! {
            biased; _ = cancel.changed() => Ok(()), result = feeder => result,
            _ = readiness => Err(SourceError::retry("直播播放器启动超时")),
            _ = child.wait() => Err(SourceError::terminal("当前直播编码无法播放")),
        };
        drop(input);
        let _ = child.kill().await; let _ = child.wait().await;
        let stderr = error_task.await.unwrap_or_default();
        if result.is_err() && !stderr.is_empty() { tracing::debug!("Preview converter stopped ({} bytes of diagnostics)", stderr.len()); }
        result
    }
}
fn cache_size(path: &std::path::Path) -> u64 {
    std::fs::read_dir(path).into_iter().flatten().filter_map(|e| e.ok()).filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()
}
async fn media(State(manager): State<Weak<PreviewManager>>, Path((token, file)): Path<(String, String)>, headers: HeaderMap) -> Response {
    if let Some(origin) = headers.get(header::ORIGIN) {
        if origin != "tauri://localhost" && origin != "http://localhost:1420" { return StatusCode::FORBIDDEN.into_response(); }
    }
    let valid_segment = file.strip_prefix("seg").and_then(|s| s.strip_suffix(".ts")).is_some_and(|n| n.len() == 9 && n.bytes().all(|c| c.is_ascii_digit()));
    if file != "index.m3u8" && !valid_segment { return StatusCode::NOT_FOUND.into_response(); }
    let Some(manager) = manager.upgrade() else { return StatusCode::GONE.into_response(); };
    let session = manager.sessions.lock().entries.values().find(|s| s.token == token && !*s.cancel.borrow()).cloned();
    let Some(session) = session else { return StatusCode::NOT_FOUND.into_response(); };
    let bytes = match tokio::fs::read(session.dir.join(&file)).await { Ok(b) => b, Err(_) => return StatusCode::NOT_FOUND.into_response() };
    let content_type = if file == "index.m3u8" { "application/vnd.apple.mpegurl" } else { "video/mp2t" };
    ([(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "no-store"), (header::ACCESS_CONTROL_ALLOW_ORIGIN, "tauri://localhost")], Body::from(bytes)).into_response()
}
