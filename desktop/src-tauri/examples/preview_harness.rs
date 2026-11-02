//! Developer-only native harness. Never included in the production DMG.
#[path="../src/state.rs"] mod state;
#[path="../src/emitter.rs"] mod emitter;
#[allow(dead_code)]
#[path="../src/commands.rs"] mod commands;
#[path="../../../backend/tests/support/preview_fixture.rs"] mod fixture;
use std::sync::{Arc,atomic::AtomicBool};
use stripchat_recorder_lib::{config::settings::AppState,core::emitter::Emitter,recording::recorder::RecorderManager,streaming::monitor::StatusMonitor,preview::PreviewManager};
use tauri::Manager;
#[tauri::command]
fn qa_preview_count(state: tauri::State<'_, state::DesktopState>) -> usize { state.previews.as_ref().map_or(0,|p|p.active_count()) }
#[tauri::command]
fn qa_add_room(username: String, state: tauri::State<'_, state::DesktopState>) -> Result<String,String> {
    let name=stripchat_recorder_lib::streaming::identity::normalize(&username).map_err(|e|e.to_string())?;
    if !state.app_state.get_streamers().iter().any(|s|s.username==name) {
        state.app_state.add_streamer(&name).map_err(|e|e.to_string())?;
    }
    Ok(name)
}
fn main() {
    let root=std::path::PathBuf::from(std::env::var("STR_PREVIEW_QA_ROOT").unwrap_or_else(|_| std::env::temp_dir().join(format!("str-preview-native-qa-{}", std::process::id())).to_string_lossy().into_owned()));
    let tools=std::path::PathBuf::from(std::env::var("STR_PREVIEW_QA_TOOLS").unwrap_or_else(|_| std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("Resources/bin").to_string_lossy().into_owned()));
    stripchat_recorder_lib::core::runtime::initialize(root.join("data"),tools).unwrap();
    let rt=Arc::new(tokio::runtime::Runtime::new().unwrap());
    let (source,fixture_server)=rt.block_on(fixture::fixture(&root.join("media")));
    let app_state=AppState::new().unwrap();
    for name in ["qa_one","chaturbate~qa_two","qa_three","chaturbate~qa_four"] { app_state.add_streamer(name).unwrap(); }
    { let mut data=app_state.data.write();data.settings.output_dir=root.join("recordings").to_string_lossy().into();data.pipeline.nodes.clear(); }
    let runtime=rt.clone();
    let app=tauri::Builder::default().setup(move|app| {
        let emitter:Arc<dyn Emitter>=Arc::new(emitter::TauriEmitter::new(app.handle().clone()));
        let recorder=RecorderManager::new(app_state.clone());
        let monitor=StatusMonitor::new(app_state.clone(),recorder.clone());
        let synthetic=fixture::resolver(&source);
        let public=PreviewManager::public_resolver(app_state.clone());
        let resolver:stripchat_recorder_lib::preview::SourceResolver=Arc::new(move|name| {
            if name.starts_with("qa_") || name.starts_with("chaturbate~qa_") { synthetic(name) } else { public(name) }
        });
        let previews=runtime.block_on(PreviewManager::with_resolver(root.join("cache"),emitter.clone(),resolver)).unwrap();
        app.manage(state::DesktopState{exit_ready:AtomicBool::new(false),exiting:AtomicBool::new(false),startup_done:Arc::new(AtomicBool::new(true)),app_state,recorder,monitor,emitter,previews:Some(previews)});
        if let Some(window) = app.get_webview_window("main") {
            window.navigate("tauri://localhost/tests/preview-harness.html".parse().unwrap()).unwrap();
            let _ = window.set_title("STR Preview QA — synthetic streams");
            let _ = window.show(); let _ = window.set_focus();
        }
        Ok(())
    }).invoke_handler(tauri::generate_handler![qa_add_room,qa_preview_count,commands::start_live_preview,commands::stop_live_preview,commands::stop_all_live_previews,commands::get_live_preview_status])
    .build(tauri::generate_context!("tauri.preview-qa.conf.json")).unwrap();
    app.run(|_,_|{});
    fixture_server.abort();
}
