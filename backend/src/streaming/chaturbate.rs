//! Chaturbate public-room adapter. Access challenges and unknown responses are errors.
use super::stripchat::{StreamInfo, StripchatApi};
use crate::core::error::{AppError, Result};
use regex::Regex;
use reqwest::Client;
use serde_json::Value;
use std::{collections::HashMap, process::Stdio, sync::LazyLock};
use tokio::io::AsyncWriteExt;

static DOSSIER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:window\.)?initialRoomDossier\s*=\s*("(?:\\.|[^"\\])*")"#).unwrap()
});

type AudioSegmentCache = HashMap<(String, u32), (String, Option<String>)>;

pub struct ChaturbateApi {
    api: Client,
    cdn: Client,
    audio_playlists: parking_lot::Mutex<HashMap<String, Option<String>>>,
    audio_segments: parking_lot::Mutex<AudioSegmentCache>,
    audio_inits: parking_lot::Mutex<HashMap<String, Vec<u8>>>,
}
impl ChaturbateApi {
    pub fn new(api_proxy: Option<&str>, cdn_proxy: Option<&str>) -> Result<Self> {
        Ok(Self {
            api: super::stripchat::build_client(api_proxy)?,
            cdn: super::stripchat::build_client(cdn_proxy)?,
            audio_playlists: parking_lot::Mutex::new(HashMap::new()),
            audio_segments: parking_lot::Mutex::new(HashMap::new()),
            audio_inits: parking_lot::Mutex::new(HashMap::new()),
        })
    }
    pub async fn get_stream_info(
        &self,
        name: &str,
        fetch: bool,
        resolution: u32,
        higher: bool,
    ) -> Result<StreamInfo> {
        // Public AJAX endpoint used by the site player. Do not provide session cookies
        // or attempt to bypass authentication, geo restrictions, or access challenges.
        let response = self
            .api
            .post("https://chaturbate.com/get_edge_hls_url_ajax/")
            .header("Referer", format!("https://chaturbate.com/{name}/"))
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Accept", "application/json")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(format!("room_slug={name}"))
            .send()
            .await?
            .error_for_status()?;
        let body = response.text().await?;
        let mut info = match serde_json::from_str::<Value>(&body) {
            Ok(mut room) if room["room_status"].is_string() => {
                room["hls_source"] = room["url"].clone();
                parse_room_data(&room)?
            }
            _ => {
                let response = self
                    .api
                    .get(format!("https://chaturbate.com/{name}/"))
                    .header("Referer", "https://chaturbate.com/")
                    .send()
                    .await?;
                if response.status() == reqwest::StatusCode::NOT_FOUND {
                    return Err(AppError::UserNotFound(name.into()));
                }
                parse_room(&response.error_for_status()?.text().await?)?
            }
        };
        // Use the site's current thumbnail CDN directly: the legacy
        // highwebmedia hostname has an expired certificate. Rotate the URL
        // with status polls so a live card does not keep its first snapshot.
        let snapshot = chrono::Utc::now().timestamp() / 30;
        info.thumbnail_url = Some(format!(
            "https://thumb.live.mmcdn.com/ri/{name}.jpg?t={snapshot}"
        ));
        if fetch && info.is_recordable {
            let source = info
                .playlist_url
                .take()
                .ok_or_else(|| AppError::Other("Public room has no HLS source".into()))?;
            let master = self.fetch_playlist(&source).await?;
            let video = select_playlist(&master, &source, resolution, higher)?;
            let audio = select_audio_playlist(&master, &source, &video)?;
            self.audio_playlists
                .lock()
                .insert(video.clone(), audio.clone());
            info.audio_playlist_url = audio;
            info.playlist_url = Some(video);
        } else {
            info.playlist_url = None;
        }
        Ok(info)
    }
    pub fn cache_audio_playlist(&self, video: &str, audio: Option<String>) {
        self.audio_playlists.lock().insert(video.into(), audio);
    }
    pub async fn prepare_av_segment(
        &self,
        name: &str,
        video_url: &str,
        sequence: u32,
        video: Vec<u8>,
        resolution: u32,
        higher: bool,
    ) -> Result<Vec<u8>> {
        let cached = self.audio_playlists.lock().get(video_url).cloned();
        let audio_url = if let Some(cached) = cached {
            cached
        } else {
            let info = self.get_stream_info(name, true, resolution, higher).await?;
            let selected = info
                .playlist_url
                .ok_or_else(|| AppError::StreamOffline(name.into()))?;
            let audio = self
                .audio_playlists
                .lock()
                .get(&selected)
                .cloned()
                .flatten();
            self.audio_playlists
                .lock()
                .insert(video_url.into(), audio.clone());
            audio
        };
        let Some(audio_url) = audio_url else {
            return Ok(video);
        };
        let key = (audio_url.clone(), sequence);
        if !self.audio_segments.lock().contains_key(&key) {
            self.refresh_audio_segments(&audio_url).await?;
        }
        let (segment, init) = self
            .audio_segments
            .lock()
            .get(&key)
            .cloned()
            .ok_or_else(|| {
                AppError::Other(format!(
                    "Audio segment {sequence} not in live window; retrying video"
                ))
            })?;
        let mut audio = if let Some(init) = init {
            let cached = self.audio_inits.lock().get(&init).cloned();
            if let Some(data) = cached {
                data
            } else {
                let data = self.download_segment(&init).await?;
                self.audio_inits.lock().insert(init, data.clone());
                data
            }
        } else {
            Vec::new()
        };
        audio.extend(self.download_segment(&segment).await?);
        mux_av_segment(video, audio).await
    }

