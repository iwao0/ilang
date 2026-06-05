/// Clickable text label. Cocoa: a borderless `NSTextField` styled
/// in the system link colour with mouse-down forwarded into
/// `onClick`; Linux: `GtkLinkButton`; Win32: custom-drawn
/// `IlangGuiLink` child window — there is no native one-line
/// hyperlink control that draws as plain text rather than as a
/// SysLink hit-region.
///
///     let l = new gui.LinkLabel()
///     l.text = "ilang on GitHub"
///     l.url  = "https://github.com/example/ilang"
///     l.x = 20.0; l.y = 200.0
///     l.width = 240.0; l.height = 18.0
///     window.addChild(l)
///
/// Clicking opens `url` in the OS-default handler (LaunchServices
/// on macOS, `gtk_show_uri` on Linux, `ShellExecuteW` on Win32).
/// `onClick` fires alongside the open so callers can log or trace
/// the activation. If `url` is empty, only `onClick` fires — the
/// label is then a plain clickable surface.

use std.events { Signal }
use events { MouseEvent }
use window { Window, Control, Container }
use gui_impl {
    createNativeLinkLabel,
    setLinkLabelText, setLinkLabelUrl,
    setLinkLabelFrame, setLinkLabelEnabled, setLinkLabelVisible,
    setLinkLabelTooltip,
    setOnLinkLabelClick,
}

pub class LinkLabel : Control {
    handle: i64
    _text:      string
    _url:       string
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string
    pub onClick: Signal<MouseEvent>

    pub init() {
        super()
        this.handle     = -1
        this._text      = ""
        this._url       = ""
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 200.0
        this._height    = 20.0
        this._isEnabled = true
        this._isVisible = true
        this._tooltip   = ""
        this.onClick    = new Signal<MouseEvent>()
    }

    pub get text(): string { this._text }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setLinkLabelText(this.handle, v) }
    }

    /// Target URL opened on click. Empty disables auto-open and
    /// makes the label a plain `onClick` surface.
    pub get url(): string { this._url }
    pub set url(v: string) {
        this._url = v
        if this.handle >= 0 { setLinkLabelUrl(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setLinkLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setLinkLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setLinkLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setLinkLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setLinkLabelEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setLinkLabelVisible(this.handle, v) }
    }

    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setLinkLabelTooltip(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeLinkLabel(
            parent.nativeChildHandle(),
            this._text, this._url,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnLinkLabelClick(this.handle, fn(x: f64, y: f64) {
            this.onClick.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        if this._tooltip != "" { setLinkLabelTooltip(this.handle, this._tooltip) }
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setLinkLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
