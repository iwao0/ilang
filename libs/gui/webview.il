/// Embedded web view. Cocoa: `WKWebView`. Linux / Win32 are
/// stubbed in this round (the call surface exists so cross-
/// platform code compiles, but no widget is rendered and the
/// commands are no-ops).
///
///     let v = new gui.WebView()
///     v.x = 0.0; v.y = 0.0
///     v.width = 640.0; v.height = 480.0
///     window.addChild(v)
///     v.url = "https://example.com"
///     v.onLoadFinish.add(fn(e: gui.NavigateEvent) {
///         if e.isError {
///             console.log("load failed:", e.errorMessage)
///         } else {
///             console.log("loaded:", e.url)
///         }
///     })
///
/// `url = "..."` issues a real network request. For inline HTML
/// use `loadHtml(html, baseUrl)` — `baseUrl` resolves relative
/// asset references in the HTML; pass `""` to disable.
/// `onNavigate` fires once per commit (after the server response
/// is in); `onLoadFinish` fires on success **and** failure, with
/// the failure path setting `isError = true`.

use std.events { Signal }
use events { NavigateEvent }
use window { Window, Control, Container }
use gui_impl {
    createNativeWebView,
    setWebViewUrl, readWebViewUrl,
    webViewLoadHtml,
    webViewGoBack, webViewGoForward, webViewReload,
    webViewCanGoBack, webViewCanGoForward,
    setWebViewFrame, setWebViewVisible,
    setOnWebViewNavigate, setOnWebViewLoadFinish,
}

pub class WebView : Control {
    handle: i64
    _url:       string
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isVisible: bool
    pub onNavigate:   Signal<NavigateEvent>
    pub onLoadFinish: Signal<NavigateEvent>

    pub init() {
        super()
        this.handle     = -1
        this._url       = ""
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 480.0
        this._height    = 320.0
        this._isVisible = true
        this.onNavigate   = new Signal<NavigateEvent>()
        this.onLoadFinish = new Signal<NavigateEvent>()
    }

    /// The currently-displayed URL. Setting it kicks off a real
    /// network load; reading returns the URL the view has
    /// committed to (may differ from a freshly-set value until
    /// the navigation lands).
    pub get url(): string {
        if this.handle >= 0 { this._url = readWebViewUrl(this.handle) }
        this._url
    }
    pub set url(v: string) {
        this._url = v
        if this.handle >= 0 { setWebViewUrl(this.handle, v) }
    }

    /// Render `html` directly. `baseUrl` resolves relative
    /// references (img / link / a) — pass `""` to disable.
    pub loadHtml(html: string, baseUrl: string) {
        if this.handle >= 0 { webViewLoadHtml(this.handle, html, baseUrl) }
    }

    pub goBack() {
        if this.handle >= 0 { webViewGoBack(this.handle) }
    }
    pub goForward() {
        if this.handle >= 0 { webViewGoForward(this.handle) }
    }
    pub reload() {
        if this.handle >= 0 { webViewReload(this.handle) }
    }

    pub get canGoBack(): bool {
        if this.handle >= 0 { webViewCanGoBack(this.handle) } else { false }
    }
    pub get canGoForward(): bool {
        if this.handle >= 0 { webViewCanGoForward(this.handle) } else { false }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setWebViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setWebViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setWebViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setWebViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setWebViewVisible(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeWebView(
            parent.nativeChildHandle(),
            this._x, this._y, this._width, this._height,
            this._isVisible
        )
        setOnWebViewNavigate(this.handle, fn(url: string) {
            this.onNavigate.emit(new NavigateEvent(this, url, false, ""))
        })
        setOnWebViewLoadFinish(this.handle,
            fn(url: string, isError: bool, errorMessage: string) {
                this.onLoadFinish.emit(
                    new NavigateEvent(this, url, isError, errorMessage)
                )
            })
        if this._url != "" { setWebViewUrl(this.handle, this._url) }
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setWebViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
