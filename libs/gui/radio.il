/// Radio button — one member of a mutually-exclusive group. All
/// `RadioButton`s sharing the same `group` string flip off when
/// any one of them is checked.
///
///   let a = new gui.RadioButton(); a.group = "size"; a.text = "S"
///   let b = new gui.RadioButton(); b.group = "size"; b.text = "M"
///   let c = new gui.RadioButton(); c.group = "size"; c.text = "L"
///   a.checked = true
///   window.addChild(a); window.addChild(b); window.addChild(c)
///
/// Mutual exclusion is handled in user space via a process-wide
/// `RADIOS` registry — neither backend has to coordinate native
/// group bits.

use std.events { Signal }
use events { ToggleEvent, FocusEvent, BlurEvent, MouseEvent, KeyEvent }
use window { Window, Control, Container }
use gui_impl {
    createNativeRadio, readRadioChecked,
    setOnRadioToggle, setRadioText, setRadioChecked,
    setRadioFrame, setRadioEnabled, setRadioVisible,
    setRadioTooltip,
    setOnRadioFocus, setOnRadioBlur,
    setOnRadioMouseEnter, setOnRadioMouseLeave,
    setOnRadioMouseDown, setOnRadioMouseUp, setOnRadioContextMenu,
    setOnRadioMouseWheel,
    setOnRadioKeyDown, setOnRadioKeyUp,
    focusRadio, blurRadio,
}

// Process-wide registry of every `RadioButton` ever constructed.
// `RadioButton.attachTo` appends to this list so the toggle
// handler can walk siblings sharing the same `group`.
let RADIOS: RadioButton[] = []

pub class RadioButton : Control {
    handle: i64
    pub group: string
    _text:      string
    _checked:   bool
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string
    pub onToggle: Signal<ToggleEvent>
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
        this.group      = ""
        this._text      = ""
        this._checked   = false
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 160.0
        this._height    = 22.0
        this._isEnabled = true
        this._isVisible = true
        this._tooltip   = ""
        this.onToggle   = new Signal<ToggleEvent>()
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

    pub get text(): string { this._text }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setRadioText(this.handle, v) }
    }
    pub get checked(): bool {
        if this.handle >= 0 { this._checked = readRadioChecked(this.handle) }
        this._checked
    }
    pub set checked(v: bool) {
        this._checked = v
        if this.handle >= 0 { setRadioChecked(this.handle, v) }
        // Programmatic check should also drive sibling mutex.
        if v { this._unselectSiblings() }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setRadioFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setRadioFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setRadioFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setRadioFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setRadioEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setRadioVisible(this.handle, v) }
    }

    /// Hover tooltip. Empty string clears it.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setRadioTooltip(this.handle, v) }
    }

    _unselectSiblings() {
        let n = RADIOS.length
        let i: i64 = 0
        while i < n {
            let r = RADIOS[i]
            if r.group == this.group && r.handle != this.handle {
                if r.handle >= 0 { setRadioChecked(r.handle, false) }
                r._checked = false
            }
            i = i + 1
        }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeRadio(
            parent.nativeChildHandle(),
            this._text, this._checked,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        RADIOS.push(this)
        setOnRadioToggle(this.handle, fn(checked: bool) {
            this._checked = checked
            if checked { this._unselectSiblings() }
            this.onToggle.emit(new ToggleEvent(this, checked))
        })
        setOnRadioFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnRadioBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnRadioMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnRadioMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnRadioMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnRadioMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnRadioContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnRadioMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        setOnRadioKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnRadioKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        if this._tooltip != "" { setRadioTooltip(this.handle, this._tooltip) }
    }

    pub focus() { if this.handle >= 0 { focusRadio(this.handle) } }
    pub blur()  { if this.handle >= 0 { blurRadio(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setRadioFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
