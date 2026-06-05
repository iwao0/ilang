/// Indeterminate circular spinner — "loading" indicator. Cocoa:
/// `NSProgressIndicator` in spinner style. Linux / Win32 are
/// stubbed in this round (the call surface exists so cross-
/// platform code compiles, but no widget is rendered).
///
///     let a = new gui.ActivityIndicator()
///     a.x = 20.0; a.y = 20.0
///     a.width = 24.0; a.height = 24.0
///     window.addChild(a)
///     a.start()                 // begin spinning
///     ...
///     a.stop()                  // freeze
///
/// Use a small frame (16 / 24 / 32 px is conventional on macOS);
/// the spinner sizes itself inside whatever rect you give it.

use window { Window, Control, Container }
use gui_impl {
    createNativeActivityIndicator,
    startActivityIndicator, stopActivityIndicator,
    setActivityIndicatorFrame,
    setActivityIndicatorVisible, setActivityIndicatorTooltip,
}

pub class ActivityIndicator : Control {
    handle: i64
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isVisible: bool
    _tooltip:   string
    // Cached "was start() called before attach?" flag — flushed
    // into the native widget during `attachTo` so callers can set
    // it up before adding to the window.
    _pendingStart: bool

    pub init() {
        super()
        this.handle     = -1
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 24.0
        this._height    = 24.0
        this._isVisible = true
        this._tooltip   = ""
        this._pendingStart = false
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setActivityIndicatorFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setActivityIndicatorFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setActivityIndicatorFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setActivityIndicatorFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setActivityIndicatorVisible(this.handle, v) }
    }

    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setActivityIndicatorTooltip(this.handle, v) }
    }

    /// Begin the spin animation. No-op until attached; the call
    /// is remembered and replayed during `attachTo`.
    pub start() {
        if this.handle >= 0 {
            startActivityIndicator(this.handle)
        } else {
            this._pendingStart = true
        }
    }

    /// Freeze the spinner. Safe before attach (clears the pending
    /// start flag).
    pub stop() {
        if this.handle >= 0 {
            stopActivityIndicator(this.handle)
        } else {
            this._pendingStart = false
        }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeActivityIndicator(
            parent.nativeChildHandle(),
            this._x, this._y, this._width, this._height,
            this._isVisible
        )
        if this._tooltip != "" { setActivityIndicatorTooltip(this.handle, this._tooltip) }
        if this._pendingStart { startActivityIndicator(this.handle) }
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setActivityIndicatorFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
