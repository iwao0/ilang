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

use std.events { Signal }
use events {
    EventTarget,
    CloseEvent, SizeEvent, KeyEvent, MouseEvent,
}
use layout { Dock, Margin }
use gui_impl {
    createNativeWindow,
    setWindowTitle, setWindowSize,
    showNativeWindow, hideNativeWindow,
    setOnClose,
    setOnResize,
    setOnKeyDown,
    setOnKeyUp,
    setOnMouseMove,
    setOnMouseDown,
    setOnMouseUp,
    setWindowMenuBar,
}
use menubar { Menu }

/// A top-level OS window. Construct with `new Window()`, then
/// configure (`title` / `width` / `height` / `addChild`) before
/// calling `show()` to display it.
///
/// The native window is created HIDDEN in `init` so callers can
/// stage their initial layout without flashing an empty grey
/// rectangle. The first `show()` brings it on screen; later
/// `show()` / `hide()` toggle visibility.
///
/// Each `on*` field is a `Signal<T>` users register on with
/// `w.onClose.add(fn(e) { ... })`. Listeners fire on the main
/// thread synchronously from inside `gui.run()`.
pub class Window : EventTarget {
    pub handle: i64
    _title:  string
    _width:  f64
    _height: f64
    /// Direct children, in `addChild` order. Used by the dock /
    /// margin layout pass on resize. Each entry is a `Control`
    /// (subclass) so the layout pass can call its virtual
    /// `setFrame` to write geometry through one native call.
    _children: Control[]
    pub onClose:     Signal<CloseEvent>
    pub onResize:    Signal<SizeEvent>
    pub onKeyDown:   Signal<KeyEvent>
    pub onKeyUp:     Signal<KeyEvent>
    pub onMouseMove: Signal<MouseEvent>
    pub onMouseDown: Signal<MouseEvent>
    pub onMouseUp:   Signal<MouseEvent>

    pub init() {
        super()
        this._title  = ""
        this._width  = 480.0
        this._height = 320.0
        this._children = []
        this.handle  = createNativeWindow(this._title, this._width, this._height)
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
            this._width  = w
            this._height = h
            // Re-flow docked children before user listeners observe
            // the new size, so any onResize handler that inspects
            // child geometry sees the post-layout values.
            this.relayout()
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

    pub get title(): string { this._title }
    pub set title(v: string) {
        this._title = v
        setWindowTitle(this.handle, v)
    }

    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        setWindowSize(this.handle, this._width, this._height)
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        setWindowSize(this.handle, this._width, this._height)
    }

    /// Display the window. First call brings the hidden native
    /// window on screen and activates the app; subsequent calls
    /// re-show after a `hide()`.
    pub show() { showNativeWindow(this.handle) }

    /// Hide the window without destroying it. The HWND / NSWindow
    /// stays alive so a later `show()` brings the same instance
    /// (with its current children + state) back.
    pub hide() { hideNativeWindow(this.handle) }

    /// Attach a child control. The native OS control is realized
    /// on this call; property setters invoked before `addChild`
    /// apply on realization, those invoked after propagate
    /// immediately. Dispatches polymorphically through
    /// `Control.attachTo`.
    ///
    /// The child is recorded for later dock / margin layout
    /// passes. After realisation a single `relayout()` runs so a
    /// child added with `dock = fill` (or any non-`none` dock)
    /// snaps into the right rectangle on its first paint.
    pub addChild(child: Control) {
        child.attachTo(this)
        this._children.push(child)
        this.relayout()
    }

    /// Re-run the dock / margin layout pass over every direct
    /// child. Called automatically on `addChild` and on native
    /// window resize; user code can also call it after toggling a
    /// child's `dock` or `margin` to re-flow without resizing the
    /// window.
    pub relayout() {
        let availX: f64 = 0.0
        let availY: f64 = 0.0
        let availW: f64 = this._width
        let availH: f64 = this._height
        let n = this._children.length
        let i = 0
        while i < n {
            let child = this._children[i]
            let mt = child.margin.top
            let mr = child.margin.right
            let mb = child.margin.bottom
            let ml = child.margin.left
            match child.dock {
                none {
                    // Absolute positioning — leave the child's own
                    // x / y / width / height alone.
                }
                top {
                    let h = child.frameHeight()
                    child.setFrame(availX + ml, availY + mt, availW - ml - mr, h)
                    let consumed = h + mt + mb
                    availY = availY + consumed
                    availH = availH - consumed
                }
                bottom {
                    let h = child.frameHeight()
                    child.setFrame(availX + ml, availY + availH - h - mb, availW - ml - mr, h)
                    availH = availH - (h + mt + mb)
                }
                left {
                    let w = child.frameWidth()
                    child.setFrame(availX + ml, availY + mt, w, availH - mt - mb)
                    let consumed = w + ml + mr
                    availX = availX + consumed
                    availW = availW - consumed
                }
                right {
                    let w = child.frameWidth()
                    child.setFrame(availX + availW - w - mr, availY + mt, w, availH - mt - mb)
                    availW = availW - (w + ml + mr)
                }
                fill {
                    child.setFrame(availX + ml, availY + mt, availW - ml - mr, availH - mt - mb)
                }
            }
            i = i + 1
        }
    }

    /// Install `menus` as the window's menu bar. On macOS this
    /// drives the system-wide screen menu bar (the `windowHandle`
    /// argument is accepted for API symmetry and ignored). On
    /// Windows each window owns its own menu bar.
    pub setMenuBar(menus: Menu[]) {
        let handles: i64[] = []
        let n = menus.length
        let i = 0
        while i < n {
            handles.push(menus[i].handle)
            i = i + 1
        }
        setWindowMenuBar(this.handle, handles)
    }
}

/// Base class for every gui widget that lives inside a `Window`.
/// Subclasses override `attachTo` to allocate / parent the native
/// peer; the default is a no-op so a forgotten override
/// fails-soft (the control simply never appears).
///
/// Layout support: `dock` + `margin` are honoured by the parent
/// `Window`'s relayout pass. Default `dock = Dock.none` keeps the
/// child at the absolute coordinates its own `x` / `y` setters
/// wrote — same behaviour as before the layout layer existed.
///
/// `setFrame` is the single batched geometry setter the layout
/// pass invokes (vs. four per-property assignments that would
/// each trigger a native `set*Frame` call and flicker). Each
/// concrete `Control` subclass overrides it to update its cached
/// `_x` / `_y` / `_width` / `_height` and forward to its native
/// `set*Frame`. `frameWidth` / `frameHeight` are read-back hooks
/// so the layout pass can size top / bottom / left / right docks
/// using the child's own preferred extent.
pub class Control : EventTarget {
    pub dock:   Dock
    pub margin: Margin
    pub init() {
        super()
        this.dock   = Dock.none
        this.margin = new Margin()
    }
    pub attachTo(window: Window) {}
    /// Default no-op. Concrete subclasses override.
    pub setFrame(x: f64, y: f64, w: f64, h: f64) {}
    /// Subclass-supplied current width for layout. Default 0.0.
    pub frameWidth():  f64 { 0.0 }
    pub frameHeight(): f64 { 0.0 }
}
