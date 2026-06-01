/// GPU drawing surface — Canvas-style widget whose backing store
/// is a `WGPUSurface` (CAMetalLayer on macOS; Windows / Linux
/// still stubbed). User code builds a wgpu render pipeline once
/// against the exposed `device` / `queue` / `format`, then
/// records draw commands into a `WGPURenderPassEncoder` inside
/// the `onRender` callback.
///
///   use gui
///   use wgpu { wgpuRenderPassEncoderSetPipeline, wgpuRenderPassEncoderDraw }
///
///   let w = new gui.Window("triangle", 640.0, 480.0)
///   let v = new gui.GpuView()
///   v.x = 0.0; v.y = 0.0; v.width = 640.0; v.height = 480.0
///   w.addChild(v)                            // attachTo runs;
///                                            // v.device / v.format
///                                            // are now valid.
///   let pipeline = ...                       // build with v.device
///   v.onRender.add(fn(e: gui.RenderEvent) {
///       wgpuRenderPassEncoderSetPipeline(e.pass, pipeline)
///       wgpuRenderPassEncoderDraw(e.pass, 3, 1, 0, 0)
///   })
///   v.startLoop()                            // CADisplayLink @ 60Hz
///   gui.run()
///
/// On platforms without a `GpuView` backend (Windows / Linux),
/// `attachTo` returns a `-1` handle and every method / property
/// access is a silent no-op — same fallback Canvas takes when
/// drawing before realisation.

use window { Window, Control }
use events { EventTarget, Event }
use std.events { Signal }
use wgpu {
    WGPUSurface, WGPUDevice, WGPUQueue, WGPURenderPassEncoder,
}
use gui_impl {
    createNativeGpuView, setGpuViewFrame, setGpuViewVisible,
    setOnGpuViewRender,
    getGpuViewSurface, getGpuViewDevice, getGpuViewQueue, getGpuViewFormat,
    startGpuViewLoop, stopGpuViewLoop, requestGpuViewFrame,
}

/// One per-tick render callback. `pass` is a live
/// `WGPURenderPassEncoder` with the colour attachment already
/// cleared; user code calls `setPipeline` / `draw` / etc. on it
/// and returns. The library ends the pass, submits, and presents.
pub class RenderEvent : Event {
    pub pass: WGPURenderPassEncoder
    pub init(sender: EventTarget, pass: WGPURenderPassEncoder) {
        super(sender)
        this.pass = pass
    }
}

pub class GpuView : Control {
    handle:       i64
    _x:           f64
    _y:           f64
    _width:       f64
    _height:      f64
    _isVisible:   bool

    /// Fires once per display refresh while the loop is running
    /// (`startLoop` → `stopLoop`), or once per `requestFrame`
    /// call. Multiple listeners run in registration order on the
    /// same encoder.
    pub onRender: Signal<RenderEvent>

    pub init() {
        super()
        this.handle     = -1
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 400.0
        this._height    = 300.0
        this._isVisible = true
        this.onRender   = new Signal<RenderEvent>()
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

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setGpuViewVisible(this.handle, v) }
    }

    // ─── low-level escape hatches ──────────────────────────────
    //
    // Populated by attachTo and read straight from the backing
    // GpuViewSlot — no caching, so they always reflect the
    // current configuration even after a resize.

    pub get surface(): WGPUSurface { getGpuViewSurface(this.handle) }
    pub get device():  WGPUDevice  { getGpuViewDevice(this.handle) }
    pub get queue():   WGPUQueue   { getGpuViewQueue(this.handle) }
    /// Surface texture format picked at configure time
    /// (`WGPUTextureFormat` discriminant). Match this when
    /// building the render pipeline's colour target.
    pub get format():  u32         { getGpuViewFormat(this.handle) }

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
            this._isVisible
        )
        if this.handle >= 0 {
            // Bridge the bare-fn render callback into the
            // Signal<RenderEvent>. One subscription per native
            // view; user-side `onRender.add` fan-out happens
            // inside the signal.
            setOnGpuViewRender(this.handle, fn(pass: WGPURenderPassEncoder) {
                this.onRender.emit(new RenderEvent(this, pass))
            })
        }
    }
}
