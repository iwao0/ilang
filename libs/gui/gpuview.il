/// GPU drawing surface — Canvas-style widget whose backing store
/// is a `WGPUSurface` (CAMetalLayer on macOS; Windows / Linux
/// still stubbed). User code builds a wgpu render pipeline once
/// against the exposed `device` / `queue` / `format`, then
/// records draw commands into a `WGPURenderPassEncoder` inside
/// the `onRender` callback.
///
/// `GpuView` is NOT re-exported from `gui` — `use gui` alone
/// stays wgpu-free so apps that don't render GPU content avoid
/// the dylib requirement below. Import it explicitly:
///
///   use gui
///   use gui.gpuview { GpuView, RenderEvent }
///   use wgpu { wgpuRenderPassEncoderSetPipeline, wgpuRenderPassEncoderDraw }
///
///   let w = new gui.Window("triangle", 640.0, 480.0)
///   let v = new GpuView()
///   v.x = 0.0; v.y = 0.0; v.width = 640.0; v.height = 480.0
///   w.addChild(v)                            // attachTo runs;
///                                            // v.device / v.format
///                                            // are now valid.
///   let pipeline = ...                       // build with v.device
///   v.onRender.add(fn(e: RenderEvent) {
///       wgpuRenderPassEncoderSetPipeline(e.pass, pipeline)
///       wgpuRenderPassEncoderDraw(e.pass, 3, 1, 0, 0)
///   })
///   v.startLoop()                            // CADisplayLink @ 60Hz
///   gui.run()
///
/// Setup (once): third_party/wgpu/fetch.sh   (downloads libwgpu_native)
/// Run (macOS):
///   DYLD_LIBRARY_PATH=third_party/wgpu/macos-aarch64/lib \
///     cargo run -p ilang -- run examples/libs/gui/gpuview/main.il
/// Run (Linux):
///   LD_LIBRARY_PATH=third_party/wgpu/linux-x86_64/lib \
///     cargo run -p ilang -- run examples/libs/gui/gpuview/main.il
/// Run (Windows): put wgpu_native.dll on PATH (or next to the entry),
///   then: cargo run -p ilang -- run examples/libs/gui/gpuview/main.il
///
/// On platforms without a `GpuView` backend (Windows / Linux),
/// `attachTo` returns a `-1` handle and every method / property
/// access is a silent no-op — same fallback Canvas takes when
/// drawing before realisation.
///
/// Input + per-frame state are wired so a `GpuView` is usable as
/// a game canvas (keyboard / mouse events local to the view +
/// snapshot getters for "is W still held" polling). Coordinates
/// in mouse events are GpuView-local pixels (origin = view's
/// top-left).

use window { Window, Control }
use events {
    EventTarget, Event, KeyEvent, MouseEvent, SizeEvent,
    FocusEvent, BlurEvent,
}
use std.events { Signal }
use wgpu29 {
    WGPUSurface, WGPUDevice, WGPUQueue, WGPURenderPassEncoder,
    Device, Surface, Queue, TextureFormat,
}
// Direct import from the platform-specific `gpuview` module —
// bypasses the `gui_impl` umbrella so apps that import only
// `use gui` don't drag in wgpu's `libwgpu_native` at JIT-link
// time.
use gui_impl.gpuview {
    createNativeGpuView, setGpuViewFrame,
    setGpuViewEnabled, setGpuViewVisible, setGpuViewTooltip,
    setOnGpuViewRender,
    getGpuViewSurface, getGpuViewDevice, getGpuViewQueue, getGpuViewFormat,
    startGpuViewLoop, stopGpuViewLoop, requestGpuViewFrame,
    setOnGpuViewKeyDown, setOnGpuViewKeyUp,
    setOnGpuViewFocus, setOnGpuViewBlur,
    setOnGpuViewMouseEnter, setOnGpuViewMouseLeave,
    setOnGpuViewMouseMove, setOnGpuViewMouseDown, setOnGpuViewMouseUp,
    setOnGpuViewContextMenu, setOnGpuViewMouseWheel, setOnGpuViewResize,
    focusGpuView, blurGpuView,
}

/// One per-tick render callback. `pass` is a live
/// `WGPURenderPassEncoder` with the colour attachment already
/// cleared; user code calls `setPipeline` / `draw` / etc. on it
/// and returns. The library ends the pass, submits, and presents.
///
/// `time` is seconds since the render loop started (or since the
/// first `requestFrame` for unlooped views); `dt` is seconds
/// since the previous frame (`0.0` for the first frame, so a
/// `pos += velocity * dt` integrator starts cleanly).
pub class RenderEvent : Event {
    pub pass: WGPURenderPassEncoder
    pub time: f64
    pub dt:   f64
    pub init(sender: EventTarget, pass: WGPURenderPassEncoder, time: f64, dt: f64) {
        super(sender)
        this.pass = pass
        this.time = time
        this.dt   = dt
    }
}

