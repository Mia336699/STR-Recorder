//! Explicit public-room smoke test with isolated settings. Prints no signed source URLs.
use std::{sync::{Arc,Mutex},time::Duration};
use stripchat_recorder_lib::{config::settings::AppState,core::emitter::Emitter,preview::PreviewManager};
#[derive(Default)] struct Events(Mutex<Vec<serde_json::Value>>);
impl Emitter for Events { fn emit_raw(&self,event:&str,payload:&str){ if event=="live-preview-state" { if let Ok(value)=serde_json::from_str(payload){self.0.lock().unwrap().push(value);} } } }
#[tokio::main]
async fn main(){
 let _=tracing_subscriber::fmt().with_env_filter("stripchat_recorder_lib::preview=warn").try_init();
 let root=std::env::temp_dir().join(format!("str-preview-probe-{}",std::process::id()));
 stripchat_recorder_lib::core::runtime::initialize(root.join("data"),std::path::PathBuf::from(std::env::var("STR_TOOLS").unwrap_or_default())).unwrap();
 let state=AppState::new().unwrap(); let events=Arc::new(Events::default());
 let manager=PreviewManager::new(root.join("cache"),state.clone(),events.clone()).await.unwrap();
 let client=reqwest::Client::new();
 for (i,name) in std::env::args().skip(1).enumerate(){
  let identity=stripchat_recorder_lib::streaming::identity::normalize(&name).unwrap();state.add_streamer(&identity).unwrap();
  let id=format!("00000000-0000-4000-8000-{i:012}");let started=manager.start(&identity,&id).unwrap();
  let deadline=tokio::time::Instant::now()+Duration::from_secs(35);let mut played=false;
  loop{
   let phase=manager.status(&id).map(|s|s.state);
   if phase.as_deref()==Some("ready"){
    let response=client.get(&started.playlist_url).send().await.unwrap();
    played=response.status().is_success()&&response.text().await.unwrap().contains("#EXTINF:");break;
   }
   if phase.is_none()||tokio::time::Instant::now()>deadline{break;}
   tokio::time::sleep(Duration::from_millis(250)).await;
  }
  let mut media_verified=false; let mut codecs=Vec::new();
  if played {
   tokio::time::sleep(Duration::from_secs(6)).await;
   if let Ok(response)=client.get(&started.playlist_url).send().await {
    if let Ok(text)=response.text().await {
     if let Some(segment)=text.lines().filter(|line|line.ends_with(".ts")).last() {
      if let Ok(response)=client.get(started.playlist_url.replace("index.m3u8",segment)).send().await {
       if let Ok(bytes)=response.bytes().await {
        let sample=root.join("preview-sample.ts");std::fs::write(&sample,bytes).unwrap();
        let probe=std::process::Command::new(stripchat_recorder_lib::core::runtime::tool("ffprobe"))
         .args(["-v","error","-show_entries","stream=codec_name,codec_type","-of","json"]).arg(&sample).output().unwrap();
        if let Ok(info)=serde_json::from_slice::<serde_json::Value>(&probe.stdout) {
         codecs=info["streams"].as_array().map(|s|s.iter().map(|v|v["codec_name"].as_str().unwrap_or("").to_owned()).collect()).unwrap_or_default();
        }
        media_verified=std::process::Command::new(stripchat_recorder_lib::core::runtime::tool("ffmpeg"))
         .args(["-v","error","-xerror","-i"]).arg(&sample).args(["-fps_mode","passthrough","-enc_time_base:v","demux","-f","null","-"]).status().unwrap().success();
       }
      }
     }
    }
   }
  }
  let last=events.0.lock().unwrap().iter().rev().find(|e|e["preview_id"]==id).cloned();
  println!("{}",serde_json::json!({"identity":identity,"hls_ready":played,"media_decoded":media_verified,"codecs":codecs,"last_state":last.as_ref().map(|e|&e["state"]),"message":last.as_ref().map(|e|&e["message"])}));
  manager.stop(&id).await;
 }
 manager.shutdown().await;let _=std::fs::remove_dir_all(root);
}
