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
    sender: EventTarget
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
    width: f64
    height: f64
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
    code: string
    ctrlKey: bool
    shiftKey: bool
    altKey: bool
    metaKey: bool
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
    x: f64
    y: f64
    button: i32
    ctrlKey: bool
    shiftKey: bool
    altKey: bool
    metaKey: bool
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

/// A top-level OS window. Construct with `new Window(title, w, h)`;
/// the window appears immediately but no events fire until
/// `gui.run()` is called.
///
/// Each `on*` field is a `Signal<T>` users register on with
/// `w.onClose.add(fn(e) { ... })`. Listeners fire on the main
/// thread synchronously from inside `gui.run()`.
pub class Window : EventTarget {
    pub handle: i64
    pub onClose: Signal<CloseEvent>

    pub init(title: string, width: f64, height: f64) {
        super()
        this.handle = createNativeWindow(title, width, height)
        this.onClose = new Signal<CloseEvent>()
        // Forward the native close notification to `onClose`.
        // The closure captures `this` so the same Window flows
        // through as `event.sender`.
        setOnClose(this.handle, fn() {
            this.onClose.emit(new CloseEvent(this))
        })
    }
}

/// Run the platform event loop until every window has closed
/// (macOS: `NSApp.run()` until `terminate:`; Windows:
/// `GetMessage` loop until `WM_QUIT`). Blocks the calling thread.
pub fn run() {
    runEventLoop()
}
