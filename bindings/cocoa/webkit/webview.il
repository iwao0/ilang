/// WKWebView + supporting classes. Linked against
/// `WebKit.framework` at JIT / AOT time.
///
/// Round 1 surfaces enough of the API to host a navigation panel:
/// `loadRequest:`, `loadHTMLString:baseURL:`, the URL / canGoBack
/// / canGoForward properties, the goBack / goForward / reload
/// commands, and the `WKNavigationDelegate` protocol's finish /
/// fail / commit hooks. JS bridge (`evaluateJavaScript:` /
/// `WKScriptMessageHandler` / `WKUserContentController`) is not
/// included.

use foundation { NSObject, NSString, NSRect, NSURL, NSURLRequest, NSError }
use appkit { NSView }

@extern(ObjC, "/System/Library/Frameworks/WebKit.framework/WebKit") {
    /// `WKWebViewConfiguration` — settings bundle passed at
    /// init. Default-constructed instance is enough for plain
    /// HTML + script-disabled pages; the JS bridge knobs are
    /// deferred to a follow-up.
    @objc pub class WKWebViewConfiguration : NSObject {
        @objc("alloc")
        pub static alloc(): WKWebViewConfiguration
        @objc("init")
        pub init(): WKWebViewConfiguration
    }

    /// `WKNavigation` — opaque token returned by `loadRequest:`
    /// and friends; identifies one in-flight navigation across
    /// the delegate callbacks. No methods exposed in Round 1.
    @objc pub class WKNavigation : NSObject {}

    /// `WKNavigationDelegate` — navigation-progress callbacks.
    /// Implement a subclass and pass its instance to
    /// `WKWebView.setNavigationDelegate`. All three selectors are
    /// optional at the ObjC level; ilang subclasses can override
    /// only the ones they need.
    @objc pub interface WKNavigationDelegate {
        @objc("webView:didCommitNavigation:")
        didCommitNavigation?(webView: NSObject, navigation: WKNavigation)
        @objc("webView:didFinishNavigation:")
        didFinishNavigation?(webView: NSObject, navigation: WKNavigation)
        @objc("webView:didFailNavigation:withError:")
        didFailNavigation?(
            webView: NSObject, navigation: WKNavigation, error: NSError
        )
    }

    /// `WKWebView` — embeddable web page. `loadRequest:` kicks
    /// off a real network load; `loadHTMLString:baseURL:` renders
    /// arbitrary HTML with optional resource base. The active URL
    /// reads back via `URL` once a navigation has committed.
    @objc pub class WKWebView : NSView {
        @objc("alloc")
        pub static alloc(): WKWebView
        @objc("initWithFrame:configuration:")
        pub init(frame: NSRect, configuration: WKWebViewConfiguration): WKWebView

        @objc("setNavigationDelegate:")
        pub setNavigationDelegate(d: WKNavigationDelegate)
        @objc("setNavigationDelegate:")
        pub setNavigationDelegate(d: NSObject)

        @objc("URL")
        pub get URL(): NSURL
        @objc("title")
        pub get title(): NSString
        @objc("canGoBack")
        pub get canGoBack(): bool
        @objc("canGoForward")
        pub get canGoForward(): bool

        @objc("loadRequest:")
        pub loadRequest(req: NSURLRequest): WKNavigation
        /// `baseURL` is `nullable NSURL` in ObjC — pass `nil`
        /// (typed as `NSObject`) to render the HTML without a
        /// base, or wrap an `NSURL` in `(... as NSObject)`.
        @objc("loadHTMLString:baseURL:")
        pub loadHTMLString(html: NSString, baseURL: NSObject): WKNavigation

        @objc("reload")
        pub reload(): WKNavigation
        @objc("goBack")
        pub goBack(): WKNavigation
        @objc("goForward")
        pub goForward(): WKNavigation
    }
}
