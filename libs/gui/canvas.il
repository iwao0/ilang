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

use window { Window, Control }
use gui_impl {
    createNativeCanvas, setCanvasFrame, setCanvasVisible,
    setCanvasFillColor, setCanvasStrokeColor, setCanvasLineWidth,
    canvasClear,
    canvasFillRect, canvasStrokeRect,
    canvasBeginPath, canvasMoveTo, canvasLineTo,
    canvasArc, canvasClosePath, canvasFill, canvasStroke,
}

pub class Canvas : Control {
    handle: i64
    _x:           f64
    _y:           f64
    _width:       f64
    _height:      f64
    _isVisible:   bool
    _fillColor:   u32
    _strokeColor: u32
    _lineWidth:   f64

    pub init() {
        super()
        this.handle       = -1
        this._x           = 0.0
        this._y           = 0.0
        this._width       = 200.0
        this._height      = 200.0
        this._isVisible   = true
        this._fillColor   = 0xFF000000 as u32
        this._strokeColor = 0xFF000000 as u32
        this._lineWidth   = 1.0
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

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setCanvasVisible(this.handle, v) }
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
            this._isVisible
        )
        if this.handle >= 0 {
            setCanvasFillColor(this.handle, this._fillColor)
            setCanvasStrokeColor(this.handle, this._strokeColor)
            setCanvasLineWidth(this.handle, this._lineWidth)
        }
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setCanvasFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
