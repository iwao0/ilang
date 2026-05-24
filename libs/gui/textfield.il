/// A single-line editable text field. Backed by the `EDIT`
/// control class on win32 and a bezeled `NSTextField` on macOS.
/// `onChange` fires on every keystroke (DOM `oninput` semantics),
/// not just on commit, so live previews work without polling.
///
///   let t = new gui.TextField()
///   t.text   = "hello"
///   t.x      = 120.0
///   t.y      = 20.0
///   t.width  = 200.0
///   t.height = 24.0
///   t.onChange.add(fn(e: gui.ChangeEvent) { console.log(e.value) })
///   window.addChild(t)

use std.events { Signal }
use event { ChangeEvent }
use window { Window, Control }
use gui_impl {
    createNativeEdit, readEditText,
    setOnEditChange, setEditText, setEditFrame,
    setEditEnabled, setEditVisible,
}

pub class TextField : Control {
    handle: i64
    _text:      string
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    pub onChange: Signal<ChangeEvent>

    pub init() {
        super()
        this.handle     = -1
        this._text      = ""
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 160.0
        this._height    = 24.0
        this._isEnabled = true
        this._isVisible = true
        this.onChange   = new Signal<ChangeEvent>()
    }

    pub get text(): string {
        // Once attached, the native control is the source of truth
        // — the user may have typed since the last setter call.
        if this.handle >= 0 {
            this._text = readEditText(this.handle)
        }
        this._text
    }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setEditText(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setEditEnabled(this.handle, v) }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setEditVisible(this.handle, v) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeEdit(
            window.handle,
            this._text, this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnEditChange(this.handle, fn(v: string) {
            this._text = v
            this.onChange.emit(new ChangeEvent(this, v))
        })
    }
}
