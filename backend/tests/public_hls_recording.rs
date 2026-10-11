//! Explicit local acceptance test: no real rooms, cookies, or existing user configuration.
use axum::{Router, routing::get};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};
use stripchat_recorder_lib::{
    config::settings::AppState,
    core::emitter::{Emitter, NoopEmitter},
    recording::recorder::RecorderManager,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ffmpeg and ffprobe; run cargo test --test public_hls_recording -- --ignored"]
async fn records_both_platforms_and_merges_video_with_audio() {
    let root = std::env::temp_dir().join(format!(
        "str-hls-qa-{}-{}",
        std::process::id(),
        rand::random::<u64>()
    ));
    let media = root.join("media");
    std::fs::create_dir_all(&media).unwrap();
    let status = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "6",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-g",
            "50",
            "-c:a",
            "aac",
            "-f",
            "hls",
            "-hls_time",
            "2",
            "-hls_list_size",
            "0",
            "-start_number",
            "105",
        ])
        .arg(media.join("playlist.m3u8"))
        .status()
        .unwrap();
    assert!(status.success());
    let serve_media = media.clone();
    let app = Router::new().route(
        "/{file}",
        get(
            move |axum::extract::Path(file): axum::extract::Path<String>| {
                let dir = serve_media.clone();
                async move { tokio::fs::read(dir.join(file)).await.unwrap() }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    // AppState's executable-relative config is inside this freshly built test directory.
    // Override settings in memory; never save or start the status monitor.
    let state = AppState::new().unwrap();
    {
        let mut data = state.data.write();
        data.settings.output_dir = root.join("recordings").to_string_lossy().into();
        data.settings.api_proxy_url = None;
        data.settings.cdn_proxy_url = None;
        data.settings.sc_mirror_url = None;
        data.settings.max_recording_duration_secs = 0;
        data.settings.merge_format = "mp4".into();
        data.pipeline.nodes.clear();
    }
    let recorder = RecorderManager::new(state);
    let emitter: Arc<dyn Emitter> = Arc::new(NoopEmitter);
    for identity in ["alice", "chaturbate~alice"] {
        let session = PathBuf::from(
            recorder
                .start_recording_with_emitter(
                    identity,
                    &format!("http://{address}/playlist.m3u8"),
                    emitter.clone(),
                )
                .await
                .unwrap(),
        );
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let count = std::fs::read_dir(&session)
                    .unwrap()
                    .flatten()
                    .filter(|e| e.path().extension().is_some_and(|s| s == "ts"))
                    .count();
                if count == 3 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("three distinct HLS segments should be recorded");
        // Give one additional polling pass a chance to detect duplicate segments.
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert_eq!(
            std::fs::read_dir(&session)
                .unwrap()
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|s| s == "ts"))
                .count(),
            3
        );
        if identity.starts_with("chaturbate~") {
            tokio::time::timeout(Duration::from_secs(30), recorder.shutdown())
                .await.expect("desktop shutdown must finalize recordings");
            assert!(recorder.start_recording_with_emitter(
                "new_room", &format!("http://{address}/playlist.m3u8"), emitter.clone()
            ).await.is_err(), "shutdown must reject new sessions");
        } else {
            recorder.stop_recording(identity).await.unwrap();
        }
        let output = session.with_extension("mp4");
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if output.exists() && !session.exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("stop should merge MP4 and finish the session");
        verify_media(&output);
    }
    server.abort();
    std::fs::remove_dir_all(root).unwrap();
}

fn verify_media(path: &Path) {
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(probe.status.success());
    let info: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    let streams = info["streams"].as_array().unwrap();
    assert!(
        streams
            .iter()
            .any(|s| s["codec_type"] == "video" && s["codec_name"] == "h264")
    );
    assert!(
        streams
            .iter()
            .any(|s| s["codec_type"] == "audio" && s["codec_name"] == "aac")
    );
    let duration: f64 = info["format"]["duration"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((5.8..7.0).contains(&duration), "duration: {duration}");
    assert!(
        Command::new("ffmpeg")
            .args(["-v", "error", "-xerror", "-i"])
            .arg(path)
            .args(["-f", "null", "-"])
            .status()
            .unwrap()
            .success()
    );
}
