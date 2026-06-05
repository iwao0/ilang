/// Top-level OS window + `Control` / `Container` base classes.
///
/// `Control` is the leaf-widget base every concrete widget
/// (`Button` / `Label` / `TextField` / …) inherits from.
///
/// `Container : Control` adds child management (`_children`,
/// `addChild`) and a virtual `relayout()` for layout containers
/// (`VBox` / `HBox` / `Grid` — see `box.il`). `Window` extends
/// `Container` so the root window is itself a container and the
/// same `_children` / dock-loop machinery applies to it.
///
/// `Control` and `Window` share a file because they reference
/// each other (`Container.addChild(c: Control)` and
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
    MinimizeEvent, RestoreEvent,
    MaximizeEvent, UnmaximizeEvent,
    EnterFullScreenEvent, LeaveFullScreenEvent,
    FocusEvent, BlurEvent,
    ShowEvent, HideEvent, MoveEvent,
    FilesDroppedEvent,
}
use layout { Dock, Margin, Point, Size, Rect }
use gui_impl {
    createNativeWindow,
    setWindowTitle, setWindowSize,
    showNativeWindow, hideNativeWindow,
    minimizeNativeWindow, restoreNativeWindow,
    maximizeNativeWindow, unmaximizeNativeWindow,
    setNativeWindowFullScreen,
    isNativeWindowMinimized, isNativeWindowMaximized, isNativeWindowFullScreen,
    setNativeWindowResizable, isNativeWindowResizable,
    setNativeWindowMovable,   isNativeWindowMovable,
    setNativeWindowClosable,  isNativeWindowClosable,
    focusNativeWindow, blurNativeWindow, isNativeWindowFocused,
    setNativeWindowAlwaysOnTop, isNativeWindowAlwaysOnTop,
    moveTopNativeWindow,
    setNativeWindowOpacity, getNativeWindowOpacity,
    centerNativeWindow,
    setNativeWindowPosition, getNativeWindowX, getNativeWindowY,
    setNativeWindowBounds,
    setNativeWindowMinSize,
    getNativeWindowMinWidth, getNativeWindowMinHeight,
    setNativeWindowMaxSize,
    getNativeWindowMaxWidth, getNativeWindowMaxHeight,
    flashNativeWindow,
    setNativeWindowProgressBar,
    setNativeWindowParent,
    setNativeWindowModal, isNativeWindowModal,
    runNativeWindowModal,
    setOnClose,
    setOnResize,
    setOnKeyDown,
    setOnKeyUp,
    setOnMouseMove,
    setOnMouseDown,
    setOnMouseUp,
    setOnWindowMouseEnter, setOnWindowMouseLeave,
    setOnWindowMouseWheel, setOnWindowContextMenu,
    setOnWindowFilesDropped,
    setOnWindowMinimize, setOnWindowRestore,
    setOnWindowMaximize, setOnWindowUnmaximize,
    setOnWindowEnterFullScreen, setOnWindowLeaveFullScreen,
    setOnWindowFocus, setOnWindowBlur,
    setOnWindowMove,
    setWindowMenuBar, menuPopup,
    showAlertDialog, showConfirmDialog,
    showOpenFileDialog, showSaveFileDialog,
    showMessageDialog, showColorPickerDialog, showFontPickerDialog,
    setNativeWindowIcon,
}
use font { Font }
use menubar { Menu }
use statusbar { StatusBar }
use toolbar { Toolbar }

