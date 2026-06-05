/// Inline colour swatch the user can click to pick a colour.
/// Cocoa: `NSColorWell` (opens the floating system colour picker);
/// Linux: `GtkColorDialogButton`; Win32: custom-drawn rectangle
/// that opens `ChooseColor` on click.
///
/// `color` is `u32` in `0xAARRGGBB` — the same encoding
/// `Window.colorPicker` already uses for the modal picker
/// dialog. `onChange` fires once when the user commits a new
/// colour (panel closed / dialog OK'd).
///
///     let cw = new gui.ColorWell()
///     cw.color = 0xFFE74C3C   // initial red
///     cw.x = 20.0; cw.y = 20.0
///     cw.width = 40.0; cw.height = 24.0
///     cw.onChange.add(fn(e: gui.ColorEvent) {
///         console.log(format("color = 0x{:08x}", e.color))
///     })
///     window.addChild(cw)

use std.events { Signal }
use events { ColorEvent }
use window { Window, Control, Container }
use gui_impl {
    createNativeColorWell,
    readColorWellColor, setColorWellColor,
    setColorWellFrame, setColorWellEnabled, setColorWellVisible,
    setColorWellTooltip,
    setOnColorWellChange,
}

pub class ColorWell : Control {
    handle: i64
    _color:     u32
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string
    pub onChange: Signal<ColorEvent>

    pub init() {
        super()
        this.handle     = -1
        this._color     = 0xFF000000 as u32   // opaque black
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 44.0
        this._height    = 24.0
        this._isEnabled = true
        this._isVisible = true
        this._tooltip   = ""
        this.onChange   = new Signal<ColorEvent>()
    }

    pub get color(): u32 {
        if this.handle >= 0 { this._color = readColorWellColor(this.handle) }
        this._color
    }
    pub set color(v: u32) {
        this._color = v
        if this.handle >= 0 { setColorWellColor(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setColorWellFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setColorWellFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setColorWellFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setColorWellFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setColorWellEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setColorWellVisible(this.handle, v) }
    }

    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setColorWellTooltip(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeColorWell(
            parent.nativeChildHandle(),
            this._color,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnColorWellChange(this.handle, fn(c: u32) {
            this._color = c
            this.onChange.emit(new ColorEvent(this, c))
        })
        if this._tooltip != "" { setColorWellTooltip(this.handle, this._tooltip) }
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setColorWellFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