pub class GpuView : Control {
    handle:       i64
    _x:           f64
    _y:           f64
    _width:       f64
    _height:      f64
    _isEnabled:   bool
    _isVisible:   bool
    _tooltip:     string

    // Friendly wrappers, built once in attachTo. Starting
    // sentinels (handle == 0) so reads before realisation are
    // well-defined.
    _surface:    Surface
    _device:     Device
    _queue:      Queue

    // ─── input snapshot ─────────────────────────────────────────
    //
    // Updated from the per-event bridge callbacks installed in
    // `attachTo`. Reads via `isKeyDown` / `mouseX` / `mouseY` /
    // `mouseButtons` are O(1) and don't allocate. Keys held when
    // the view loses focus stay in the set until a matching
    // KeyUp arrives — same as W3C / SDL semantics.
    _keysDown:     Map<string, bool>
    _mouseX:       f64
    _mouseY:       f64
    _mouseButtons: u32

    /// Fires once per display refresh while the loop is running
    /// (`startLoop` → `stopLoop`), or once per `requestFrame`
    /// call. Multiple listeners run in registration order on the
    /// same encoder.
    pub onRender: Signal<RenderEvent>

    /// View-local key/mouse signals. Coordinates are pixels from
    /// the view's top-left corner. Keyboard events only fire while
    /// the parent window has focus; on platforms where the
    /// GpuView can't grab keyboard focus directly, events bubble
    /// from the window.
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
    pub onResize:     Signal<SizeEvent>

