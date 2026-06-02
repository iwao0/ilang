/// A boolean toggle with an inline title. Win32: `BUTTON` class
/// with `BS_AUTOCHECKBOX`; Cocoa: `NSButton` with
/// `setButtonType(NSButtonTypeSwitch)`. `onToggle` fires whenever
/// the checked state flips.

use std.events { Signal }
use events { ToggleEvent }
use window { Window, Control }
use gui_impl {
    createNativeCheckbox, readCheckboxChecked,
    setOnCheckboxToggle, setCheckboxText, setCheckboxChecked,
    setCheckboxFrame, setCheckboxEnabled, setCheckboxVisible,
}

pub class Checkbox : Control {
    handle: i64
    _text:      string
    _checked:   bool
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    pub onToggle: Signal<ToggleEvent>

    pub init() {
        super()
        this.handle     = -1
        this._text      = ""
        this._checked   = false
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 160.0
        this._height    = 22.0
        this._isEnabled = true
        this._isVisible = true
        this.onToggle   = new Signal<ToggleEvent>()
    }

    pub get text(): string { this._text }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setCheckboxText(this.handle, v) }
    }
    pub get checked(): bool {
        if this.handle >= 0 { this._checked = readCheckboxChecked(this.handle) }
        this._checked
    }
    pub set checked(v: bool) {
        this._checked = v
        if this.handle >= 0 { setCheckboxChecked(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setCheckboxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setCheckboxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setCheckboxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setCheckboxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setCheckboxEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setCheckboxVisible(this.handle, v) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeCheckbox(
            window.handle,
            this._text, this._checked,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnCheckboxToggle(this.handle, fn(checked: bool) {
            this._checked = checked
            this.onToggle.emit(new ToggleEvent(this, checked))
        })
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setCheckboxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
