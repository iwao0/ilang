/// A multi-line editable text area. Win32: `EDIT` with
/// `ES_MULTILINE`; Cocoa: `NSTextView` inside an `NSScrollView`;
/// GTK: `GtkTextView` inside a `GtkScrolledWindow`. `onChange`
/// fires on every keystroke, matching `TextField` /
/// DOM `oninput` semantics.
///
///   let t = new gui.TextArea()
///   t.text   = "line 1\nline 2"
///   t.x      = 20.0
///   t.y      = 60.0
///   t.width  = 320.0
///   t.height = 160.0
///   window.addChild(t)

use std.events { Signal }
use events { ChangeEvent, FocusEvent, BlurEvent, MouseEvent, KeyEvent }
use window { Window, Control, Container }
use gui_impl {
    createNativeTextArea, readTextAreaText, setTextAreaText, setTextAreaFrame,
    setTextAreaEnabled, setTextAreaVisible, setTextAreaTooltip,
    setTextAreaReadOnly,
    setOnTextAreaChange,
    setOnTextAreaFocus, setOnTextAreaBlur,
    setOnTextAreaMouseEnter, setOnTextAreaMouseLeave,
    setOnTextAreaMouseDown, setOnTextAreaMouseUp, setOnTextAreaContextMenu,
    setOnTextAreaMouseWheel,
    setOnTextAreaKeyDown, setOnTextAreaKeyUp,
    focusTextArea, blurTextArea,
}

pub class TextArea : Control {
    handle: i64
    _text:       string
    _x:          f64
    _y:          f64
    _width:      f64
    _height:     f64
    _isEnabled:  bool
    _isVisible:  bool
    _tooltip:    string
    _isReadOnly: bool
    pub onChange: Signal<ChangeEvent>
    pub onFocus:  Signal<FocusEvent>
    pub onBlur:   Signal<BlurEvent>
    pub onMouseEnter:  Signal<MouseEvent>
    pub onMouseLeave:  Signal<MouseEvent>
    pub onMouseDown:   Signal<MouseEvent>
    pub onMouseUp:     Signal<MouseEvent>
    pub onContextMenu: Signal<MouseEvent>
    pub onKeyDown:     Signal<KeyEvent>
    pub onKeyUp:       Signal<KeyEvent>
    pub onMouseWheel:  Signal<MouseEvent>

    pub init() {
        super()
        this.handle      = -1
        this._text       = ""
        this._x          = 0.0
        this._y          = 0.0
        this._width      = 240.0
        this._height     = 120.0
        this._isEnabled  = true
        this._isVisible  = true
        this._tooltip    = ""
        this._isReadOnly = false
        this.onChange    = new Signal<ChangeEvent>()
        this.onFocus     = new Signal<FocusEvent>()
        this.onBlur      = new Signal<BlurEvent>()
        this.onMouseEnter  = new Signal<MouseEvent>()
        this.onMouseLeave  = new Signal<MouseEvent>()
        this.onMouseDown   = new Signal<MouseEvent>()
        this.onMouseUp     = new Signal<MouseEvent>()
        this.onContextMenu = new Signal<MouseEvent>()
        this.onKeyDown     = new Signal<KeyEvent>()
        this.onKeyUp       = new Signal<KeyEvent>()
        this.onMouseWheel  = new Signal<MouseEvent>()
    }

    pub get text(): string {
        if this.handle >= 0 {
            this._text = readTextAreaText(this.handle)
        }
        this._text
    }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setTextAreaText(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setTextAreaFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setTextAreaFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setTextAreaFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setTextAreaFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setTextAreaEnabled(this.handle, v) }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setTextAreaVisible(this.handle, v) }
    }

    /// Hover tooltip. Empty string clears it.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setTextAreaTooltip(this.handle, v) }
    }

    /// Read-only mode. Selection / scrolling still works; only
    /// typed input is suppressed.
    pub get isReadOnly(): bool { this._isReadOnly }
    pub set isReadOnly(v: bool) {
        this._isReadOnly = v
        if this.handle >= 0 { setTextAreaReadOnly(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeTextArea(
            parent.nativeChildHandle(),
            this._text, this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnTextAreaChange(this.handle, fn(v: string) {
            this._text = v
            this.onChange.emit(new ChangeEvent(this, v))
        })
        setOnTextAreaFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnTextAreaBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnTextAreaMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnTextAreaMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnTextAreaMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnTextAreaMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnTextAreaContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnTextAreaKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnTextAreaKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnTextAreaMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        if this._tooltip != "" { setTextAreaTooltip(this.handle, this._tooltip) }
        if this._isReadOnly { setTextAreaReadOnly(this.handle, true) }
    }

    pub focus() { if this.handle >= 0 { focusTextArea(this.handle) } }
    pub blur()  { if this.handle >= 0 { blurTextArea(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setTextAreaFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