    pub init() {
        super()
        this.handle        = -1
        this._x            = 0.0
        this._y            = 0.0
        this._width        = 400.0
        this._height       = 300.0
        this._isEnabled    = true
        this._isVisible    = true
        this._tooltip      = ""
        this._surface      = new Surface(0 as WGPUSurface)
        this._device       = new Device(0 as WGPUDevice)
        this._queue        = new Queue(0 as WGPUQueue)
        this._keysDown     = new Map<string, bool>()
        this._mouseX       = 0.0
        this._mouseY       = 0.0
        this._mouseButtons = 0 as u32
        this.onRender      = new Signal<RenderEvent>()
        this.onKeyDown     = new Signal<KeyEvent>()
        this.onKeyUp       = new Signal<KeyEvent>()
        this.onFocus       = new Signal<FocusEvent>()
        this.onBlur        = new Signal<BlurEvent>()
        this.onMouseEnter  = new Signal<MouseEvent>()
        this.onMouseLeave  = new Signal<MouseEvent>()
        this.onMouseMove   = new Signal<MouseEvent>()
        this.onMouseDown   = new Signal<MouseEvent>()
        this.onMouseUp     = new Signal<MouseEvent>()
        this.onContextMenu = new Signal<MouseEvent>()
        this.onMouseWheel  = new Signal<MouseEvent>()
        this.onResize      = new Signal<SizeEvent>()
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setGpuViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setGpuViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setGpuViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setGpuViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setGpuViewEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setGpuViewVisible(this.handle, v) }
    }

    /// Hover tooltip. Empty string clears it.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setGpuViewTooltip(this.handle, v) }
    }

    // ─── low-level escape hatches ──────────────────────────────
    //
    // Wrappers are constructed once in `attachTo` and re-used on
    // every getter call (no allocation per render-loop tick).
    // Before realisation each wrapper holds a null handle, which
    // user code can check via `v.device.handle == 0`.

    pub get surface(): Surface       { this._surface }
    pub get device():  Device        { this._device }
    pub get queue():   Queue         { this._queue }
    /// Surface texture format picked at configure time. The
    /// underlying `WGPUTextureFormat` discriminant cast into the
    /// `wgpu.TextureFormat` enum so user code can match against
    /// named variants (`v.format == TextureFormat.bgra8UnormSrgb`).
    pub get format():  TextureFormat { getGpuViewFormat(this.handle) as TextureFormat }

    // ─── per-frame state queries ───────────────────────────────
    //
    // Snapshot-style getters for game-loop polling — edge-only
    // signals can't answer "is W still held down". The snapshot
    // is updated by the per-event closures installed in
    // `attachTo`.

    /// True while `code` is currently pressed. `code` is the W3C
    /// `KeyboardEvent.code` string — compare against the
    /// `gui.KeyCode` enum's string form:
    ///
    ///     if v.isKeyDown(gui.KeyCode.keyW as string) { ... }
    pub isKeyDown(code: string): bool {
        match this._keysDown.get(code) {
            some(b) { b }
            none    { false }
        }
    }
    pub get mouseX(): f64 { this._mouseX }
    pub get mouseY(): f64 { this._mouseY }
    /// Bitfield of currently-pressed mouse buttons: bit 0 = left,
    /// bit 1 = right, bit 2 = middle. Matches the `MouseEvent`
    /// button enumeration shifted into a bit position.
    pub get mouseButtons(): u32 { this._mouseButtons }

    /// Start the CADisplayLink-driven render loop on macOS — the
    /// `onRender` signal fires once per display refresh until
    /// `stopLoop` is called.
    pub startLoop() {
        if this.handle >= 0 { startGpuViewLoop(this.handle) }
    }
    pub stopLoop() {
        if this.handle >= 0 { stopGpuViewLoop(this.handle) }
    }
    /// Render exactly one frame outside the loop. No-op while the
    /// loop is running (the upcoming tick handles it).
    pub requestFrame() {
        if this.handle >= 0 { requestGpuViewFrame(this.handle) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeGpuView(
            window.handle,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        if this.handle >= 0 {
            // Cache the friendly wrappers once — getters return
            // these without allocating.
            this._surface = new Surface(getGpuViewSurface(this.handle))
            this._device  = new Device(getGpuViewDevice(this.handle))
            this._queue   = new Queue(getGpuViewQueue(this.handle))
            // Bridge the bare-fn render callback into the
            // Signal<RenderEvent>. One subscription per native
            // view; user-side `onRender.add` fan-out happens
            // inside the signal.
            setOnGpuViewRender(this.handle, fn(pass: WGPURenderPassEncoder, t: f64, dt: f64) {
                this.onRender.emit(new RenderEvent(this, pass, t, dt))
            })
            // Input bridges. Each native callback updates the
            // snapshot first (so `isKeyDown` answers correctly
            // even if a user listener throws) and then fans the
            // event out through its Signal.
            setOnGpuViewKeyDown(this.handle,
                fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this._keysDown[c] = true
                    this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
                })
            setOnGpuViewKeyUp(this.handle,
                fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this._keysDown[c] = false
                    this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
                })
            setOnGpuViewFocus(this.handle, fn() {
                this.onFocus.emit(new FocusEvent(this))
            })
            setOnGpuViewBlur(this.handle, fn() {
                this.onBlur.emit(new BlurEvent(this))
            })
            setOnGpuViewMouseEnter(this.handle, fn(x: f64, y: f64) {
                this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
            })
            setOnGpuViewMouseLeave(this.handle, fn(x: f64, y: f64) {
                this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
            })
            setOnGpuViewMouseMove(this.handle,
                fn(x: f64, y: f64, b: u32, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this._mouseX = x
                    this._mouseY = y
                    this._mouseButtons = b
                    this.onMouseMove.emit(new MouseEvent(this, x, y, 0, ck, sk, ak, mk))
                })
            setOnGpuViewMouseDown(this.handle,
                fn(x: f64, y: f64, btn: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this._mouseX = x
                    this._mouseY = y
                    this._mouseButtons = this._mouseButtons | buttonMask(btn)
                    this.onMouseDown.emit(new MouseEvent(this, x, y, btn, ck, sk, ak, mk))
                })
            setOnGpuViewMouseUp(this.handle,
                fn(x: f64, y: f64, btn: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this._mouseX = x
                    this._mouseY = y
                    this._mouseButtons = this._mouseButtons & (~buttonMask(btn))
                    this.onMouseUp.emit(new MouseEvent(this, x, y, btn, ck, sk, ak, mk))
                })
            setOnGpuViewContextMenu(this.handle,
                fn(x: f64, y: f64, btn: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this.onContextMenu.emit(new MouseEvent(this, x, y, btn, ck, sk, ak, mk))
                })
            setOnGpuViewMouseWheel(this.handle,
                fn(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {
                    this._mouseX = x
                    this._mouseY = y
                    let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                    e.wheelDx = dx
                    e.wheelDy = dy
                    this.onMouseWheel.emit(e)
                })
            setOnGpuViewResize(this.handle, fn(w: f64, h: f64) {
                this._width  = w
                this._height = h
                this.onResize.emit(new SizeEvent(this, w, h))
            })
            if this._tooltip != "" { setGpuViewTooltip(this.handle, this._tooltip) }
        }
    }

    /// Move keyboard focus to this view. No-op until attached.
    pub focus() { if this.handle >= 0 { focusGpuView(this.handle) } }
    /// Release focus iff this view holds it. No-op until attached.
    pub blur()  { if this.handle >= 0 { blurGpuView(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setGpuViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}

/// Map a W3C `MouseEvent.button` (0=left, 1=middle, 2=right) into
/// the bitfield `mouseButtons` exposes (bit 0=left, 1=right,
/// 2=middle). Unknown buttons fall through to bit 0 so the
/// snapshot at least flips on something for back / forward
/// thumb buttons.
fn buttonMask(btn: i32): u32 {
    match btn {
        0 { 0x1 as u32 }
        1 { 0x4 as u32 }
        2 { 0x2 as u32 }
        _ { 0x1 as u32 }
    }
}
