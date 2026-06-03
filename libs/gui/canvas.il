/// 2D drawing surface — HTML Canvas-like widget. Owns a
/// persistent pixel buffer that the OS repaint just blits, so
/// resize / occlusion-recovery doesn't run any user code.
///
///   let c = new gui.Canvas()
///   c.x = 0.0
///   c.y = 0.0
///   c.width  = 400.0
///   c.height = 300.0
///   window.addChild(c)
///   c.fillColor = 0xFFFF0000   // opaque red (0xAARRGGBB)
///   c.fillRect(10.0, 10.0, 100.0, 50.0)
///   c.beginPath()
///   c.moveTo(0.0, 0.0)
///   c.lineTo(200.0, 200.0)
///   c.stroke()
///
/// Drawing operations issued before `window.addChild(c)` are
/// silently ignored — the underlying bitmap doesn't exist yet.
/// Property setters (`fillColor` / `strokeColor` / `lineWidth`)
/// cache pre-realize and apply on `attachTo`.

use std.events { Signal }
use events { KeyEvent, MouseEvent, FocusEvent, BlurEvent, FilesDroppedEvent }
use window { Window, Control }
use gui_impl {
    createNativeCanvas, setCanvasFrame,
    setCanvasEnabled, setCanvasVisible, setCanvasTooltip,
    setCanvasFillColor, setCanvasStrokeColor, setCanvasLineWidth,
    canvasClear,
    canvasFillRect, canvasStrokeRect,
    canvasBeginPath, canvasMoveTo, canvasLineTo,
    canvasArc, canvasClosePath, canvasFill, canvasStroke,
    setOnCanvasKeyDown, setOnCanvasKeyUp,
    setOnCanvasFocus, setOnCanvasBlur,
    setOnCanvasMouseEnter, setOnCanvasMouseLeave,
    setOnCanvasMouseMove, setOnCanvasMouseDown, setOnCanvasMouseUp,
    setOnCanvasContextMenu, setOnCanvasMouseWheel,
    setOnCanvasFilesDropped,
    focusCanvas, blurCanvas,
}

pub class Canvas : Control {
    handle: i64
    _x:           f64
    _y:           f64
    _width:       f64
    _height:      f64
    _isEnabled:   bool
    _isVisible:   bool
    _tooltip:     string
    _fillColor:   u32
    _strokeColor: u32
    _lineWidth:   f64

    /// View-local input events. Coordinates are pixels from the
    /// canvas's top-left corner. Keyboard events fire only while
    /// the canvas owns focus — clicking inside it grabs focus
    /// (matches the GpuView convention).
    pub onKeyDown:    Signal<KeyEvent>
    pub onKeyUp:      Signal<KeyEvent>
    pub onFocus:      Signal<FocusEvent>
    pub onBlur:       Signal<BlurEvent>
    pub onMouseEnter: Signal<MouseEvent>
    pub onMouseLeave: Signal<MouseEvent>
    pub onMouseMove:  Signal<MouseEvent>
    pub onMouseDown:  Signal<MouseEvent>
    pub onMouseUp:    Signal<MouseEvent>
    pub onContextMenu: Signal<MouseEvent>
    pub onMouseWheel: Signal<MouseEvent>
    pub onFilesDropped: Signal<FilesDroppedEvent>

