/// Read-only progress bar. Win32: `msctls_progress32`; Cocoa:
/// `NSProgressIndicator` (determinate bar). Set `value` within
/// `[min, max]` to advance.

use window { Window, Control }
use gui_impl {
    createNativeProgress,
    setProgressRange, setProgressValue,
    setProgressFrame, setProgressVisible,
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
    _isVisible: bool

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
        this._isVisible = true
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

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setProgressVisible(this.handle, v) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeProgress(
            window.handle,
            this._min, this._max, this._value,
            this._x, this._y, this._width, this._height,
            this._isVisible
        )
    }
}
