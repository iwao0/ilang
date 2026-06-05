/// WebView. `WKWebView` from `WebKit.framework`, with a
/// `WKNavigationDelegate` subclass routing commit / finish /
/// fail into `onNavigate` / `onLoadFinish`.

use cocoa {
    NSObject, NSString, NSURL, NSURLRequest, NSError,
    NSView, nsstr, ilstr, nil, makeRect,
}
use webkit {
    WKWebView, WKWebViewConfiguration, WKNavigation, WKNavigationDelegate,
}
use core { resolveParentView }

class WebViewSlot {
    pub view:    WKWebView
    pub onNavigate:   fn(string)
    pub onLoadFinish: fn(string, bool, string)
    pub delegate: NSObject
    pub init(view: WKWebView, delegate: NSObject) {
        this.view = view
        this.onNavigate   = webViewNoOpUrl
        this.onLoadFinish = webViewNoOpFinish
        this.delegate = delegate
    }
}

fn webViewNoOpUrl(url: string) {}
fn webViewNoOpFinish(url: string, isError: bool, errorMessage: string) {}

let WEBVIEWS = new Map<i64, WebViewSlot>()

fn currentUrlOf(view: WKWebView): string {
    let u = view.URL
    if u.handle == 0 as i64 { return "" }
    ilstr(u.absoluteString)
}

/// `WKNavigationDelegate` implementation. ObjC dispatches the
/// `webView:did...` selectors at the delegate; we route them
/// back to the slot keyed by the web view handle.
class WebViewNavDelegate : NSObject {
    pub didCommitNavigation(webView: NSObject, navigation: WKNavigation) {
        if let some(slot) = WEBVIEWS.get(webView.handle) {
            slot.onNavigate(currentUrlOf(slot.view))
        }
    }
    pub didFinishNavigation(webView: NSObject, navigation: WKNavigation) {
        if let some(slot) = WEBVIEWS.get(webView.handle) {
            slot.onLoadFinish(currentUrlOf(slot.view), false, "")
        }
    }
    pub didFailNavigation(webView: NSObject, navigation: WKNavigation, error: NSError) {
        if let some(slot) = WEBVIEWS.get(webView.handle) {
            slot.onLoadFinish(currentUrlOf(slot.view), true, ilstr(error.localizedDescription))
        }
    }
}

pub fn createNativeWebView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let cfg = WKWebViewConfiguration.alloc().init()
    let view = WKWebView.alloc().init(makeRect(x, y, width, height), cfg)
    (view as NSView).setHidden(!visible)

    WebViewNavDelegate.register()
    let d = WebViewNavDelegate.alloc().init()
    view.setNavigationDelegate(d as NSObject)

    parent.addSubview(view as NSView)

    let handle = view.handle
    WEBVIEWS[handle] = new WebViewSlot(view, d as NSObject)
    handle
}

pub fn setWebViewUrl(handle: i64, url: string) {
    if let some(slot) = WEBVIEWS.get(handle) {
        if url == "" { return }
        let u = NSURL.fromString(nsstr(url))
        if u.handle == 0 as i64 { return }
        let req = NSURLRequest.requestWithURL(u)
        let _ = slot.view.loadRequest(req)
    }
}

pub fn readWebViewUrl(handle: i64): string {
    match WEBVIEWS.get(handle) {
        some(slot) { currentUrlOf(slot.view) }
        none       { "" }
    }
}

pub fn webViewLoadHtml(handle: i64, html: string, baseUrl: string) {
    if let some(slot) = WEBVIEWS.get(handle) {
        let base: NSObject = if baseUrl == "" {
            nil
        } else {
            NSURL.fromString(nsstr(baseUrl)) as NSObject
        }
        let _ = slot.view.loadHTMLString(nsstr(html), base)
    }
}

pub fn webViewGoBack(handle: i64) {
    if let some(slot) = WEBVIEWS.get(handle) {
        if slot.view.canGoBack { let _ = slot.view.goBack() }
    }
}
pub fn webViewGoForward(handle: i64) {
    if let some(slot) = WEBVIEWS.get(handle) {
        if slot.view.canGoForward { let _ = slot.view.goForward() }
    }
}
pub fn webViewReload(handle: i64) {
    if let some(slot) = WEBVIEWS.get(handle) {
        let _ = slot.view.reload()
    }
}
pub fn webViewCanGoBack(handle: i64): bool {
    match WEBVIEWS.get(handle) {
        some(slot) { slot.view.canGoBack }
        none       { false }
    }
}
pub fn webViewCanGoForward(handle: i64): bool {
    match WEBVIEWS.get(handle) {
        some(slot) { slot.view.canGoForward }
        none       { false }
    }
}

pub fn setWebViewFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = WEBVIEWS.get(handle) {
        (slot.view as NSView).setFrame(makeRect(x, y, width, height))
    }
}
pub fn setWebViewVisible(handle: i64, visible: bool) {
    if let some(slot) = WEBVIEWS.get(handle) {
        (slot.view as NSView).setHidden(!visible)
    }
}

pub fn setOnWebViewNavigate(handle: i64, cb: fn(string)) {
    if let some(slot) = WEBVIEWS.get(handle) { slot.onNavigate = cb }
}
pub fn setOnWebViewLoadFinish(handle: i64, cb: fn(string, bool, string)) {
    if let some(slot) = WEBVIEWS.get(handle) { slot.onLoadFinish = cb }
}
