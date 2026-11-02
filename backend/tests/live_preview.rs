#[path="support/preview_fixture.rs"] mod support;
use std::{sync::{Arc,atomic::{AtomicUsize,Ordering}}, time::Duration};
use stripchat_recorder_lib::{preview::{PreviewManager,SourceResolver,SourceError}, core::emitter::{Emitter,NoopEmitter}, config::settings::AppState, recording::recorder::RecorderManager};
fn id(n: u32)->String { format!("00000000-0000-4000-8000-{n:012}") }
#[tokio::test(flavor="multi_thread",worker_threads=4)]
#[ignore="requires ffmpeg and ffprobe"]
async fn four_previews_cancellation_and_recording_are_independent() {
    let root=std::env::temp_dir().join(format!("str-preview-test-{}",rand::random::<u64>()));
    let (url,server)=support::fixture(&root.join("media")).await;
    let emitter:Arc<dyn Emitter>=Arc::new(NoopEmitter);
    let source=support::resolver(&url);
    let recovered_calls=Arc::new(AtomicUsize::new(0));
    let counts=recovered_calls.clone();
    let source:SourceResolver=Arc::new(move |name| {
        if name=="room_c" && counts.fetch_add(1,Ordering::SeqCst)==0 {
            Box::pin(async { Err(SourceError::retry("Temporary connection failure")) })
        } else { source(name) }
    });
    let manager=PreviewManager::with_resolver(root.join("cache"),emitter.clone(),source).await.unwrap();
    let client=reqwest::Client::new();
    let mut previews=Vec::new();
    for (i,username) in ["room_a","chaturbate~room_b","room_c","chaturbate~room_d"].iter().enumerate() {
        previews.push(manager.start(username,&id(i as u32)).unwrap());
    }
    for preview in &previews {
        tokio::time::timeout(Duration::from_secs(25),async {
            loop {
                if manager.status(&preview.preview_id).is_some_and(|s|s.state=="ready") { break; }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }).await.expect("native HLS playlist ready");
        let playlist=client.get(&preview.playlist_url).send().await.unwrap(); assert!(playlist.status().is_success());
        let text=playlist.text().await.unwrap(); assert!(text.contains("#EXTINF:"));
        assert!(text.lines().filter(|l|l.ends_with(".ts")).count()<=6);
        let segment=text.lines().find(|l|l.ends_with(".ts")).unwrap();
        let segment_url=preview.playlist_url.replace("index.m3u8",segment);
        let bytes=client.get(&segment_url).send().await.unwrap().bytes().await.unwrap();
        let path=root.join(format!("{}-preview.ts",preview.preview_id)); std::fs::write(&path,&bytes).unwrap();
        assert!(std::process::Command::new("ffmpeg").args(["-v","error","-xerror","-i"]).arg(&path).args(["-f","null","-"]).status().unwrap().success());
        let bad=client.get(&preview.playlist_url).header("Origin","https://outside.example").send().await.unwrap();
        assert_eq!(bad.status(),reqwest::StatusCode::FORBIDDEN);
        let bad=client.get(preview.playlist_url.replace("index.m3u8","secret.txt")).send().await.unwrap();
        assert_eq!(bad.status(),reqwest::StatusCode::NOT_FOUND);
    }
    assert!(recovered_calls.load(Ordering::SeqCst)>=2,"a temporary failure must recover automatically");
    // Start the actual recorder while four previews are running.
    let state=AppState::new().unwrap(); {
        let mut data=state.data.write(); data.settings.output_dir=root.join("recordings").to_string_lossy().into();
        data.settings.api_proxy_url=None; data.settings.cdn_proxy_url=None; data.settings.max_recording_duration_secs=0;
        data.settings.merge_format="mp4".into(); data.pipeline.nodes.clear();
    }
    let recorder=RecorderManager::new(state);
    let recording=recorder.start_recording_with_emitter("room_a",&url,emitter).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    manager.stop(&previews[0].preview_id).await;
    assert_eq!(manager.active_count(),3);
    assert!(recorder.is_recording("room_a"));
    assert_eq!(client.get(&previews[0].playlist_url).send().await.unwrap().status(),reqwest::StatusCode::NOT_FOUND);
    assert!(client.get(&previews[1].playlist_url).send().await.unwrap().status().is_success());
    manager.stop_username("chaturbate~room_b").await; assert_eq!(manager.active_count(),2);
    assert!(client.get(&previews[2].playlist_url).send().await.unwrap().status().is_success());
    manager.stop_all().await; assert_eq!(manager.active_count(),0);
    assert!(recorder.is_recording("room_a"));
    recorder.shutdown().await;
    let output=std::path::PathBuf::from(recording).with_extension("mp4"); assert!(output.exists());
    assert!(std::process::Command::new("ffmpeg").args(["-v","error","-xerror","-i"]).arg(&output).args(["-f","null","-"]).status().unwrap().success());
    // Stop before start is admitted; then many rapidly started/cancelled previews.
    manager.stop(&id(100)).await; assert!(manager.start("room_a",&id(100)).is_err());
    for i in 101..115 { let handle=manager.start("room_a",&id(i)).unwrap(); manager.stop(&handle.preview_id).await; }
    assert_eq!(manager.active_count(),0);
    assert_eq!(std::fs::read_dir(root.join("cache")).unwrap().count(),0);
    manager.shutdown().await; assert!(manager.start("room_a",&id(999)).is_err());
    server.abort(); std::fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn terminal_errors_cancel_pending_resolution_and_retry_budget() {
    let root=std::env::temp_dir().join(format!("str-preview-errors-{}",rand::random::<u64>()));
    let calls=Arc::new(AtomicUsize::new(0)); let counts=calls.clone();
    let resolver:SourceResolver=Arc::new(move|name| {
        let counts=counts.clone(); Box::pin(async move {
            if name=="pending" { return std::future::pending().await; }
            counts.fetch_add(1,Ordering::SeqCst);
            Err(if name=="offline" {SourceError::terminal("Offline")} else {SourceError::retry("Temporary")})
        })
    });
    let manager=PreviewManager::with_resolver(root.clone(),Arc::new(NoopEmitter),resolver).await.unwrap();
    manager.start("pending",&id(1)).unwrap(); manager.stop(&id(1)).await; assert_eq!(manager.active_count(),0);
    manager.start("offline",&id(2)).unwrap(); tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(calls.load(Ordering::SeqCst),1); assert_eq!(manager.active_count(),0);
    manager.start("retry",&id(3)).unwrap();
    tokio::time::timeout(Duration::from_secs(12),async { while manager.active_count()>0 { tokio::time::sleep(Duration::from_millis(100)).await; } }).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst),5); // one terminal attempt + initial/retries (four)
    manager.shutdown().await; std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test(flavor="multi_thread",worker_threads=4)]
