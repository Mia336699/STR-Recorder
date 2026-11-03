//! Let the webview flush buffered edits before native recording finalization.
use tauri::{Emitter, Listener};

pub async fn prepare(app: &tauri::AppHandle) {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let listener = app.once("ui-exit-ready", move |_| { let _ = sender.send(()); });
    phase(app, "preparing");
    // An unresponsive webview must not prevent the recorder from finalizing.
    match tokio::time::timeout(std::time::Duration::from_secs(3), receiver).await {
        Ok(Ok(())) => tracing::info!("Desktop UI edits flushed before shutdown"),
        _ => tracing::warn!("Desktop UI did not acknowledge shutdown; continuing recorder finalization"),
    }
    app.unlisten(listener);
}

pub fn phase(app: &tauri::AppHandle, value: &str) {
    let _ = app.emit("app-exiting", value);
}

/// History entries stay in memory; only active tasks and final writes delay exit.
pub async fn wait_for_postprocess(state: std::sync::Arc<stripchat_recorder_lib::config::settings::AppState>) {
    loop {
        while state.pp_tasks.read().values().any(|task| task.is_active()) {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let writer = state.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _guard = writer.pp_lock.lock().unwrap_or_else(|error| error.into_inner());
        }).await;
        if let Err(error) = result { tracing::error!("Post-processing completion barrier failed: {error}"); }
        if !state.pp_tasks.read().values().any(|task| task.is_active()) { break; }
    }
}
