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

fn verify_media(path: &Path) { verify_media_duration(path, 5.8, 7.0); }

fn verify_media_duration(path: &Path, min: f64, max: f64) {
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
    assert!((min..max).contains(&duration), "duration: {duration}");
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

/// Real recorder rotation (not a filename-only test), followed by mixed-format crash recovery.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires ffmpeg and ffprobe"]
async fn sixteen_parts_sort_and_recover_without_changing_actual_times() {
    use stripchat_recorder_lib::recording::{meta, recorder::startup_merge_leftover_segments};
    let root = std::env::temp_dir().join(format!("str-parts-{}", rand::random::<u64>()));
    let media = root.join("media"); std::fs::create_dir_all(&media).unwrap();
    assert!(Command::new("ffmpeg").args([
        "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=160x90:rate=25",
        "-f", "lavfi", "-i", "sine=frequency=660:sample_rate=48000", "-t", "16",
        "-c:v", "libx264", "-preset", "ultrafast", "-g", "25", "-sc_threshold", "0",
        "-c:a", "aac", "-f", "hls", "-hls_time", "1", "-hls_list_size", "0",
    ]).arg(media.join("playlist.m3u8")).status().unwrap().success());
    let serve = media.clone();
    let app = Router::new().route("/{file}", get(move |axum::extract::Path(file): axum::extract::Path<String>| {
        let dir = serve.clone(); async move { tokio::fs::read(dir.join(file)).await.unwrap() }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let state = AppState::new().unwrap();
    {
        let mut data = state.data.write();
        data.settings.output_dir = root.join("recordings").to_string_lossy().into();
        data.settings.api_proxy_url = None; data.settings.cdn_proxy_url = None;
        data.settings.sc_mirror_url = None; data.settings.max_recording_duration_secs = 1;
        data.settings.merge_format = "mp4".into(); data.pipeline.nodes.clear();
    }
    let recorder = RecorderManager::new(state.clone());
    let emitter: Arc<dyn Emitter> = Arc::new(NoopEmitter);
    let first = PathBuf::from(recorder.start_recording_with_emitter(
        "chaturbate~test_room", &format!("http://{address}/playlist.m3u8"), emitter.clone()
    ).await.unwrap());
    let parent = first.parent().unwrap().to_path_buf();
    let stem = first.file_name().unwrap().to_str().unwrap();
    assert!(stem.starts_with("test_room_"));
    let prefix = stem.strip_suffix("part01").unwrap();
    let last = parent.join(format!("{prefix}part16"));
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            if last.join("chaturbate~test_room_segment000015.ts").exists() { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("sixteen real rotated files");
    tokio::time::timeout(Duration::from_secs(90), recorder.shutdown()).await.unwrap();
    let expected: Vec<_> = (1..=16).map(|i| format!("{prefix}part{i:02}.mp4")).collect();
    let mut names: Vec<_> = std::fs::read_dir(&parent).unwrap().flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "mp4"))
        .map(|entry| entry.file_name().to_string_lossy().into_owned()).collect();
    names.sort(); assert_eq!(names, expected); // includes part09 -> part10
    let mut starts = Vec::new(); let mut group = None;
    for (index, name) in names.iter().enumerate() {
        let path = parent.join(name); verify_media_duration(&path, 0.9, 1.15);
        let metadata = meta::read_meta(&path).unwrap();
        assert_eq!(metadata.part_index, Some((index + 1) as u8));
        if index == 0 { group = metadata.group_started_at.clone(); }
        assert!(group.is_some()); assert_eq!(metadata.group_started_at, group);
        assert_eq!(metadata.status, "finish"); starts.push(metadata.started_at);
    }
    assert!(starts.windows(2).all(|w| w[0] < w[1]));
    // Post-processing updates preserve grouping and actual per-part timestamps.
    let tenth = parent.join(&names[9]);
    meta::set_pp_done(&tenth, "finish", Vec::new(), [("preview".into(), "preview.jpg".into())].into());
    assert_eq!(meta::read_meta(&tenth).unwrap().started_at, starts[9]);
    assert_eq!(meta::read_meta(&tenth).unwrap().part_index, Some(10));

    // Create a legacy recording directory and a new part directory as if interrupted.
    let fixture = media.join("playlist0.ts");
    for (stem, actual, part) in [
        ("legacy_room_20261016_203000", "2026-10-16T20:30:00+08:00", None),
        ("recovered_room_20261016_233000_part02", "2026-10-17T00:00:02+08:00", Some(2)),
    ] {
        let dir = parent.join(stem); std::fs::create_dir(&dir).unwrap();
        std::fs::copy(&fixture, dir.join("segment.ts")).unwrap();
        // Interrupted merge output must not get published or prevent recovery.
        std::fs::write(dir.join(".merge-interrupted.mp4"), b"partial").unwrap();
        std::fs::write(dir.join("playlist.m3u8"), "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\nsegment.ts\n#EXT-X-ENDLIST\n").unwrap();
        let output = dir.with_extension("mp4"); meta::ensure_meta(&output, actual);
        if part.is_some() {
            let mut metadata = meta::read_meta(&output).unwrap();
            metadata.group_started_at = Some("2026-10-16T23:30:00+08:00".into());
            metadata.part_index = part; meta::write_meta(&output, &metadata);
        }
    }
    let recovered_state = state.clone(); let recovered_recorder = recorder.clone(); let recovered_emitter = emitter.clone();
    let recover_root = root.join("recordings");
    tokio::task::spawn_blocking(move || {
        startup_merge_leftover_segments(&recover_root, "mp4", &recovered_emitter, &recovered_recorder);
        let files = stripchat_recorder_lib::commands::recording_cmd::list_recordings_inner(&recovered_state, &recovered_recorder).unwrap();
        assert_eq!(files.len(), 18);
        assert!(files.windows(2).all(|pair| {
            let left = chrono::DateTime::parse_from_rfc3339(&pair[0].started_at).unwrap();
            let right = chrono::DateTime::parse_from_rfc3339(&pair[1].started_at).unwrap();
            left >= right
        }));
        let file = files.iter().find(|f| f.name.ends_with("233000_part02.mp4")).unwrap();
        assert_eq!(file.started_at, "2026-10-17T00:00:02+08:00");
    }).await.unwrap();
    let recovered = parent.join("recovered_room_20261016_233000_part02.mp4");
    verify_media_duration(&recovered, 0.9, 1.15);
    assert_eq!(meta::read_meta(&recovered).unwrap().part_index, Some(2));
    assert!(parent.join("legacy_room_20261016_203000.mp4").exists());
    server.abort(); std::fs::remove_dir_all(root).unwrap();
}
