/// Continuous numeric slider over `[min, max]`. Win32:
/// `msctls_trackbar32`; Cocoa: `NSSlider` (continuous). `onChange`
/// fires live while the thumb is being dragged.

use std.events { Signal }
use events { SliderEvent, FocusEvent, BlurEvent, MouseEvent, KeyEvent }
use window { Window, Control }
use gui_impl {
    createNativeSlider, readSliderValue,
    setOnSliderChange, setOnSliderRelease,
    setSliderRange, setSliderValue,
    setSliderTickInterval, setSliderPageStep,
    setSliderFrame, setSliderEnabled, setSliderVisible,
    setSliderTooltip,
    setOnSliderFocus, setOnSliderBlur,
    setOnSliderMouseEnter, setOnSliderMouseLeave,
    setOnSliderMouseDown, setOnSliderMouseUp, setOnSliderContextMenu,
    setOnSliderMouseWheel,
    setOnSliderKeyDown, setOnSliderKeyUp,
    focusSlider, blurSlider,
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
    _tooltip:   string
    _tickInterval: f64
    _pageStep:     f64
    pub onChange: Signal<SliderEvent>
    /// Fires once when the user releases the slider thumb after a
    /// drag. Use this instead of `onChange` when you only want the
    /// final value (e.g. firing an expensive query off the commit
    /// rather than every intermediate position).
    pub onRelease: Signal<SliderEvent>
    pub onFocus:  Signal<FocusEvent>
    pub onBlur:   Signal<BlurEvent>
    pub onMouseEnter:  Signal<MouseEvent>
    pub onMouseLeave:  Signal<MouseEvent>
    pub onMouseDown:   Signal<MouseEvent>
    pub onMouseUp:     Signal<MouseEvent>
    pub onContextMenu: Signal<MouseEvent>
    pub onMouseWheel:  Signal<MouseEvent>
    pub onKeyDown:     Signal<KeyEvent>
    pub onKeyUp:       Signal<KeyEvent>

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
        this._tooltip   = ""
        this._tickInterval = 0.0
        this._pageStep     = 0.0
        this.onChange   = new Signal<SliderEvent>()
        this.onRelease  = new Signal<SliderEvent>()
        this.onFocus    = new Signal<FocusEvent>()
        this.onBlur     = new Signal<BlurEvent>()
        this.onMouseEnter  = new Signal<MouseEvent>()
        this.onMouseLeave  = new Signal<MouseEvent>()
        this.onMouseDown   = new Signal<MouseEvent>()
        this.onMouseUp     = new Signal<MouseEvent>()
        this.onContextMenu = new Signal<MouseEvent>()
        this.onMouseWheel  = new Signal<MouseEvent>()
        this.onKeyDown     = new Signal<KeyEvent>()
        this.onKeyUp       = new Signal<KeyEvent>()
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

    /// Hover tooltip. Empty string clears it.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setSliderTooltip(this.handle, v) }
    }

    /// Spacing between tick marks in slider value units. `0.0`
    /// (default) draws no ticks. macOS approximates the request
    /// via `setNumberOfTickMarks((max-min)/interval)`; Win32 calls
    /// `TBM_SETTICFREQ`; GTK lays out evenly-spaced
    /// `gtk_scale_add_mark` entries.
    pub get tickInterval(): f64 { this._tickInterval }
    pub set tickInterval(v: f64) {
        this._tickInterval = v
        if this.handle >= 0 {
            setSliderTickInterval(this.handle, this._min, this._max, v)
        }
    }

    /// PageUp / PageDown / mouse-on-track keyboard increment.
    /// `0.0` keeps the platform default. Cocoa has no equivalent —
    /// NSSlider doesn't honour PgUp/PgDn — so the setter is a
    /// no-op on macOS.
    pub get pageStep(): f64 { this._pageStep }
    pub set pageStep(v: f64) {
        this._pageStep = v
        if this.handle >= 0 { setSliderPageStep(this.handle, v) }
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
        setOnSliderRelease(this.handle, fn(v: f64) {
            this._value = v
            this.onRelease.emit(new SliderEvent(this, v))
        })
        setOnSliderFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnSliderBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnSliderMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnSliderMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnSliderMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnSliderMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnSliderContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnSliderMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        setOnSliderKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnSliderKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        if this._tooltip != "" { setSliderTooltip(this.handle, this._tooltip) }
        if this._tickInterval > 0.0 {
            setSliderTickInterval(this.handle, this._min, this._max, this._tickInterval)
        }
        if this._pageStep > 0.0 {
            setSliderPageStep(this.handle, this._pageStep)
        }
    }

    pub focus() { if this.handle >= 0 { focusSlider(this.handle) } }
    pub blur()  { if this.handle >= 0 { blurSlider(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setSliderFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
