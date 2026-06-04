/// On/off toggle switch. Cocoa: `NSSwitch` (macOS 10.15+);
/// Linux: `GtkSwitch`; Win32: custom-drawn (no native widget —
/// `Windows.UI.Xaml.Controls.ToggleSwitch` is UWP-only).
///
/// Functionally a `Checkbox` without a label — the switch's track
/// and thumb carry the state visually. If a caption is needed,
/// place a `Label` next to the switch yourself:
///
///     let row = new gui.HBox()
///     row.addChild(new gui.Label("Enable preview"))
///     let sw = new gui.Switch()
///     sw.checked = true
///     row.addChild(sw)
///
/// `onToggle` fires whenever `checked` flips.

use std.events { Signal }
use events { ToggleEvent, FocusEvent, BlurEvent, MouseEvent, KeyEvent }
use window { Window, Control, Container }
use gui_impl {
    createNativeSwitch, readSwitchChecked,
    setOnSwitchToggle, setSwitchChecked,
    setSwitchFrame, setSwitchEnabled, setSwitchVisible,
    setSwitchTooltip,
    setOnSwitchFocus, setOnSwitchBlur,
    setOnSwitchMouseEnter, setOnSwitchMouseLeave,
    setOnSwitchMouseDown, setOnSwitchMouseUp, setOnSwitchContextMenu,
    setOnSwitchMouseWheel,
    setOnSwitchKeyDown, setOnSwitchKeyUp,
    focusSwitch, blurSwitch,
}

pub class Switch : Control {
    handle: i64
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
        this._checked   = false
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 38.0
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

    pub get checked(): bool {
        if this.handle >= 0 { this._checked = readSwitchChecked(this.handle) }
        this._checked
    }
    pub set checked(v: bool) {
        this._checked = v
        if this.handle >= 0 { setSwitchChecked(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setSwitchFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setSwitchFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setSwitchFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setSwitchFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setSwitchEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setSwitchVisible(this.handle, v) }
    }

    /// Hover tooltip. Empty string clears it.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setSwitchTooltip(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeSwitch(
            parent.nativeChildHandle(),
            this._checked,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnSwitchToggle(this.handle, fn(checked: bool) {
            this._checked = checked
            this.onToggle.emit(new ToggleEvent(this, checked))
        })
        setOnSwitchFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnSwitchBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnSwitchMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnSwitchMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnSwitchMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnSwitchMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnSwitchContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnSwitchMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        setOnSwitchKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnSwitchKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        if this._tooltip != "" { setSwitchTooltip(this.handle, this._tooltip) }
    }

    pub focus() { if this.handle >= 0 { focusSwitch(this.handle) } }
    pub blur()  { if this.handle >= 0 { blurSwitch(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setSwitchFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
