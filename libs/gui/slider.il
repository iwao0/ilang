/// Continuous numeric slider over `[min, max]`. Win32:
/// `msctls_trackbar32`; Cocoa: `NSSlider` (continuous). `onChange`
/// fires live while the thumb is being dragged.

use std.events { Signal }
use events { SliderEvent }
use window { Window, Control }
use gui_impl {
    createNativeSlider, readSliderValue,
    setOnSliderChange, setSliderRange, setSliderValue,
    setSliderFrame, setSliderEnabled, setSliderVisible,
}

pub class Slider : Control {
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
    pub onChange: Signal<SliderEvent>

    pub init() {
        super()
        this.handle     = -1
        this._min       = 0.0
        this._max       = 100.0
        this._value     = 0.0
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 200.0
        this._height    = 26.0
        this._isEnabled = true
        this._isVisible = true
        this.onChange   = new Signal<SliderEvent>()
    }

    pub get min(): f64 { this._min }
    pub set min(v: f64) {
        this._min = v
        if this.handle >= 0 { setSliderRange(this.handle, this._min, this._max) }
    }
    pub get max(): f64 { this._max }
    pub set max(v: f64) {
        this._max = v
        if this.handle >= 0 { setSliderRange(this.handle, this._min, this._max) }
    }
    pub get value(): f64 {
        if this.handle >= 0 { this._value = readSliderValue(this.handle) }
        this._value
    }
    pub set value(v: f64) {
        this._value = v
        if this.handle >= 0 { setSliderValue(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setSliderFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setSliderFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setSliderFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setSliderFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setSliderEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setSliderVisible(this.handle, v) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeSlider(
            window.handle,
            this._min, this._max, this._value,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnSliderChange(this.handle, fn(v: f64) {
            this._value = v
            this.onChange.emit(new SliderEvent(this, v))
        })
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setSliderFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
