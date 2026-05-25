/// Push-button widget. Construct unparented, configure with
/// property setters, then attach with `window.addChild(button)`.
/// Properties set before `addChild` are cached on the Button and
/// applied to the native control once it's realized; properties
/// set after `addChild` propagate immediately.
///
///   let b = new gui.Button()
///   b.text = "OK"
///   b.x = 100.0
///   b.y = 50.0
///   b.width  = 80.0
///   b.height = 30.0
///   b.onClick.add(fn(e: gui.MouseEvent) { console.log("clicked") })
///   window.addChild(b)

use std.events { Signal }
use events { MouseEvent }
use window { Window, Control }
use gui_impl {
    createNativeButton,
    setOnButtonClick,
    setButtonText, setButtonFrame, setButtonEnabled, setButtonVisible,
}

pub class Button : Control {
    // Cached state. `handle` is -1 until `addChild` realizes the
    // native control; before then, setters only update the cache.
    handle: i64
    _text:      string
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    pub onClick: Signal<MouseEvent>

    pub init() {
        super()
        this.handle      = -1
        this._text       = ""
        this._x          = 0.0
        this._y          = 0.0
        this._width      = 80.0
        this._height     = 30.0
        this._isEnabled  = true
        this._isVisible  = true
        this.onClick     = new Signal<MouseEvent>()
    }

    pub get text(): string { this._text }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setButtonText(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setButtonEnabled(this.handle, v) }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setButtonVisible(this.handle, v) }
    }

    /// Realize the native control under `window`. Called via
    /// `Window.addChild(this)` → `Control.attachTo`; not part of
    /// the public surface the user invokes directly.
    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeButton(
            window.handle,
            this._text, this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        // Forward native click → onClick. Native pos is (centre, 0)
        // on macOS and (0, 0) on win32; we surface it as a MouseEvent
        // with `sender = this` for downcast.
        setOnButtonClick(this.handle, fn(x: f64, y: f64, b: i32) {
            this.onClick.emit(new MouseEvent(this, x, y, b, false, false, false, false))
        })
    }
}