/// Icon glyph shown next to the message text in `Window.message`.
/// Maps to each platform's standard dialog-icon enum so the
/// result looks native.
pub enum MessageIcon: i32 {
    info     = 0
    warning  = 1
    error    = 2
    question = 3
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
/// `flex` is consumed by `VBox` / `HBox` only: `0.0` (default)
/// makes the child use its preferred `frameWidth` / `frameHeight`;
/// `> 0` makes the child stretch and absorb a share of the
/// remaining main-axis space proportional to its `flex`.
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
    pub flex:   f64
    pub init() {
        super()
        this.dock   = Dock.none
        this.margin = new Margin()
        this.flex   = 0.0
    }
    /// Realise the control's native peer under `parent`. The
    /// parent supplies the native container handle via
    /// `parent.nativeChildHandle()`; for top-level widgets
    /// (direct children of a `Window`) that's the window's own
    /// handle, for widgets nested inside a `ScrollView` it's the
    /// scroll view's documentView / inner-container handle.
    pub attachTo(parent: Container) {}
    /// Default no-op. Concrete subclasses override.
    pub setFrame(x: f64, y: f64, w: f64, h: f64) {}
    /// Subclass-supplied current x / y / width / height for
    /// layout. Defaults are 0.0; each concrete widget overrides
    /// to return its `_x` / `_y` / `_width` / `_height`. Used by
    /// `ScrollView` to compute the scrollable content rectangle
    /// from the children's frames.
    pub frameX():      f64 { 0.0 }
    pub frameY():      f64 { 0.0 }
    pub frameWidth():  f64 { 0.0 }
    pub frameHeight(): f64 { 0.0 }
}

/// Base class for widgets that themselves host child controls.
/// Owns the `_children` array, propagates `attachTo` to children
/// once a root `Window` is known, and exposes a virtual `relayout`
/// every concrete container overrides (Window: dock loop; VBox:
/// flex vertical; HBox: flex horizontal; future Grid: row/col).
///
/// `Container` does not have a native peer of its own — child
/// controls are realised as direct subviews of the root
/// `Window`'s contentView and placed in absolute window
/// coordinates by the container's relayout pass. The container's
/// allocated `_x` / `_y` / `_w` / `_h` (set by its own parent's
/// relayout via `setFrame`) is therefore an absolute rectangle
/// inside the window, and the container translates child-local
/// offsets through that rectangle when calling `child.setFrame`.
pub class Container : Control {
    /// Direct children, in `addChild` order. Each entry is a
    /// `Control` (subclass) so the layout pass can call its
    /// virtual `setFrame` to write geometry through one native
    /// call.
    pub _children: Control[]
    /// Parent container this one was attached under, populated
    /// the moment `attachTo` runs. `Window` short-circuits this
    /// in its init to `some(this)` so children added before
    /// `show()` realise immediately. Acts as the "is realised
    /// yet?" gate for `addChild`.
    pub _root: Container?
    /// Absolute window-coordinate rectangle assigned to this
    /// container by its parent's relayout pass. Used by VBox /
    /// HBox / etc. to place children with `child.setFrame(_x +
    /// child_local_x, _y + child_local_y, ..., ...)`.
    pub _x: f64
    pub _y: f64
    pub _w: f64
    pub _h: f64

    pub init() {
        super()
        this._children = []
        this._root     = none
        this._x        = 0.0
        this._y        = 0.0
        this._w        = 0.0
        this._h        = 0.0
    }

    /// When the container is itself attached to a parent, every
    /// already-buffered child is realised under THIS container
    /// (so the child's `attachTo` resolves to OUR
    /// `nativeChildHandle`) in declaration order. Subsequent
    /// `addChild` calls realise immediately (the `_root` branch
    /// in `addChild`).
    pub override attachTo(parent: Container) {
        this._root = some(parent)
        let n = this._children.length
        let i = 0
        while i < n {
            this._children[i].attachTo(this)
            i = i + 1
        }
    }

    /// Native container handle children should be parented under.
    /// `Window` returns its OS window handle; `ScrollView` returns
    /// its documentView / inner-container handle. Plain layout
    /// containers (VBox / HBox / Grid) own no native peer — their
    /// children attach to the nearest enclosing native host, so
    /// they delegate up the `_root` chain until something with a
    /// real native handle answers. `-1` is the fail-soft sentinel
    /// when no root is attached yet.
    pub nativeChildHandle(): i64 {
        if let some(root) = this._root {
            return root.nativeChildHandle()
        }
        return -1
    }

