/// A read-only text label. Backed by `STATIC` on win32 and a
/// configured `NSTextField` on macOS. Mouse events (enter / leave /
/// down / up / context-menu / wheel) are exposed so labels can act
/// as clickable surfaces.
///
///   let l = new gui.Label()
///   l.text   = "Name:"
///   l.x      = 20.0
///   l.y      = 24.0
///   l.width  = 80.0
///   l.height = 20.0
///   window.addChild(l)

use std.events { Signal }
use events { MouseEvent }
use window { Window, Control }
use gui_impl {
    createNativeLabel,
    setLabelText, setLabelFrame, setLabelVisible, setLabelSelectable,
    setLabelTooltip,
    setOnLabelMouseEnter, setOnLabelMouseLeave,
    setOnLabelMouseDown, setOnLabelMouseUp, setOnLabelContextMenu,
    setOnLabelMouseWheel,
}

pub class Label : Control {
    handle: i64
    _text:       string
    _x:          f64
    _y:          f64
    _width:      f64
    _height:     f64
    _isVisible:  bool
    /// macOS NSTextField hangs an I-beam cursor rect off any
    /// selectable label, which reads as "this is editable text"
    /// even on a plain caption. Default `false` keeps the arrow
    /// cursor; flip to `true` only when the label genuinely
    /// presents user-copyable content.
    _selectable: bool
    _tooltip:    string
    pub onMouseEnter:  Signal<MouseEvent>
    pub onMouseLeave:  Signal<MouseEvent>
    pub onMouseDown:   Signal<MouseEvent>
    pub onMouseUp:     Signal<MouseEvent>
    pub onContextMenu: Signal<MouseEvent>
    pub onMouseWheel:  Signal<MouseEvent>

    pub init() {
        super()
        this.handle      = -1
        this._text       = ""
        this._x          = 0.0
        this._y          = 0.0
        this._width      = 80.0
        this._height     = 20.0
        this._isVisible  = true
        this._selectable = false
        this._tooltip    = ""
        this.onMouseEnter  = new Signal<MouseEvent>()
        this.onMouseLeave  = new Signal<MouseEvent>()
        this.onMouseDown   = new Signal<MouseEvent>()
        this.onMouseUp     = new Signal<MouseEvent>()
        this.onContextMenu = new Signal<MouseEvent>()
        this.onMouseWheel  = new Signal<MouseEvent>()
    }

    pub get text(): string { this._text }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setLabelText(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setLabelVisible(this.handle, v) }
    }

    pub get selectable(): bool { this._selectable }
    pub set selectable(v: bool) {
        this._selectable = v
        if this.handle >= 0 { setLabelSelectable(this.handle, v) }
    }

    /// Hover tooltip. Empty string clears it.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setLabelTooltip(this.handle, v) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeLabel(
            window.handle,
            this._text, this._x, this._y, this._width, this._height,
            this._isVisible, this._selectable
        )
        setOnLabelMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnLabelMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnLabelMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnLabelMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnLabelContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnLabelMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        if this._tooltip != "" { setLabelTooltip(this.handle, this._tooltip) }
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