    pub async fn prefetch_audio(&self, video: &str) -> Result<()> {
        let audio = self.audio_playlists.lock().get(video).cloned().flatten();
        if let Some(audio) = audio {
            self.refresh_audio_segments(&audio).await?;
        }
        Ok(())
    }
    async fn refresh_audio_segments(&self, audio: &str) -> Result<()> {
        let text = self.fetch_playlist(audio).await?;
        let (segments, init) = crate::recording::hls::parse_playlist(
            &text,
            &crate::recording::hls::get_url_prefix(audio),
            &HashMap::new(),
        )?;
        let mut cache = self.audio_segments.lock();
        if cache.len() > 64 {
            cache.clear();
        }
        for segment in segments {
            cache.insert(
                (audio.into(), segment.sequence),
                (segment.url, init.clone()),
            );
        }
        Ok(())
    }
    pub async fn fetch_playlist(&self, url: &str) -> Result<String> {
        let text = self
            .cdn
            .get(url)
            .header("Referer", "https://chaturbate.com/")
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        if !text.trim_start().starts_with("#EXTM3U") {
            return Err(AppError::Other(
                "Chaturbate CDN did not return an HLS playlist".into(),
            ));
        }
        Ok(text)
    }
    pub async fn download_segment(&self, url: &str) -> Result<Vec<u8>> {
        Ok(self
            .cdn
            .get(url)
            .header("Referer", "https://chaturbate.com/")
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec())
    }
}

pub fn parse_room(html: &str) -> Result<StreamInfo> {
    let captures = DOSSIER.captures(html).ok_or_else(|| {
        AppError::Other(
            "Chaturbate room dossier missing: possible access challenge or changed page format"
                .into(),
        )
    })?;
    let encoded: String = serde_json::from_str(&captures[1])?;
    let room: Value = serde_json::from_str(&encoded)?;
    parse_room_data(&room)
}

