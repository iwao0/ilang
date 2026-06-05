/// WebView — stub. WebKitGTK (`webkit_web_view_new` /
/// `webkit_web_view_load_uri` / `load-changed` signal) maps
/// cleanly and is the natural follow-up, but Round 1 ships the
/// macOS path only. The call surface is present so cross-platform
/// consumer code links and runs; the returned handle is `-1` and
/// every setter is a no-op.

@extern(C) {

pub fn createNativeWebView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    -1 as i64
}

pub fn setWebViewUrl(handle: i64, url: string) {}
pub fn readWebViewUrl(handle: i64): string { "" }
pub fn webViewLoadHtml(handle: i64, html: string, baseUrl: string) {}
pub fn webViewGoBack(handle: i64) {}
pub fn webViewGoForward(handle: i64) {}
pub fn webViewReload(handle: i64) {}
pub fn webViewCanGoBack(handle: i64): bool { false }
pub fn webViewCanGoForward(handle: i64): bool { false }
pub fn setWebViewFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {}
pub fn setWebViewVisible(handle: i64, visible: bool) {}
pub fn setOnWebViewNavigate(handle: i64, cb: fn(string)) {}
pub fn setOnWebViewLoadFinish(handle: i64, cb: fn(string, bool, string)) {}

} // @extern(C)
