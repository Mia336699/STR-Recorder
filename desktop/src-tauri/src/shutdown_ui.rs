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
