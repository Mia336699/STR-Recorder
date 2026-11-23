/// WKWebView disables element fullscreen by default, even when video can play inline.
pub fn enable_live_fullscreen(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    if let Err(error) = window.with_webview(|webview| {
        // Tauri provides a live WKWebView on the main thread for this callback.
        unsafe {
            let view = &*webview.inner().cast::<objc2_web_kit::WKWebView>();
            view.configuration().preferences().setElementFullscreenEnabled(true);
        }
    }) {
        tracing::warn!("Could not enable WebKit fullscreen: {error}");
    }
    #[cfg(not(target_os = "macos"))]
    let _ = window;
}