#[ignore="requires ffmpeg and ffprobe"]
async fn separate_audio_window_skew_keeps_live_timeline_advancing() {
    use stripchat_recorder_lib::streaming::platform::StreamApi;
    let root=std::env::temp_dir().join(format!("str-preview-audio-{}",rand::random::<u64>()));
    let (url,server)=support::fixture(&root.join("media")).await;
    let resolver:SourceResolver=Arc::new(move |username| {
        let url=url.clone(); Box::pin(async move {
            let audio=url.replace("live.m3u8","audio.m3u8");
            let api=StreamApi::new_api_only(None,None,None).unwrap().with_channel(&username).with_audio_playlist(&url,Some(audio));
            Ok((api,url))
        })
    });
    let manager=PreviewManager::with_resolver(root.join("cache"),Arc::new(NoopEmitter),resolver).await.unwrap();
    let preview=manager.start("chaturbate~audio_skew",&id(998)).unwrap();
    tokio::time::timeout(Duration::from_secs(30),async {
        while !manager.status(&preview.preview_id).is_some_and(|s|s.state=="ready") {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("preview survives missing oldest audio segment");
    tokio::time::sleep(Duration::from_secs(6)).await;
    let playlist=reqwest::get(&preview.playlist_url).await.unwrap().text().await.unwrap();
    let durations:Vec<f64>=playlist.lines().filter_map(|line|line.strip_prefix("#EXTINF:")).filter_map(|n|n.trim_end_matches(',').parse().ok()).collect();
    assert!(durations.len()>=3 && durations.iter().sum::<f64>()>=5.0,"timeline must advance: {playlist}");
    manager.shutdown().await; server.abort(); std::fs::remove_dir_all(root).unwrap();
}
