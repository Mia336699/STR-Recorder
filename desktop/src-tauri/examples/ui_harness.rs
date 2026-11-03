//! Full native UI acceptance app. Uses disposable synthetic media and real production views/commands.
#[path="../src/state.rs"] mod state;
#[path="../src/emitter.rs"] mod emitter;
#[allow(dead_code)] #[path="../src/commands.rs"] mod commands;
#[path="../../../backend/tests/support/preview_fixture.rs"] mod fixture;
use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
use stripchat_recorder_lib::{config::settings::AppState,core::emitter::{Emitter,EmitterExt},recording::{recorder::RecorderManager,meta},streaming::monitor::StatusMonitor,preview::PreviewManager};
use tauri::Manager;
struct Source(String);
#[tauri::command(rename="list_streamers")]
fn qa_list_streamers(state:tauri::State<'_,state::DesktopState>)->serde_json::Value {
 serde_json::json!(state.app_state.get_streamers().iter().map(|s| {
 let online=!s.username.contains("offline");let public=online&&!s.username.contains("private");
 serde_json::json!({"username":s.username,"auto_record":s.auto_record,"added_at":s.added_at,"is_online":online,"is_recordable":public,"is_recording":state.recorder.is_recording(&s.username),"viewers":if online{128}else{0},"status":if public{"公开秀"}else if online{"私密秀"}else{"离线"},"thumbnail_url":null})
 }).collect::<Vec<_>>())
}
#[tauri::command(rename="verify_streamer")]
fn qa_verify_streamer(username:String)->serde_json::Value {serde_json::json!({"exists":!username.contains("missing"),"is_recordable":true,"is_online":true,"status":"公开秀"})}
#[tauri::command(rename="add_streamer")]
async fn qa_add_streamer(username:String,auto_record:Option<bool>,state:tauri::State<'_,state::DesktopState>,source:tauri::State<'_,Source>)->Result<serde_json::Value,String>{
 let username=stripchat_recorder_lib::streaming::identity::normalize(&username).map_err(|e|e.to_string())?;
 state.app_state.add_streamer_with_auto_record(&username,Some(auto_record.unwrap_or(false))).map_err(|e|e.to_string())?;
 state.emitter.emit("streamer-added",&serde_json::json!({"username":username}));
 if auto_record.unwrap_or(false) {state.recorder.start_recording_with_emitter(&username,&source.0,state.emitter.clone()).await.map_err(|e|e.to_string())?;}
 Ok(serde_json::json!({"ok":true}))
}
#[tauri::command(rename="start_recording")]
async fn qa_start_recording(username:String,state:tauri::State<'_,state::DesktopState>,source:tauri::State<'_,Source>)->Result<serde_json::Value,String>{
 let path=state.recorder.start_recording_with_emitter(&username,&source.0,state.emitter.clone()).await.map_err(|e|e.to_string())?;Ok(serde_json::json!({"path":path}))
}
fn seed(root:&std::path::Path){
 use std::process::Command;
 let dir=root.join("recordings/chaturbate/qa_archive");std::fs::create_dir_all(&dir).unwrap();
 let sample=root.join("sample.mp4");
 assert!(Command::new(stripchat_recorder_lib::core::runtime::tool("ffmpeg")).args(["-v","error","-y","-f","lavfi","-i","testsrc2=size=640x360:rate=25","-f","lavfi","-i","sine=frequency=440:sample_rate=48000","-t","4","-c:v","libx264","-preset","ultrafast","-pix_fmt","yuv420p","-c:a","aac","-movflags","+faststart"]).arg(&sample).status().unwrap().success());
 for i in 1..=16 {
 let path=dir.join(format!("qa_archive_20261102_233000_part{i:02}.mp4"));std::fs::copy(&sample,&path).unwrap();
 let minute=30+4*(i-1);let (day,hour,minute)=if minute>=60 {(3,0,minute-60)}else{(2,23,minute)};
 let started=format!("2026-11-{day:02}T{hour:02}:{minute:02}:00+08:00");
 meta::ensure_meta(&path,&started);let mut m=meta::read_meta(&path).unwrap();m.group_started_at=Some("2026-11-02T23:30:00+08:00".into());m.part_index=Some(i);m.status="finish".into();m.video_duration_secs=Some(4);meta::write_meta(&path,&m);
 }
 let legacy=dir.join("qa_archive_20261017_190000.mp4");std::fs::copy(&sample,&legacy).unwrap();meta::ensure_meta(&legacy,"2026-10-17T19:00:00+08:00");meta::set_status(&legacy,"finish");
}
fn main(){
 let exe=std::env::current_exe().unwrap();let bundle=exe.parent().unwrap().parent().unwrap();
 let root=std::env::temp_dir().join(format!("str-native-ui-qa-{}",std::process::id()));
 stripchat_recorder_lib::core::runtime::initialize(root.join("data"),bundle.join("Resources/bin")).unwrap();
 let rt=Arc::new(tokio::runtime::Runtime::new().unwrap());
 let (source,server)=rt.block_on(fixture::fixture_seconds(&root.join("media"),1200));
 seed(&root);
 let app_state=AppState::new().unwrap();
 for name in ["qa_one","chaturbate~qa_two","qa_three","chaturbate~qa_four","qa_offline","chaturbate~qa_offline","qa_private","qa_offline_long_name_123456789"] {app_state.add_streamer_with_auto_record(name,Some(false)).unwrap();}
 {let mut d=app_state.data.write();d.settings.output_dir=root.join("recordings").to_string_lossy().into();d.settings.setup_done=true;d.settings.max_recording_duration_secs=8;d.settings.auto_record=true;d.pipeline.nodes.clear();}
 let modules=bundle.join("Resources/modules");if modules.exists(){std::fs::create_dir_all(root.join("data/modules")).unwrap();for entry in std::fs::read_dir(modules).unwrap().flatten(){if entry.path().is_file(){std::fs::copy(entry.path(),root.join("data/modules").join(entry.file_name())).unwrap();}}}
 let runtime=rt.clone();
 let app=tauri::Builder::default().plugin(tauri_plugin_dialog::init()).plugin(tauri_plugin_shell::init()).plugin(tauri_plugin_fs::init()).plugin(tauri_plugin_opener::init()).setup(move|app|{
 let emitter:Arc<dyn Emitter>=Arc::new(emitter::TauriEmitter::new(app.handle().clone()));
 let recorder=RecorderManager::new(app_state.clone());let monitor=StatusMonitor::new(app_state.clone(),recorder.clone());
 let previews=runtime.block_on(PreviewManager::with_resolver(root.join("cache"),emitter.clone(),fixture::resolver(&source))).unwrap();
 app.manage(Source(source));app.manage(state::DesktopState{exit_ready:AtomicBool::new(false),exiting:AtomicBool::new(false),startup_done:Arc::new(AtomicBool::new(true)),app_state,recorder,monitor,emitter,previews:Some(previews)});
 if let Some(w)=app.get_webview_window("main"){w.navigate("tauri://localhost/".parse().unwrap()).unwrap();let _=w.set_title("STR UI QA — isolated synthetic data");let _=w.show();let _=w.set_focus();}
 Ok(())
 }).invoke_handler(tauri::generate_handler![
            commands::start_live_preview,
            commands::stop_live_preview,
            commands::stop_all_live_previews,
            commands::get_live_preview_status,
            // Streamers
            qa_list_streamers,
            qa_add_streamer,
            commands::remove_streamer,
            commands::set_auto_record,
            qa_start_recording,
            commands::stop_recording,
            qa_verify_streamer,
            // Settings
            commands::get_settings,
            commands::choose_recording_directory,
            commands::save_settings_cmd,
            commands::get_disk_space,
            // Mouflon Keys
            commands::list_mouflon_keys,
            commands::add_mouflon_key,
            commands::remove_mouflon_key,
            commands::sync_mouflon_keys,
            // Recordings
            commands::list_recordings,
            commands::get_merging_dirs,
            commands::delete_recording,
            commands::open_recording,
            commands::authorize_recording_media,
            commands::inspect_recording,
            commands::reveal_recording,
            commands::open_output_dir,
            commands::read_output_file,
            commands::get_module_outputs,
            // Post-processing
            commands::run_postprocess_cmd,
            commands::run_postprocess_batch,
            commands::cancel_postprocess,
            commands::get_postprocess_tasks,
            // Pipeline
            commands::get_pipeline,
            commands::save_pipeline,
            commands::list_modules,
            // Locale
            commands::get_locale,
            commands::list_locales,
            // Startup warnings
            commands::get_startup_warnings,
            commands::remove_missing_pp_results,
        ]).build(tauri::generate_context!("tauri.ui-qa.conf.json")).unwrap();
 app.run(|app,event|{if let tauri::RunEvent::ExitRequested{api,..}=event{let state=app.state::<state::DesktopState>();if state.exit_ready.load(Ordering::SeqCst){return}api.prevent_exit();if state.exiting.swap(true,Ordering::SeqCst){return}let previews=state.previews.clone();let recorder=state.recorder.clone();let app=app.clone();tauri::async_runtime::spawn(async move{if let Some(p)=previews{p.shutdown().await;}recorder.shutdown().await;app.state::<state::DesktopState>().exit_ready.store(true,Ordering::SeqCst);app.exit(0);});}});
 server.abort();
}
