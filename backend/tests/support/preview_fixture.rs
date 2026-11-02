use std::{path::{Path,PathBuf}, process::Command, sync::Arc, time::Instant};
use axum::{Router, extract::{Path as RoutePath, State}, routing::get};
use stripchat_recorder_lib::{preview::{SourceResolver, SourceError}, streaming::platform::StreamApi};
#[derive(Clone)]
struct Fixture { dir: PathBuf, started: Instant }
pub async fn fixture(root: &Path) -> (String, tokio::task::JoinHandle<()>) {
    std::fs::create_dir_all(root).unwrap();
    assert!(Command::new(stripchat_recorder_lib::core::runtime::tool("ffmpeg")).args([
        "-v","error","-y","-f","lavfi","-i","testsrc2=size=320x180:rate=25",
        "-f","lavfi","-i","sine=frequency=440:sample_rate=48000","-t","300",
        "-c:v","libx264","-preset","ultrafast","-g","50","-sc_threshold","0","-c:a","aac",
        "-f","hls","-hls_time","2","-hls_list_size","0",
    ]).arg(root.join("all.m3u8")).status().unwrap().success());
    let app=Router::new().route("/{file}",get(|State(s): State<Fixture>, RoutePath(file): RoutePath<String>| async move {
        if file == "live.m3u8" || file == "audio.m3u8" {
            let last=(3+s.started.elapsed().as_secs()/2).min(148);
            let first=last.saturating_sub(5) + u64::from(file == "audio.m3u8");
            let mut text=format!("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:2\n#EXT-X-MEDIA-SEQUENCE:{first}\n");
            for i in first..=last { text.push_str(&format!("#EXTINF:2,\nall{i}.ts\n")); }
            text.into_bytes()
        } else if file.starts_with("all") && file.ends_with(".ts") {
            tokio::fs::read(s.dir.join(file)).await.unwrap()
        } else { Vec::new() }
    })).with_state(Fixture{dir:root.into(),started:Instant::now()});
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url=format!("http://{}/live.m3u8",listener.local_addr().unwrap());
    let task=tokio::spawn(async move { axum::serve(listener,app).await.unwrap(); });
    (url,task)
}
pub fn resolver(url: &str) -> SourceResolver {
    let url=url.to_owned();
    Arc::new(move |username| {
        let url=url.clone();
        Box::pin(async move {
            let api=StreamApi::new_api_only(None,None,None).map_err(|e|SourceError::retry(e.to_string()))?
                .with_channel(&username).with_audio_playlist(&url,None);
            Ok((api,url))
        })
    })
}