    /// Register a child for layout. If the container already
    /// knows its root, the child's native peer is realised
    /// straight away; otherwise the realisation is deferred to
    /// `attachTo` (above). A single `relayout` pass runs after so
    /// a child added with a dock or flex setting snaps into the
    /// right rectangle on its first paint.
    pub addChild(child: Control) {
        this._children.push(child)
        if let some(_) = this._root {
            child.attachTo(this)
        }
        this.relayout()
    }

    /// Parent's relayout calls this to tell us where (and how
    /// large) we get to be. Subclasses override `relayout` (not
    /// `setFrame`) — this default cascades the new rectangle into
    /// the container's children automatically.
    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x
        this._y = y
        this._w = w
        this._h = h
        this.relayout()
    }
    pub override frameWidth():  f64 { this._w }
    pub override frameHeight(): f64 { this._h }

    /// Concrete subclasses override. Default no-op so a forgotten
    /// override fails-soft (children just stay where they were).
    pub relayout() {}
}

/// A top-level OS window. Construct with `new Window()`, then
/// configure (`title` / `width` / `height` / `addChild`) before
/// calling `show()` to display it.
///
/// The native window is created HIDDEN in `init` so callers can
/// stage their initial layout without flashing an empty grey
/// rectangle. The first `show()` brings it on screen; later
/// `show()` / `hide()` toggle visibility.
///
/// `Window` inherits from `Container`, which inherits from
/// `Control`. The `Control` slots (`dock`, `margin`, `flex`,
/// `setFrame`, `frameWidth/Height`) are present but unused at
/// the root — the window has no parent to dock into. The
/// `Container` slots (`_children`, `addChild`, `_w / _h`,
/// `relayout`) ARE used: the dock layout for direct children
/// lives in `Window.relayout` below, and `_w / _h` track the
/// window's current content size (kept in sync with the OS by
/// the `setOnResize` callback installed in `init`).
///
/// Each `on*` field is a `Signal<T>` users register on with
/// `w.onClose.add(fn(e) { ... })`. Listeners fire on the main
/// thread synchronously from inside `gui.run()`.
pub class Window : Container {
    pub handle: i64
    _title:  string
    pub onClose:            Signal<CloseEvent>
    pub onResize:           Signal<SizeEvent>
    pub onKeyDown:          Signal<KeyEvent>
    pub onKeyUp:            Signal<KeyEvent>
    pub onMouseMove:        Signal<MouseEvent>
    pub onMouseDown:        Signal<MouseEvent>
    pub onMouseUp:          Signal<MouseEvent>
    pub onMouseEnter:       Signal<MouseEvent>
    pub onMouseLeave:       Signal<MouseEvent>
    pub onMouseWheel:       Signal<MouseEvent>
    pub onContextMenu:      Signal<MouseEvent>
    pub onFilesDropped:     Signal<FilesDroppedEvent>
    pub onMinimize:         Signal<MinimizeEvent>
    pub onRestore:          Signal<RestoreEvent>
    pub onMaximize:         Signal<MaximizeEvent>
    pub onUnmaximize:       Signal<UnmaximizeEvent>
    pub onEnterFullScreen:  Signal<EnterFullScreenEvent>
    pub onLeaveFullScreen:  Signal<LeaveFullScreenEvent>
    pub onFocus:            Signal<FocusEvent>
    pub onBlur:             Signal<BlurEvent>
    pub onShow:             Signal<ShowEvent>
    pub onHide:             Signal<HideEvent>
    pub onMove:             Signal<MoveEvent>
    /// Parent window kept around for `parentWindow()` lookups —
    /// the native layer only stores the handle, so the public
    /// getter rides this slot instead of going through the OS.
    _parent: Window?

