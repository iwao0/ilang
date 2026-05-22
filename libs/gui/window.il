/// Top-level OS window + `Control` base class.
///
/// `Control` and `Window` share a file because they reference
/// each other (`Window.addChild(child: Control)` and
/// `Control.attachTo(window: Window)`) — splitting them into
/// separate modules would form an import cycle.
///
/// Every concrete control (`Button`, `Label`, `TextField`,
/// `Checkbox`, `Slider`, `ProgressBar`, `ComboBox`,
/// `RadioButton`, `ListBox`, `GroupBox`) inherits from `Control`
/// and overrides `attachTo` to realise its native peer under
/// `window`'s `handle`.

use events { Signal }
use event {
    EventTarget,
    CloseEvent, SizeEvent, KeyEvent, MouseEvent,
}
use gui_impl {
    createNativeWindow,
    setOnClose,
    setOnResize,
    setOnKeyDown,
    setOnKeyUp,
    setOnMouseMove,
    setOnMouseDown,
    setOnMouseUp,
}

/// A top-level OS window. Construct with
/// `new Window(title, width, height)`; the window appears
/// immediately but no events fire until `gui.run()` is called.
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

    /// Attach a child control. The native OS control is realized
    /// on this call; property setters invoked before `addChild`
    /// apply on realization, those invoked after propagate
    /// immediately. Dispatches polymorphically through
    /// `Control.attachTo`.
    pub addChild(child: Control) {
        child.attachTo(this)
    }
}

/// Base class for every gui widget that lives inside a `Window`.
/// Subclasses override `attachTo` to allocate / parent the native
/// peer; the default is a no-op so a forgotten override
/// fails-soft (the control simply never appears).
pub class Control : EventTarget {
    pub init() { super() }
    pub attachTo(window: Window) { let _ = window }
}
