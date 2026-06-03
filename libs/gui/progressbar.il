/// Read-only progress bar. Win32: `msctls_progress32`; Cocoa:
/// `NSProgressIndicator` (determinate bar). Set `value` within
/// `[min, max]` to advance.

use window { Window, Control }
use gui_impl {
    createNativeProgress,
    setProgressRange, setProgressValue,
    setProgressFrame, setProgressVisible,
    setProgressEnabled, setProgressTooltip,
}

pub class ProgressBar : Control {
    handle: i64
    _min:       f64
    _max:       f64
    _value:     f64
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string

    pub init() {
        super()
        this.handle     = -1
        this._min       = 0.0
        this._max       = 100.0
        this._value     = 0.0
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 200.0
        this._height    = 18.0
        this._isEnabled = true
        this._isVisible = true
        this._tooltip   = ""
    }

    pub get min(): f64 { this._min }
    pub set min(v: f64) {
        this._min = v
        if this.handle >= 0 { setProgressRange(this.handle, this._min, this._max) }
    }
    pub get max(): f64 { this._max }
    pub set max(v: f64) {
        this._max = v
        if this.handle >= 0 { setProgressRange(this.handle, this._min, this._max) }
    }
    pub get value(): f64 { this._value }
    pub set value(v: f64) {
        this._value = v
        if this.handle >= 0 { setProgressValue(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setProgressFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setProgressFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setProgressFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setProgressFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setProgressEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setProgressVisible(this.handle, v) }
    }

    /// Hover tooltip. Empty string clears it.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setProgressTooltip(this.handle, v) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeProgress(
            window.handle,
            this._min, this._max, this._value,
            this._x, this._y, this._width, this._height,
            this._isVisible
        )
        if !this._isEnabled { setProgressEnabled(this.handle, false) }
        if this._tooltip != "" { setProgressTooltip(this.handle, this._tooltip) }
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setProgressFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