fn parse_room_data(room: &Value) -> Result<StreamInfo> {
    let state = room["room_status"]
        .as_str()
        .ok_or_else(|| AppError::Other("Chaturbate room_status missing".into()))?;
    let (online, recordable, status) = match state {
        "public" => (true, true, "公开秀"),
        "private" | "private_show" => (true, false, "私密秀"),
        "group" | "group_show" => (true, false, "群组秀"),
        "hidden" | "away" | "password protected" => (true, false, "等待"),
        "offline" => (false, false, "离线"),
        _ => {
            return Err(AppError::Other(format!(
                "Unknown Chaturbate room state: {state}"
            )));
        }
    };
    let source = room["hls_source"].as_str().filter(|s| !s.is_empty());
    if recordable && source.is_none() {
        return Err(AppError::Other(
            "Chaturbate public HLS unavailable: possibly geo restricted".into(),
        ));
    }
    if let Some(source) = source {
        let url = reqwest::Url::parse(source)
            .map_err(|_| AppError::Other("Invalid HLS source URL".into()))?;
        if url.scheme() != "https" || url.host_str().is_none() {
            return Err(AppError::Other(
                "Chaturbate requires an HTTPS HLS source".into(),
            ));
        }
    }
    Ok(StreamInfo {
        is_online: online,
        is_recordable: recordable,
        viewers: room["num_viewers"]
            .as_i64()
            .or_else(|| room["num_viewers"].as_str()?.parse().ok())
            .unwrap_or(0),
        status: status.into(),
        thumbnail_url: None,
        audio_playlist_url: None,
        playlist_url: if recordable {
            source.map(str::to_owned)
        } else {
            None
        },
    })
}

pub fn select_playlist(text: &str, source: &str, resolution: u32, higher: bool) -> Result<String> {
    if !text.trim_start().starts_with("#EXTM3U") {
        return Err(AppError::Other("Invalid HLS playlist".into()));
    }
    if !text.contains("#EXT-X-STREAM-INF:") {
        if text.contains("#EXTINF:") {
            return Ok(source.into());
        }
        return Err(AppError::Other("HLS playlist contains no streams".into()));
    }
    let (variant, _) = StripchatApi::parse_stream(text, resolution, higher)
        .ok_or_else(|| AppError::Other("No matching Chaturbate HLS variant".into()))?;
    let base = reqwest::Url::parse(source).map_err(|e| AppError::Other(e.to_string()))?;
    let url = base
        .join(&variant)
        .map_err(|e| AppError::Other(e.to_string()))?;
    if url.scheme() != "https" {
        return Err(AppError::Other("Invalid HLS variant scheme".into()));
    }
    Ok(url.into())
}

