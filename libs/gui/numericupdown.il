/// Numeric entry with up/down stepper buttons. Cocoa:
/// `NSTextField` + `NSStepper` placed side-by-side; Linux:
/// `GtkSpinButton` (a built-in one-piece widget); Win32: `EDIT`
/// + `msctls_updown32` buddy pair.
///
/// Values are `f64` — pass `step = 1.0` for integer-only use:
///
///     let n = new NumericUpDown()
///     n.min = 0.0; n.max = 100.0; n.step = 1.0
///     n.value = 50.0
///     n.onChange.add(fn(e) { console.log("value =", e.value) })
///     window.add(n)
///
/// `decimalPlaces` controls the rendered precision (0 = integer
/// display, default 0). `wrap` makes `min` and `max` connect:
/// stepping past `max` lands on `min` and vice versa.

use std.events { Signal }
use events {
    NumericChangeEvent,
    FocusEvent, BlurEvent, MouseEvent, KeyEvent,
}
use window { Window, Control, Container }
use gui_impl {
    createNativeNumericUpDown,
    setNumericValue, readNumericValue,
    setNumericRange, setNumericStep,
    setNumericDecimalPlaces, setNumericWrap,
    setNumericFrame, setNumericEnabled, setNumericVisible, setNumericTooltip,
    setOnNumericChange,
    setOnNumericFocus, setOnNumericBlur,
    setOnNumericMouseEnter, setOnNumericMouseLeave,
    setOnNumericMouseDown, setOnNumericMouseUp, setOnNumericContextMenu,
    setOnNumericMouseWheel,
    setOnNumericKeyDown, setOnNumericKeyUp,
    focusNumeric, blurNumeric,
}

pub class NumericUpDown : Control {
    handle: i64
    _value:         f64
    _min:           f64
    _max:           f64
    _step:          f64
    _decimalPlaces: i32
    _wrap:          bool
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string
    pub onChange: Signal<NumericChangeEvent>
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
        this.handle         = -1
        this._value         = 0.0
        this._min           = 0.0
        this._max           = 100.0
        this._step          = 1.0
        this._decimalPlaces = 0 as i32
        this._wrap          = false
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 96.0
        this._height    = 24.0
        this._isEnabled = true
        this._isVisible = true
        this._tooltip   = ""
        this.onChange   = new Signal<NumericChangeEvent>()
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

    pub get value(): f64 {
        if this.handle >= 0 { this._value = readNumericValue(this.handle) }
        this._value
    }
    pub set value(v: f64) {
        this._value = v
        if this.handle >= 0 { setNumericValue(this.handle, v) }
    }

    pub get min(): f64 { this._min }
    pub set min(v: f64) {
        this._min = v
        if this.handle >= 0 { setNumericRange(this.handle, this._min, this._max) }
    }
    pub get max(): f64 { this._max }
    pub set max(v: f64) {
        this._max = v
        if this.handle >= 0 { setNumericRange(this.handle, this._min, this._max) }
    }
    pub get step(): f64 { this._step }
    pub set step(v: f64) {
        this._step = v
        if this.handle >= 0 { setNumericStep(this.handle, v) }
    }
    pub get decimalPlaces(): i32 { this._decimalPlaces }
    pub set decimalPlaces(v: i32) {
        this._decimalPlaces = v
        if this.handle >= 0 { setNumericDecimalPlaces(this.handle, v) }
    }
    pub get wrap(): bool { this._wrap }
    pub set wrap(v: bool) {
        this._wrap = v
        if this.handle >= 0 { setNumericWrap(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setNumericFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setNumericFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setNumericFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setNumericFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setNumericEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setNumericVisible(this.handle, v) }
    }
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setNumericTooltip(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeNumericUpDown(
            parent.nativeChildHandle(),
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setNumericRange(this.handle, this._min, this._max)
        setNumericStep(this.handle, this._step)
        setNumericDecimalPlaces(this.handle, this._decimalPlaces)
        setNumericWrap(this.handle, this._wrap)
        setNumericValue(this.handle, this._value)

        setOnNumericChange(this.handle, fn(v: f64) {
            this._value = v
            this.onChange.emit(new NumericChangeEvent(this, v))
        })
        setOnNumericFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnNumericBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnNumericMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnNumericMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnNumericMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnNumericMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnNumericContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnNumericMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        setOnNumericKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnNumericKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        if this._tooltip != "" { setNumericTooltip(this.handle, this._tooltip) }
    }

    pub focus() { if this.handle >= 0 { focusNumeric(this.handle) } }
    pub blur()  { if this.handle >= 0 { blurNumeric(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setNumericFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