    pub init() {
        super()
        this._title  = ""
        this._w      = 480.0
        this._h      = 320.0
        this.handle  = createNativeWindow(this._title, this._w, this._h)
        // The root container's `_root` is itself — so any child
        // added via `addChild` (inherited from Container) gets
        // `attachTo(this)` called immediately rather than buffered.
        this._root   = some(this)
        this.onClose            = new Signal<CloseEvent>()
        this.onResize           = new Signal<SizeEvent>()
        this.onKeyDown          = new Signal<KeyEvent>()
        this.onKeyUp            = new Signal<KeyEvent>()
        this.onMouseMove        = new Signal<MouseEvent>()
        this.onMouseDown        = new Signal<MouseEvent>()
        this.onMouseUp          = new Signal<MouseEvent>()
        this.onMouseEnter       = new Signal<MouseEvent>()
        this.onMouseLeave       = new Signal<MouseEvent>()
        this.onMouseWheel       = new Signal<MouseEvent>()
        this.onContextMenu      = new Signal<MouseEvent>()
        this.onFilesDropped     = new Signal<FilesDroppedEvent>()
        this.onMinimize         = new Signal<MinimizeEvent>()
        this.onRestore          = new Signal<RestoreEvent>()
        this.onMaximize         = new Signal<MaximizeEvent>()
        this.onUnmaximize       = new Signal<UnmaximizeEvent>()
        this.onEnterFullScreen  = new Signal<EnterFullScreenEvent>()
        this.onLeaveFullScreen  = new Signal<LeaveFullScreenEvent>()
        this.onFocus            = new Signal<FocusEvent>()
        this.onBlur             = new Signal<BlurEvent>()
        this.onShow             = new Signal<ShowEvent>()
        this.onHide             = new Signal<HideEvent>()
        this.onMove             = new Signal<MoveEvent>()
        this._parent            = none
        // Forward native callbacks to each Signal. The closures
        // capture `this` so the same Window flows through as
        // `event.sender`.
        setOnClose(this.handle, fn() {
            this.onClose.emit(new CloseEvent(this))
        })
        setOnResize(this.handle, fn(w: f64, h: f64) {
            this._w = w
            this._h = h
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
        setOnWindowMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnWindowMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnWindowMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        setOnWindowContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnWindowFilesDropped(this.handle,
            fn(files: string[], x: f64, y: f64) {
                this.onFilesDropped.emit(new FilesDroppedEvent(this, files, x, y))
            })
        setOnWindowMinimize(this.handle, fn() {
            this.onMinimize.emit(new MinimizeEvent(this))
        })
        setOnWindowRestore(this.handle, fn() {
            this.onRestore.emit(new RestoreEvent(this))
        })
        setOnWindowMaximize(this.handle, fn() {
            this.onMaximize.emit(new MaximizeEvent(this))
        })
        setOnWindowUnmaximize(this.handle, fn() {
            this.onUnmaximize.emit(new UnmaximizeEvent(this))
        })
        setOnWindowEnterFullScreen(this.handle, fn() {
            this.onEnterFullScreen.emit(new EnterFullScreenEvent(this))
        })
        setOnWindowLeaveFullScreen(this.handle, fn() {
            this.onLeaveFullScreen.emit(new LeaveFullScreenEvent(this))
        })
        setOnWindowFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnWindowBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnWindowMove(this.handle, fn(x: f64, y: f64) {
            this.onMove.emit(new MoveEvent(this, x, y))
        })
    }

    /// Window IS the native container for its direct children;
    /// the OS window handle is the parent every `createNative*`
    /// call expects.
    pub override nativeChildHandle(): i64 { this.handle }

    pub get title(): string { this._title }
    pub set title(v: string) {
        this._title = v
        setWindowTitle(this.handle, v)
    }

    pub get width(): f64 { this._w }
    pub set width(v: f64) {
        this._w = v
        setWindowSize(this.handle, this._w, this._h)
    }
    pub get height(): f64 { this._h }
    pub set height(v: f64) {
        this._h = v
        setWindowSize(this.handle, this._w, this._h)
    }

    /// Display the window. First call brings the hidden native
    /// window on screen and activates the app; subsequent calls
    /// re-show after a `hide()`. `onShow` fires synchronously
    /// after the native call returns — there's no OS hook for
    /// "the window became visible", so external means (e.g. the
    /// user clicking the Dock icon) won't trigger it.
    ///
    /// When the window is marked modal (`setModal(true)`),
    /// `show()` BLOCKS — it returns only after the user closes
    /// the modal window. macOS uses `NSApp.runModalForWindow:`,
    /// Win32 disables the owner and runs a nested message pump,
    /// GTK4 nests a GMainLoop on top of the WM-enforced input
    /// grab. Non-modal windows return immediately as before.
    pub show() {
        showNativeWindow(this.handle)
        this.onShow.emit(new ShowEvent(this))
        if isNativeWindowModal(this.handle) {
            runNativeWindowModal(this.handle)
        }
    }

    /// Hide the window without destroying it. The HWND / NSWindow
    /// stays alive so a later `show()` brings the same instance
    /// (with its current children + state) back. `onHide` fires
    /// synchronously, matching the `onShow` symmetry described
    /// above.
    pub hide() {
        hideNativeWindow(this.handle)
        this.onHide.emit(new HideEvent(this))
    }

    /// Collapse the window to the OS task-bar / Dock without
    /// destroying it. Fires `onMinimize` once the platform has
    /// committed the transition; pair with `restore()` to bring
    /// the window back to its previous frame.
    pub minimize() { minimizeNativeWindow(this.handle) }
    /// Un-minimise back to the normal frame. No-op if the window
    /// isn't currently minimised. Fires `onRestore` once the
    /// platform has committed the transition.
    pub restore()  { restoreNativeWindow(this.handle) }

    /// Maximise / "zoom" to the standard frame for the window's
    /// screen. Idempotent — already-maximised windows stay
    /// maximised (callers can disambiguate with `isMaximized()`).
    /// Fires `onMaximize` on the transition.
    pub maximize()   { maximizeNativeWindow(this.handle) }
    /// Reverse of `maximize()`. Fires `onUnmaximize` on the
    /// transition.
    pub unmaximize() { unmaximizeNativeWindow(this.handle) }

    /// Enter (`on = true`) or leave (`on = false`) native
    /// full-screen mode. On macOS the window animates onto its
    /// own Space and back. Fires `onEnterFullScreen` /
    /// `onLeaveFullScreen` on the transition; idempotent when
    /// `on` matches the current state.
    pub setFullScreen(on: bool) {
        setNativeWindowFullScreen(this.handle, on)
    }

    pub isMinimized():  bool { isNativeWindowMinimized(this.handle) }
    pub isMaximized():  bool { isNativeWindowMaximized(this.handle) }
    pub isFullScreen(): bool { isNativeWindowFullScreen(this.handle) }

    /// Whether the user can resize the window by dragging its
    /// border. Disabling drops the resize grip + maximise button
    /// from the title bar on the platforms that have those.
    pub setResizable(on: bool) { setNativeWindowResizable(this.handle, on) }
    pub isResizable(): bool    { isNativeWindowResizable(this.handle) }

    /// Whether the user can drag the window by its title bar /
    /// background. macOS honours this natively; Win32 synthesises
    /// the lock by dropping `WM_SYSCOMMAND` SC_MOVE; GTK4 has no
    /// equivalent — the flag is stored but not enforced on Linux.
    pub setMovable(on: bool) { setNativeWindowMovable(this.handle, on) }
    pub isMovable(): bool    { isNativeWindowMovable(this.handle) }

    /// Whether the title-bar × button is enabled. Disabling also
    /// blocks the OS-level close paths (⌘W / Alt+F4 / system menu
    /// → Close). `onClose` won't fire while this is `false`.
    pub setClosable(on: bool) { setNativeWindowClosable(this.handle, on) }
    pub isClosable(): bool    { isNativeWindowClosable(this.handle) }

    /// Make this window key + raise it to the foreground. macOS
    /// also bounces the app into the foreground so the transition
    /// is visible regardless of the current activation state.
    pub focus()         { focusNativeWindow(this.handle) }
    /// Best-effort blur: macOS deactivates the app; Win32 clears
    /// the active-window slot via `SetActiveWindow(NULL)`; GTK4
    /// has no per-window blur and silently no-ops.
    pub blur()          { blurNativeWindow(this.handle) }
    pub isFocused(): bool { isNativeWindowFocused(this.handle) }

    /// Pin the window above ordinary peer windows. macOS raises
    /// the NSWindow level into the floating band; Win32 uses
    /// `HWND_TOPMOST` via `SetWindowPos`; GTK4 stores the flag
    /// but the WM owns stacking on Linux (no-op visually).
    pub setAlwaysOnTop(on: bool) {
        setNativeWindowAlwaysOnTop(this.handle, on)
    }
    pub isAlwaysOnTop(): bool {
        isNativeWindowAlwaysOnTop(this.handle)
    }

    /// Raise the window to the top of its peer band WITHOUT
    /// taking keyboard focus. macOS uses `orderFront:`; Win32
    /// uses `BringWindowToTop`; GTK falls back to
    /// `gtk_window_present` (which may also focus, depending on
    /// the WM).
    pub moveTop() { moveTopNativeWindow(this.handle) }

    /// Per-window opacity in `[0.0, 1.0]`. `1.0` is fully opaque
    /// (default); `0.0` is invisible. macOS drives
    /// `NSWindow.alphaValue`; Win32 enables `WS_EX_LAYERED` on
    /// the first call and routes through
    /// `SetLayeredWindowAttributes`; GTK4 uses
    /// `gtk_widget_set_opacity`.
    pub setOpacity(v: f64)  { setNativeWindowOpacity(this.handle, v) }
    pub getOpacity(): f64   { getNativeWindowOpacity(this.handle) }

    // ─── geometry / position / bounds ─────────────────────────
    //
    // All inputs / outputs use CONTENT-area dimensions and
    // W3C-style top-left screen coordinates. The underlying
    // platforms vary widely in what they expose (especially GTK4,
    // which dropped per-window positioning) — see each native
    // function's comments for what's honoured vs. recorded-only.

    /// Centre the window on the primary screen.
    pub center() { centerNativeWindow(this.handle) }

    /// Move the window's top-left frame corner to `(x, y)` in
    /// screen coordinates. No-op on GTK4 (the WM owns placement
    /// on Wayland); the value is recorded for `getPosition` so
    /// the round-trip still works.
    pub setPosition(x: f64, y: f64) {
        setNativeWindowPosition(this.handle, x, y)
    }
    pub getPosition(): Point {
        new Point(
            getNativeWindowX(this.handle),
            getNativeWindowY(this.handle)
        )
    }

    /// Set position + content size atomically. `(x, y)` is the
    /// frame top-left in screen coords; `(w, h)` is the content
    /// area (excluding the title bar). The atomic form avoids
    /// the flicker a setPosition + width / height pair would
    /// produce on Win32.
    pub setBounds(x: f64, y: f64, w: f64, h: f64) {
        setNativeWindowBounds(this.handle, x, y, w, h)
    }
    pub getBounds(): Rect {
        new Rect(
            getNativeWindowX(this.handle),
            getNativeWindowY(this.handle),
            this._w,
            this._h
        )
    }

    /// Content-size lower bound — the OS clamps user-drag resize
    /// against this. macOS / Win32 honour it natively; GTK4
    /// translates to `gtk_widget_set_size_request`.
    pub setMinimumSize(w: f64, h: f64) {
        setNativeWindowMinSize(this.handle, w, h)
    }
    pub getMinimumSize(): Size {
        new Size(
            getNativeWindowMinWidth(this.handle),
            getNativeWindowMinHeight(this.handle)
        )
    }

    /// Content-size upper bound. macOS / Win32 honour it
    /// natively; GTK4 records the value but the WM ignores it
    /// (GTK4 has no maximum-size primitive).
    pub setMaximumSize(w: f64, h: f64) {
        setNativeWindowMaxSize(this.handle, w, h)
    }
    pub getMaximumSize(): Size {
        new Size(
            getNativeWindowMaxWidth(this.handle),
            getNativeWindowMaxHeight(this.handle)
        )
    }

    /// Resize the content area to `(w, h)`. Convenience over
    /// writing the `width` / `height` getter-setter pair when the
    /// caller wants to update both axes in one call.
    pub setContentSize(w: f64, h: f64) {
        this._w = w
        this._h = h
        setWindowSize(this.handle, w, h)
    }

    /// Request user attention. macOS bounces the Dock icon via
    /// `[NSApp requestUserAttention:NSCriticalRequest]` and keeps
    /// bouncing until the user activates the app or `flashFrame
    /// (false)` cancels the pending request. Win32 drives
    /// `FlashWindowEx` with `FLASHW_ALL | FLASHW_TIMERNOFG` so the
    /// taskbar button + title bar flash until the window is
    /// foregrounded. GTK4 is a no-op (`gtk_window_set_urgency_
    /// hint` was removed in 4.0).
    pub flashFrame(on: bool) {
        flashNativeWindow(this.handle, on)
    }

    /// Attach to another `Window` as a child / transient.
    /// `setParentWindow(none)` detaches. macOS uses
    /// `addChildWindow:ordered:`, Win32 sets `GWLP_HWNDPARENT`,
    /// GTK4 uses `gtk_window_set_transient_for`. The relationship
    /// makes the child track the parent's z-order, minimise
    /// state, and (for modal windows) sit on top of it.
    pub setParentWindow(parent: Window?) {
        this._parent = parent
        let parentHandle: i64 = match parent {
            some(p) { p.handle }
            none    { 0 as i64 }
        }
        setNativeWindowParent(this.handle, parentHandle)
    }
    pub parentWindow(): Window? { this._parent }

    /// Mark the window as modal. The flag is consulted by
    /// `show()`: a modal window enters a nested OS run-loop and
    /// `show()` returns only when the user closes it. Pair with
    /// `setParentWindow(parent)` so the OS knows which window's
    /// input to block while the modal is up.
    pub setModal(on: bool) { setNativeWindowModal(this.handle, on) }
    pub isModal(): bool    { isNativeWindowModal(this.handle) }

    /// Show task progress on the OS task-bar / Dock affordance.
    /// `progress < 0` clears, `0.0..=1.0` is a fractional bar,
    /// `progress > 1.0` is indeterminate. macOS draws an
    /// `NSProgressIndicator` over the Dock tile (covers the icon
    /// while active). Win32 routes through `ITaskbarList3.Set
    /// ProgressState` / `SetProgressValue`, which paints the
    /// taskbar button background WITHOUT covering the icon. GTK4
    /// is a no-op (libunity d-bus protocol isn't bundled).
    pub setProgressBar(progress: f64) {
        setNativeWindowProgressBar(this.handle, progress)
    }

    /// Re-run the dock / margin layout pass over every direct
    /// child. Called automatically on `addChild` (inherited from
    /// Container) and on native window resize (the `setOnResize`
    /// callback installed in `init`); user code can also call it
    /// after toggling a child's `dock` or `margin` to re-flow
    /// without resizing the window.
    pub override relayout() {
        let availX: f64 = 0.0
        let availY: f64 = 0.0
        let availW: f64 = this._w
        let availH: f64 = this._h
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

    /// Install `bar` as this window's status bar. The bar docks
    /// along the bottom edge and resizes automatically with the
    /// window; segment text is driven through the `StatusBar`
    /// object itself (`setText` / `setSegments`).
    pub setStatusBar(bar: StatusBar) {
        bar.attachTo(this.handle)
    }

    /// Install `bar` as this window's toolbar. The bar docks
    /// along the top edge and resizes automatically with the
    /// window; per-item state (button clicks, title updates)
    /// rides through the `Toolbar` object itself.
    pub setToolbar(bar: Toolbar) {
        bar.attachTo(this.handle)
    }

    /// Show `menu` as a popup at `(x, y)` in this window's client
    /// coordinates. Useful as a right-click context menu fired
    /// from a control's `onContextMenu`.
    pub popupMenu(menu: Menu, x: f64, y: f64) {
        menuPopup(menu.handle, this.handle, x, y)
    }

    /// Show a modal informational alert with a single OK button.
    /// `callback` fires once the user dismisses it.
    pub alert(text: string, callback: fn()) {
        showAlertDialog(this.handle, text, callback)
    }

    /// Show an OK / Cancel confirmation dialog. `callback` runs
    /// with `true` when the user picks OK, `false` for Cancel
    /// (including window-close shortcuts).
    pub confirm(text: string, callback: fn(bool)) {
        showConfirmDialog(this.handle, text, callback)
    }

    /// Open-file picker. `callback` runs with the chosen path,
    /// or `""` if the user cancelled. Single file only.
    pub openFile(callback: fn(string)) {
        showOpenFileDialog(this.handle, callback)
    }

    /// Save-file picker. `suggestedName` pre-fills the name field
    /// (empty string keeps the OS default). `callback` runs with
    /// the chosen path, or `""` if cancelled.
    pub saveFile(suggestedName: string, callback: fn(string)) {
        showSaveFileDialog(this.handle, suggestedName, callback)
    }

    /// Modal message dialog with an arbitrary list of buttons.
    /// `icon` picks the standard glyph (info / warning / error /
    /// question). `callback` receives the 0-based index of the
    /// button the user picked, or `-1` if they dismissed the
    /// dialog without picking one (e.g. closed the window).
    /// Buttons render left-to-right on win32/linux and right-to-
    /// left on macOS (Cocoa convention) — order them so the
    /// "default" / affirmative button sits first.
    pub message(
        text: string, buttons: string[], icon: MessageIcon,
        callback: fn(i32)
    ) {
        showMessageDialog(this.handle, text, buttons, icon as i32, callback)
    }

    /// Modal color picker. `initial` is the seed colour
    /// (0xAARRGGBB). `callback` runs with the picked colour and
    /// an `accepted` flag — `true` when the user confirmed, `false`
    /// when they cancelled (the colour value in that case is
    /// platform-defined; treat it as undefined and fall back to
    /// `initial`).
    pub colorPicker(initial: u32, callback: fn(u32, bool)) {
        showColorPickerDialog(this.handle, initial, callback)
    }

    /// Modal font picker. `initial` seeds the dialog with the
    /// current family / size / weight / italic. `callback` runs
    /// with a fresh `Font` describing the user's choice and an
    /// `accepted` flag — the font is platform-defined on cancel.
    pub fontPicker(initial: Font, callback: fn(Font, bool)) {
        showFontPickerDialog(
            this.handle,
            initial.family, initial.size, initial.weight, initial.italic,
            fn(family: string, size: f64, weight: i32, italic: bool,
               accepted: bool) {
                callback(new Font(family, size, weight, italic), accepted)
            }
        )
    }

    /// Set the window's taskbar / title-bar icon from a file
    /// (Win32 prefers `.ico`; PNG / BMP also load via GDI+).
    /// Empty string clears the icon back to the OS default.
    /// macOS / GTK4 don't expose a direct path-based per-window
    /// icon API — those platforms surface this as a no-op.
    pub setIcon(path: string) {
        setNativeWindowIcon(this.handle, path)
    }
}
