/// Cross-platform GUI library — minimal slice (window + lifecycle).
///
/// ilang's `[deps] @target` filter swaps `gui_impl` between
/// `gui-cocoa` (macOS) and `gui-win32` (Windows). User code only
/// imports `gui`; the OS-specific package never appears at the
/// call site.
///
///   use gui
///   let w = new gui.Window("hello", 480.0, 320.0)
///   w.onClose.add(fn(e: gui.CloseEvent) { console.log("bye") })
///   gui.run()

use events { Signal }
use gui_impl {
    createNativeWindow,
    setOnClose,
    setOnResize,
    setOnKeyDown,
    setOnKeyUp,
    setOnMouseMove,
    setOnMouseDown,
    setOnMouseUp,
    createNativeButton,
    setOnButtonClick,
    setButtonText,
    setButtonFrame,
    setButtonEnabled,
    setButtonVisible,
    runEventLoop,
}

/// Marker base class for every object that can be the `sender`
/// of a GUI `Event`. Empty by design — concrete event-emitters
/// (today: `Window`) inherit it so their instances flow into
/// `Event.sender` without per-event `as` casts on the library
/// side. Users typically downcast (`e.sender as Window`) at the
/// call site when they need the concrete type.
pub class EventTarget {
    pub init() {}
}

/// Base class for every GUI event. The `sender` field is set by
/// the library when the event fires and points at the
/// `EventTarget` that produced it (today always the originating
/// `Window`).
pub class Event {
    pub sender: EventTarget
    pub init(sender: EventTarget) {
        this.sender = sender
    }
}

/// Fired when the user closes the window (clicks the close box,
/// uses ⌘W / Alt+F4, etc.). Carries no extra payload beyond the
/// inherited `sender`.
pub class CloseEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}

/// Fired when the window's drawable content area changes size.
/// `width` / `height` are the new client-area dimensions in
/// device-independent points.
pub class SizeEvent : Event {
    pub width: f64
    pub height: f64
    pub init(sender: EventTarget, width: f64, height: f64) {
        super(sender)
        this.width = width
        this.height = height
    }
}

/// Keyboard event. `code` is the W3C UI Events
/// `KeyboardEvent.code` string (e.g. `"KeyA"`, `"Enter"`,
/// `"ArrowLeft"`, `"ShiftLeft"`) — platform-independent and
/// independent of the active layout. The four `*Key` flags
/// mirror the DOM `KeyboardEvent.ctrlKey` / `shiftKey` / `altKey`
/// / `metaKey` booleans.
pub class KeyEvent : Event {
    pub code: string
    pub ctrlKey: bool
    pub shiftKey: bool
    pub altKey: bool
    pub metaKey: bool
    pub init(
        sender: EventTarget, code: string,
        ctrlKey: bool, shiftKey: bool, altKey: bool, metaKey: bool
    ) {
        super(sender)
        this.code = code
        this.ctrlKey = ctrlKey
        this.shiftKey = shiftKey
        this.altKey = altKey
        this.metaKey = metaKey
    }
}

/// Mouse event. `x` / `y` are in the window's client area, with
/// the origin at the top-left (W3C convention). `button` matches
/// `MouseEvent.button`: `0 = left`, `1 = middle`, `2 = right`,
/// `3 = back`, `4 = forward`. Modifier flags as in `KeyEvent`.
pub class MouseEvent : Event {
    pub x: f64
    pub y: f64
    pub button: i32
    pub ctrlKey: bool
    pub shiftKey: bool
    pub altKey: bool
    pub metaKey: bool
    pub init(
        sender: EventTarget, x: f64, y: f64, button: i32,
        ctrlKey: bool, shiftKey: bool, altKey: bool, metaKey: bool
    ) {
        super(sender)
        this.x = x
        this.y = y
        this.button = button
        this.ctrlKey = ctrlKey
        this.shiftKey = shiftKey
        this.altKey = altKey
        this.metaKey = metaKey
    }
}

/// Forward-referenced by `Button.attachTo`. The actual class
/// declaration follows below; ilang requires the parent to be
/// declared before a subclass, so `Button : EventTarget` /
/// `Window : EventTarget` can interleave so long as the parent
/// (`EventTarget`) is already in scope by then.
///
/// A top-level OS window. Construct with `new Window(title, w, h)`;
/// the window appears immediately but no events fire until
/// `gui.run()` is called.
///
/// Each `on*` field is a `Signal<T>` users register on with
/// `w.onClose.add(fn(e) { ... })`. Listeners fire on the main
/// thread synchronously from inside `gui.run()`.
pub class Window : EventTarget {
    pub handle: i64
    pub onClose:     Signal<CloseEvent>
    pub onResize:    Signal<SizeEvent>
    pub onKeyDown:   Signal<KeyEvent>
    pub onKeyUp:     Signal<KeyEvent>
    pub onMouseMove: Signal<MouseEvent>
    pub onMouseDown: Signal<MouseEvent>
    pub onMouseUp:   Signal<MouseEvent>

    pub init(title: string, width: f64, height: f64) {
        super()
        this.handle      = createNativeWindow(title, width, height)
        this.onClose     = new Signal<CloseEvent>()
        this.onResize    = new Signal<SizeEvent>()
        this.onKeyDown   = new Signal<KeyEvent>()
        this.onKeyUp     = new Signal<KeyEvent>()
        this.onMouseMove = new Signal<MouseEvent>()
        this.onMouseDown = new Signal<MouseEvent>()
        this.onMouseUp   = new Signal<MouseEvent>()
        // Forward native callbacks to each Signal. The closures
        // capture `this` so the same Window flows through as
        // `event.sender`.
        setOnClose(this.handle, fn() {
            this.onClose.emit(new CloseEvent(this))
        })
        setOnResize(this.handle, fn(w: f64, h: f64) {
            this.onResize.emit(new SizeEvent(this, w, h))
        })
        setOnKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnMouseMove(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseMove.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
    }

    /// Attach `child` to this window. The native OS control is
    /// realized on this call; property setters invoked before
    /// `addChild` apply on realization, those invoked after
    /// propagate immediately.
    pub addChild(child: Button) {
        child.attachTo(this)
    }
}

/// A push-button widget. Construct unparented, configure with
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
pub class Button : EventTarget {
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

    /// Realize the native control under `window`. Called by
    /// `Window.addChild(this)`; not part of the public surface
    /// the user invokes directly.
    pub attachTo(window: Window) {
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

/// Run the platform event loop until every window has closed
/// (macOS: `NSApp.run()` until `terminate:`; Windows:
/// `GetMessage` loop until `WM_QUIT`). Blocks the calling thread.
pub fn run() {
    runEventLoop()
}
