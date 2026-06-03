/// Image display widget — PNG / JPEG file path → on-screen pixels.
///
///   let img = new gui.ImageView()
///   img.src    = "logo.png"
///   img.x      = 20.0
///   img.y      = 20.0
///   img.width  = 200.0
///   img.height = 100.0
///   window.addChild(img)
///
/// The image is stretched to fill `width × height` (the default
/// scale mode every platform takes when no aspect-ratio API is
/// configured). Setting `src` to `""` clears the displayed image.

use std.events { Signal }
use events { MouseEvent }
use window { Window, Control }
use gui_impl {
    createNativeImageView,
    setImageViewSrc, setImageViewFrame,
    setImageViewEnabled, setImageViewVisible, setImageViewTooltip,
    setOnImageViewMouseEnter, setOnImageViewMouseLeave,
    setOnImageViewMouseDown, setOnImageViewMouseUp,
    setOnImageViewContextMenu, setOnImageViewMouseWheel,
}

pub class ImageView : Control {
    handle: i64
    _src:        string
    _x:          f64
    _y:          f64
    _width:      f64
    _height:     f64
    _isEnabled:  bool
    _isVisible:  bool
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
        this._src        = ""
        this._x          = 0.0
        this._y          = 0.0
        this._width      = 100.0
        this._height     = 100.0
        this._isEnabled  = true
        this._isVisible  = true
        this._tooltip    = ""
        this.onMouseEnter  = new Signal<MouseEvent>()
        this.onMouseLeave  = new Signal<MouseEvent>()
        this.onMouseDown   = new Signal<MouseEvent>()
        this.onMouseUp     = new Signal<MouseEvent>()
        this.onContextMenu = new Signal<MouseEvent>()
        this.onMouseWheel  = new Signal<MouseEvent>()
    }

    /// Filesystem path to a PNG / JPEG image. Empty string clears
    /// the displayed image. Each setter call re-decodes the file —
    /// don't hammer this in a hot loop.
    pub get src(): string { this._src }
    pub set src(v: string) {
        this._src = v
        if this.handle >= 0 { setImageViewSrc(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setImageViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setImageViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setImageViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setImageViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setImageViewEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setImageViewVisible(this.handle, v) }
    }

    /// Hover tooltip. Empty string clears it.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setImageViewTooltip(this.handle, v) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeImageView(
            window.handle,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        if this._src != "" { setImageViewSrc(this.handle, this._src) }
        setOnImageViewMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnImageViewMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnImageViewMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnImageViewMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnImageViewContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnImageViewMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        if this._tooltip != "" { setImageViewTooltip(this.handle, this._tooltip) }
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setImageViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