fn attribute(line: &str, key: &str) -> Option<String> {
    Regex::new(&format!(r#"(?:^|[:,]){}=\"([^\"]*)\""#, regex::escape(key)))
        .ok()?
        .captures(line)?
        .get(1)
        .map(|m| m.as_str().to_owned())
}

fn select_audio_playlist(master: &str, source: &str, video: &str) -> Result<Option<String>> {
    let base = reqwest::Url::parse(source).map_err(|e| AppError::Other(e.to_string()))?;
    let mut attrs = None;
    let mut group = None;
    for line in master.lines().map(str::trim) {
        if line.starts_with("#EXT-X-STREAM-INF:") {
            attrs = Some(line);
        } else if !line.is_empty()
            && !line.starts_with('#')
            && let Some(attributes) = attrs.take()
            && base.join(line).ok().as_ref().map(|u| u.as_str()) == Some(video)
        {
            group = attribute(attributes, "AUDIO");
            break;
        }
    }
    let Some(group) = group else {
        return Ok(None);
    };
    let rendition = master
        .lines()
        .find(|line| {
            line.starts_with("#EXT-X-MEDIA:TYPE=AUDIO")
                && attribute(line, "GROUP-ID").as_deref() == Some(&group)
        })
        .ok_or_else(|| AppError::Other("HLS audio group is missing".into()))?;
    let Some(uri) = attribute(rendition, "URI") else {
        return Ok(None);
    };
    let url = base
        .join(&uri)
        .map_err(|e| AppError::Other(e.to_string()))?;
    if url.scheme() != "https" {
        return Err(AppError::Other("Invalid audio playlist scheme".into()));
    }
    Ok(Some(url.into()))
}

/// Mux the corresponding video/audio segments before entering the existing TS pipeline.
/// Reading stdout while writing stdin prevents pipe-buffer deadlocks for large segments.
struct TempAudioFile(std::path::PathBuf);
impl Drop for TempAudioFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn mux_av_segment(video: Vec<u8>, audio: Vec<u8>) -> Result<Vec<u8>> {
    let path = std::env::temp_dir().join(format!(
        "str-audio-{}-{}.mp4",
        std::process::id(),
        rand::random::<u64>()
    ));
    let cleanup = TempAudioFile(path.clone());
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    std::io::Write::write_all(&mut file, &audio)?;
    drop(file);
    let result = async {
        let mut child = tokio::process::Command::new(crate::core::runtime::tool("ffmpeg"))
            .args(["-hide_banner", "-loglevel", "error", "-i", "pipe:0", "-i"])
            .arg(&path)
            .args([
                "-map", "0:v:0", "-map", "1:a:0", "-c", "copy", "-f", "mpegts", "pipe:1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        let writer = tokio::spawn(async move {
            stdin.write_all(&video).await?;
            stdin.shutdown().await
        });
        let output = child.wait_with_output().await?;
        let written = writer.await.map_err(|e| AppError::Other(e.to_string()))?;
        if !output.status.success() {
            return Err(AppError::Other(format!(
                "Audio/video mux failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        written?;
        Ok(output.stdout)
    }
    .await;
    drop(cleanup);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page(state: &str, source: &str) -> String {
        let room = serde_json::json!({"room_status":state,"hls_source":source,"num_viewers":"123"})
            .to_string();
        format!(
            "window.initialRoomDossier = {};",
            serde_json::to_string(&room).unwrap()
        )
    }
    #[test]
    fn public_and_nonpublic_rooms() {
        let info = parse_room(&page("public", "https://cdn.test/playlist.m3u8")).unwrap();
        assert!(info.is_recordable);
        assert_eq!(info.viewers, 123);
        for state in ["private", "group", "hidden", "offline"] {
            let info = parse_room(&page(state, "https://cdn.test/playlist.m3u8")).unwrap();
            assert!(!info.is_recordable);
            assert!(info.playlist_url.is_none());
        }
    }
    #[tokio::test]
    #[ignore = "requires ffmpeg/ffprobe; run cargo test -- --include-ignored"]
    async fn downloads_and_muxes_independent_fmp4_audio() {
        use axum::{Router, routing::get};
        use std::process::Command;
        let root = std::env::temp_dir().join(format!("str-split-av-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&root).unwrap();
        for (kind, source, codec) in [
            ("video", "testsrc2=size=320x180:rate=25", "libx264"),
            ("audio", "sine=frequency=440:sample_rate=48000", "aac"),
        ] {
            let status = Command::new(crate::core::runtime::tool("ffmpeg"))
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    source,
                    "-t",
                    "2",
                    "-c",
                    codec,
                    "-f",
                    "hls",
                    "-hls_segment_type",
                    "fmp4",
                    "-hls_time",
                    "2",
                    "-hls_list_size",
                    "0",
                    "-start_number",
                    "105",
                    "-hls_fmp4_init_filename",
                ])
                .arg(format!("{kind}-init.mp4"))
                .arg(root.join(format!("{kind}.m3u8")))
                .status()
                .unwrap();
            assert!(status.success());
        }
        let media_root = root.clone();
        let app = Router::new().route(
            "/{file}",
            get(
                move |axum::extract::Path(file): axum::extract::Path<String>| {
                    let root = media_root.clone();
                    async move { tokio::fs::read(root.join(file)).await.unwrap() }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let api = ChaturbateApi::new(None, None).unwrap();
        let video_url = format!("http://{address}/video.m3u8");
        api.cache_audio_playlist(&video_url, Some(format!("http://{address}/audio.m3u8")));
        let media = api.fetch_playlist(&video_url).await.unwrap();
        let (segments, init) = crate::recording::hls::parse_playlist(
            &media,
            &format!("http://{address}"),
            &HashMap::new(),
        )
        .unwrap();
        let mut video = api.download_segment(&init.unwrap()).await.unwrap();
        video.extend(api.download_segment(&segments[0].url).await.unwrap());
        let ts = api
            .prepare_av_segment("fixture", &video_url, 105, video.clone(), 0, false)
            .await
            .unwrap();
        assert!(
            api.prepare_av_segment("fixture", &video_url, 999, video, 0, false)
                .await
                .is_err()
        );
        let output = root.join("muxed.ts");
        std::fs::write(&output, ts).unwrap();
        let probe = Command::new(crate::core::runtime::tool("ffprobe"))
            .args(["-v", "error", "-show_streams", "-of", "json"])
            .arg(&output)
            .output()
            .unwrap();
        assert!(probe.status.success());
        let streams: Value = serde_json::from_slice(&probe.stdout).unwrap();
        assert!(
            streams["streams"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["codec_type"] == "video")
        );
        assert!(
            streams["streams"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["codec_name"] == "aac")
        );
        assert!(
            Command::new(crate::core::runtime::tool("ffmpeg"))
                .args(["-v", "error", "-xerror", "-i"])
                .arg(&output)
                .args(["-f", "null", "-"])
                .status()
                .unwrap()
                .success()
        );
        server.abort();
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn matches_selected_variants_audio_group() {
        let master = "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aac\",URI=\"audio/index.m3u8\"\n#EXT-X-STREAM-INF:BANDWIDTH=200,RESOLUTION=1280x720,AUDIO=\"aac\"\nvideo/index.m3u8\n";
        assert_eq!(
            select_audio_playlist(
                master,
                "https://cdn.test/master.m3u8",
                "https://cdn.test/video/index.m3u8"
            )
            .unwrap()
            .unwrap(),
            "https://cdn.test/audio/index.m3u8"
        );
        assert!(
            select_audio_playlist(
                "#EXTM3U",
                "https://cdn.test/master.m3u8",
                "https://cdn.test/media.m3u8"
            )
            .unwrap()
            .is_none()
        );
    }
    #[test]
    fn ajax_states_and_missing_public_source() {
        let room =
            serde_json::json!({"room_status":"public","hls_source":"https://cdn.test/media.m3u8"});
        assert!(parse_room_data(&room).unwrap().is_recordable);
        assert!(
            parse_room_data(&serde_json::json!({"room_status":"public","hls_source":""})).is_err()
        );
        assert!(
            !parse_room_data(&serde_json::json!({"room_status":"password protected"}))
                .unwrap()
                .is_recordable
        );
    }
    #[test]
    fn unicode_escaped_dossier_and_challenges() {
        let page = page("public", "https://cdn.test/playlist.m3u8").replace("\\\"", "\\u0022");
        assert!(parse_room(&page).unwrap().is_online);
        assert!(parse_room("<html>Just a moment...</html>").is_err());
        assert!(parse_room(&self::page("new_state", "")).is_err());
    }
    #[test]
    fn resolves_variants_and_resolution_fallback() {
        let master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=100,RESOLUTION=640x360\n../360/index.m3u8?t=a\n#EXT-X-STREAM-INF:BANDWIDTH=200,RESOLUTION=1280x720\n//cdn.test/720/index.m3u8\n";
        assert_eq!(
            select_playlist(master, "https://cdn.test/master/playlist.m3u8", 480, false).unwrap(),
            "https://cdn.test/360/index.m3u8?t=a"
        );
        assert_eq!(
            select_playlist(master, "https://cdn.test/master/playlist.m3u8", 480, true).unwrap(),
            "https://cdn.test/720/index.m3u8"
        );
        assert_eq!(
            select_playlist(
                "#EXTM3U\n#EXTINF:2,\n1.ts",
                "https://cdn.test/media.m3u8",
                0,
                false
            )
            .unwrap(),
            "https://cdn.test/media.m3u8"
        );
    }
}
