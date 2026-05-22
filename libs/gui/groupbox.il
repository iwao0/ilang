/// Titled frame for visually grouping related controls. Win32:
/// `BUTTON` class with `BS_GROUPBOX`; Cocoa: `NSBox` with a
/// title. Children are NOT reparented — lay them out manually
/// over the frame's bounds.

use window { Window, Control }
use gui_impl {
    createNativeGroupBox,
    setGroupBoxText, setGroupBoxFrame, setGroupBoxVisible,
}

pub class GroupBox : Control {
    handle: i64
    _text:      string
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isVisible: bool

    pub init() {
        super()
        this.handle     = -1
        this._text      = ""
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 200.0
        this._height    = 100.0
        this._isVisible = true
    }

    pub get text(): string { this._text }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setGroupBoxText(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setGroupBoxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setGroupBoxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setGroupBoxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setGroupBoxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setGroupBoxVisible(this.handle, v) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeGroupBox(
            window.handle,
            this._text,
            this._x, this._y, this._width, this._height,
            this._isVisible
        )
    }
}