    pub init() {
        super()
        this.handle       = -1
        this._x           = 0.0
        this._y           = 0.0
        this._width       = 200.0
        this._height      = 200.0
        this._isEnabled   = true
        this._isVisible   = true
        this._tooltip     = ""
        this._fillColor   = 0xFF000000 as u32
        this._strokeColor = 0xFF000000 as u32
        this._lineWidth   = 1.0
        this.onKeyDown    = new Signal<KeyEvent>()
        this.onKeyUp      = new Signal<KeyEvent>()
        this.onFocus      = new Signal<FocusEvent>()
        this.onBlur       = new Signal<BlurEvent>()
        this.onMouseEnter = new Signal<MouseEvent>()
        this.onMouseLeave = new Signal<MouseEvent>()
        this.onMouseMove  = new Signal<MouseEvent>()
        this.onMouseDown  = new Signal<MouseEvent>()
        this.onMouseUp    = new Signal<MouseEvent>()
        this.onContextMenu = new Signal<MouseEvent>()
        this.onMouseWheel = new Signal<MouseEvent>()
        this.onFilesDropped = new Signal<FilesDroppedEvent>()
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setCanvasFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setCanvasFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setCanvasFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setCanvasFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setCanvasEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setCanvasVisible(this.handle, v) }
    }

    /// Hover tooltip. Empty string clears it.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setCanvasTooltip(this.handle, v) }
    }

    pub get fillColor(): u32 { this._fillColor }
    pub set fillColor(v: u32) {
        this._fillColor = v
        if this.handle >= 0 { setCanvasFillColor(this.handle, v) }
    }
    pub get strokeColor(): u32 { this._strokeColor }
    pub set strokeColor(v: u32) {
        this._strokeColor = v
        if this.handle >= 0 { setCanvasStrokeColor(this.handle, v) }
    }
    pub get lineWidth(): f64 { this._lineWidth }
    pub set lineWidth(v: f64) {
        this._lineWidth = v
        if this.handle >= 0 { setCanvasLineWidth(this.handle, v) }
    }

    /// Overwrite the entire bitmap with `color` (opaque or alpha-
    /// blended depending on the colour's A byte). Equivalent to
    /// HTML `ctx.fillStyle = …; ctx.fillRect(0, 0, w, h)`.
    pub clear(color: u32) {
        if this.handle >= 0 { canvasClear(this.handle, color) }
    }
    pub fillRect(x: f64, y: f64, w: f64, h: f64) {
        if this.handle >= 0 { canvasFillRect(this.handle, x, y, w, h) }
    }
    pub strokeRect(x: f64, y: f64, w: f64, h: f64) {
        if this.handle >= 0 { canvasStrokeRect(this.handle, x, y, w, h) }
    }

    pub beginPath() {
        if this.handle >= 0 { canvasBeginPath(this.handle) }
    }
    pub moveTo(x: f64, y: f64) {
        if this.handle >= 0 { canvasMoveTo(this.handle, x, y) }
    }
    pub lineTo(x: f64, y: f64) {
        if this.handle >= 0 { canvasLineTo(this.handle, x, y) }
    }
    /// Append a circular arc to the current path. Angles are in
    /// radians, swept clockwise from 3 o'clock (HTML Canvas
    /// default direction in screen coordinates).
    pub arc(cx: f64, cy: f64, r: f64, startAngle: f64, endAngle: f64) {
        if this.handle >= 0 {
            canvasArc(this.handle, cx, cy, r, startAngle, endAngle)
        }
    }
    pub closePath() {
        if this.handle >= 0 { canvasClosePath(this.handle) }
    }
    pub fill() {
        if this.handle >= 0 { canvasFill(this.handle) }
    }
    pub stroke() {
        if this.handle >= 0 { canvasStroke(this.handle) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeCanvas(
            window.handle,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        if this.handle >= 0 {
            setCanvasFillColor(this.handle, this._fillColor)
            setCanvasStrokeColor(this.handle, this._strokeColor)
            setCanvasLineWidth(this.handle, this._lineWidth)
            setOnCanvasKeyDown(this.handle,
                fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
                })
            setOnCanvasKeyUp(this.handle,
                fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
                })
            setOnCanvasFocus(this.handle, fn() {
                this.onFocus.emit(new FocusEvent(this))
            })
            setOnCanvasBlur(this.handle, fn() {
                this.onBlur.emit(new BlurEvent(this))
            })
            setOnCanvasMouseEnter(this.handle, fn(x: f64, y: f64) {
                this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
            })
            setOnCanvasMouseLeave(this.handle, fn(x: f64, y: f64) {
                this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
            })
            setOnCanvasMouseMove(this.handle,
                fn(x: f64, y: f64, b: u32, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this.onMouseMove.emit(new MouseEvent(this, x, y, 0, ck, sk, ak, mk))
                })
            setOnCanvasMouseDown(this.handle,
                fn(x: f64, y: f64, btn: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this.onMouseDown.emit(new MouseEvent(this, x, y, btn, ck, sk, ak, mk))
                })
            setOnCanvasMouseUp(this.handle,
                fn(x: f64, y: f64, btn: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this.onMouseUp.emit(new MouseEvent(this, x, y, btn, ck, sk, ak, mk))
                })
            setOnCanvasContextMenu(this.handle,
                fn(x: f64, y: f64, btn: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this.onContextMenu.emit(new MouseEvent(this, x, y, btn, ck, sk, ak, mk))
                })
            setOnCanvasMouseWheel(this.handle,
                fn(dx: f64, dy: f64, x: f64, y: f64,
                   ck: bool, sk: bool, ak: bool, mk: bool) {
                    let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                    e.wheelDx = dx
                    e.wheelDy = dy
                    this.onMouseWheel.emit(e)
                })
            setOnCanvasFilesDropped(this.handle,
                fn(files: string[], x: f64, y: f64) {
                    this.onFilesDropped.emit(new FilesDroppedEvent(this, files, x, y))
                })
            if this._tooltip != "" { setCanvasTooltip(this.handle, this._tooltip) }
        }
    }

    /// Move keyboard focus to this canvas. No-op until attached.
    pub focus() { if this.handle >= 0 { focusCanvas(this.handle) } }
    /// Release focus iff this canvas holds it. No-op until attached.
    pub blur()  { if this.handle >= 0 { blurCanvas(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setCanvasFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
