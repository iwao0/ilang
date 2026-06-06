/// WebView — WebKitGTK 6.0 (`WebKitWebView`). `load-changed`
/// drives onNavigate (on COMMITTED) and onLoadFinish (on FINISHED);
/// `load-failed` drives onLoadFinish with the error flag set. The
/// view is a plain GtkWidget parented into the window's GtkFixed,
/// same as every other linux widget.

use std.ffi { cstrFromString, stringFromCstr, readI64 }
use gtk {
    GtkWidget, GtkFixed, GObject,
    WebKitWebView,
    webkit_web_view_new, webkit_web_view_load_uri, webkit_web_view_get_uri,
    webkit_web_view_load_html,
    webkit_web_view_go_back, webkit_web_view_go_forward, webkit_web_view_reload,
    webkit_web_view_can_go_back, webkit_web_view_can_go_forward,
    gtk_fixed_put, gtk_fixed_move,
    gtk_widget_set_size_request, gtk_widget_set_visible,
    g_signal_connect_load_changed, g_signal_connect_load_failed,
}
use core { resolveGtkParent }

class WebViewSlot {
    pub view:  WebKitWebView
    pub fixed: GtkFixed
    pub onNavigate:   fn(string)
    pub onLoadFinish: fn(string, bool, string)
    pub init(view: WebKitWebView, fixed: GtkFixed) {
        this.view  = view
        this.fixed = fixed
        this.onNavigate   = webViewNoOpUrl
        this.onLoadFinish = webViewNoOpFinish
    }
}

fn webViewNoOpUrl(url: string) {}
fn webViewNoOpFinish(url: string, isError: bool, errorMessage: string) {}

let WEBVIEWS = new Map<i64, WebViewSlot>()

// WebKitLoadEvent enum values (webkit/WebKitWebView.h).
const LOAD_COMMITTED: u32 = 2 as u32
const LOAD_FINISHED:  u32 = 3 as u32

@extern(C) {

// Live URL of the view as an ilang string ("" when none yet). The
// raw `*const char` only appears inside this @extern(C) block.
fn webViewCurrentUrl(view: WebKitWebView): string {
    let p = webkit_web_view_get_uri(view)
    if p as i64 == 0 as i64 { "" } else { stringFromCstr(p) }
}

    /// `load-changed`. The committed phase is the navigation
    /// commit point (the URL is final by then); the finished phase
    /// is a clean load completion.
    pub fn linux_webview_load_changed(view: i64, loadEvent: u32, data: i64) {
        if let some(slot) = WEBVIEWS.get(view) {
            if loadEvent == LOAD_COMMITTED {
                slot.onNavigate(webViewCurrentUrl(slot.view))
            } elif loadEvent == LOAD_FINISHED {
                slot.onLoadFinish(webViewCurrentUrl(slot.view), false, "")
            }
        }
    }
    /// `load-failed`. `failingUri` is a `char*`; `error` a
    /// `GError*` whose `message` field sits at offset 8
    /// (`{ GQuark domain; gint code; gchar *message; }`). Return 0
    /// so WebKit still renders its default error page.
    pub fn linux_webview_load_failed(
        view: i64, loadEvent: u32, failingUri: i64, error: i64, data: i64
    ): i32 {
        if let some(slot) = WEBVIEWS.get(view) {
            let url = if failingUri == 0 as i64 {
                ""
            } else {
                stringFromCstr(failingUri as *const char)
            }
            let msg = if error == 0 as i64 {
                "load failed"
            } else {
                let m = readI64(error as *const void, 8 as i64)
                if m == 0 as i64 { "load failed" } else { stringFromCstr(m as *const char) }
            }
            slot.onLoadFinish(url, true, msg)
        }
        0 as i32
    }
}

@extern(C) {

pub fn createNativeWebView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = match resolveGtkParent(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let view = webkit_web_view_new() as WebKitWebView
    gtk_widget_set_size_request(view as GtkWidget, width as i32, height as i32)
    gtk_widget_set_visible(view as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })
    gtk_fixed_put(parent.fixed, view as GtkWidget, x, y)

    g_signal_connect_load_changed(
        view as GObject, cstrFromString("load-changed"),
        linux_webview_load_changed, 0 as *void, 0 as i64, 0 as u32
    )
    g_signal_connect_load_failed(
        view as GObject, cstrFromString("load-failed"),
        linux_webview_load_failed, 0 as *void, 0 as i64, 0 as u32
    )

    let handle = view as i64
    WEBVIEWS[handle] = new WebViewSlot(view, parent.fixed)
    handle
}

pub fn setWebViewUrl(handle: i64, url: string) {
    if let some(slot) = WEBVIEWS.get(handle) {
        if url == "" { return }
        webkit_web_view_load_uri(slot.view, cstrFromString(url))
    }
}
pub fn readWebViewUrl(handle: i64): string {
    match WEBVIEWS.get(handle) {
        some(slot) { webViewCurrentUrl(slot.view) }
        none       { "" }
    }
}
pub fn webViewLoadHtml(handle: i64, html: string, baseUrl: string) {
    if let some(slot) = WEBVIEWS.get(handle) {
        let base = if baseUrl == "" {
            0 as *const char
        } else {
            cstrFromString(baseUrl)
        }
        webkit_web_view_load_html(slot.view, cstrFromString(html), base)
    }
}
pub fn webViewGoBack(handle: i64) {
    if let some(slot) = WEBVIEWS.get(handle) {
        if webkit_web_view_can_go_back(slot.view) != 0 as i32 {
            webkit_web_view_go_back(slot.view)
        }
    }
}
pub fn webViewGoForward(handle: i64) {
    if let some(slot) = WEBVIEWS.get(handle) {
        if webkit_web_view_can_go_forward(slot.view) != 0 as i32 {
            webkit_web_view_go_forward(slot.view)
        }
    }
}
pub fn webViewReload(handle: i64) {
    if let some(slot) = WEBVIEWS.get(handle) {
        webkit_web_view_reload(slot.view)
    }
}
pub fn webViewCanGoBack(handle: i64): bool {
    match WEBVIEWS.get(handle) {
        some(slot) { webkit_web_view_can_go_back(slot.view) != 0 as i32 }
        none       { false }
    }
}
pub fn webViewCanGoForward(handle: i64): bool {
    match WEBVIEWS.get(handle) {
        some(slot) { webkit_web_view_can_go_forward(slot.view) != 0 as i32 }
        none       { false }
    }
}
pub fn setWebViewFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = WEBVIEWS.get(handle) {
        gtk_widget_set_size_request(slot.view as GtkWidget, width as i32, height as i32)
        gtk_fixed_move(slot.fixed, slot.view as GtkWidget, x, y)
    }
}
pub fn setWebViewVisible(handle: i64, visible: bool) {
    if let some(slot) = WEBVIEWS.get(handle) {
        gtk_widget_set_visible(slot.view as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 })
    }
}
pub fn setOnWebViewNavigate(handle: i64, cb: fn(string)) {
    if let some(slot) = WEBVIEWS.get(handle) { slot.onNavigate = cb }
}
pub fn setOnWebViewLoadFinish(handle: i64, cb: fn(string, bool, string)) {
    if let some(slot) = WEBVIEWS.get(handle) { slot.onLoadFinish = cb }
}

} // @extern(C)
