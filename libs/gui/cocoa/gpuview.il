/// GPU-backed view — a layer-hosting NSView whose CAMetalLayer is
/// wrapped in a `WGPUSurface`. The gui-core `GpuView` class
/// forwards every property/lifecycle call into the bridge fns
/// declared here; user code writes WGSL pipelines against the
/// surface/device exposed back on the public class.
///
/// Lifecycle:
///   gui-core `attachTo` → `createNativeGpuView`
///     → lazy-init app-wide WGPUInstance/Adapter/Device (first
///       call only — adapter is requested with `compatibleSurface`
///       set to this view's surface, so the platform's preferred
///       backend wins)
///     → create CAMetalLayer + plain NSView
///     → wgpuInstanceCreateSurface(layer)
///     → wgpuSurfaceConfigure (format/alpha/present picked from
///       surface caps[0])
///     → return view handle
///   user → `setOnGpuViewRender(handle, cb)` to stash the render
///     callback; the `Signal<RenderEvent>` shape on the public
///     class wraps that cb.
///   user → `startGpuViewLoop` allocates a CADisplayLink targeted
///     at the view itself (selector `tick:`) and adds it to the
///     main run loop in `kCFRunLoopCommonModes` so it keeps
///     firing during modal tracking. Each tick acquires a drawable,
///     runs a render pass, invokes `onRender(pass)`, submits and
///     presents.
///
/// All handle-typed globals are stored as `i64` so they can live
/// at module scope (the `@handle` types from `wgpu` only allow
/// the literal `0 as T` cast inside an `@extern(C)` block, which
/// is also where every `wgpu*` call has to be made).

use cocoa {
    NSObject, NSView, NSString, NSRunLoop,
    CAMetalLayer, CADisplayLink,
    NSPoint, NSRect, NSSize,
    makeRect, makeSize, nsstr, selectorPtr, nil,
}
use core { WINDOWS }
use wgpu {
    WGPUInstance, WGPUAdapter, WGPUDevice, WGPUQueue, WGPUSurface,
    WGPUTexture, WGPUTextureView, WGPUCommandBuffer, WGPURenderPassEncoder,
    WGPUStringView, WGPUChainedStruct, WGPUColor,
    WGPURequestAdapterOptions,
    WGPURequestAdapterCallbackInfo, WGPURequestDeviceCallbackInfo,
    WGPUSurfaceSourceMetalLayer,
    WGPUSurfaceDescriptor,
    WGPUSurfaceConfiguration, WGPUSurfaceTexture, WGPUSurfaceCapabilities,
    WGPURenderPassColorAttachment, WGPURenderPassDescriptor,
    wgpuSetLogLevel, wgpuSetLogCallback,
    wgpuCreateInstance, wgpuInstanceRequestAdapter, wgpuInstanceProcessEvents,
    wgpuAdapterRequestDevice, wgpuDeviceGetQueue,
    wgpuSurfaceGetCapabilities, wgpuInstanceCreateSurface,
    wgpuSurfaceConfigure, wgpuSurfaceGetCurrentTexture,
    wgpuTextureCreateView, wgpuSurfacePresent,
    wgpuDeviceCreateCommandEncoder, wgpuCommandEncoderBeginRenderPass,
    wgpuRenderPassEncoderEnd, wgpuCommandEncoderFinish, wgpuQueueSubmit,
    wgpuTextureViewRelease, wgpuTextureRelease,
    wgpuRenderPassEncoderRelease, wgpuCommandEncoderRelease,
    wgpuCommandBufferRelease,
}
use std.ffi { readU32, stringFromCstr }

// ─── ObjC subclass: NSView + tick: selector ────────────────────
//
// The CADisplayLink target receives `-(void)tick:(CADisplayLink*)
// link` on the main thread; we use `this.handle` to dispatch back
// into the matching GpuViewSlot. Making the view itself the link
// target avoids a separate helper class (and an extra map lookup).

class GpuViewNSView : NSView {
    /// Top-left origin to match the rest of gui-core's coordinate
    /// convention (Window/Canvas all use flipped). Also forces
    /// the ObjC class registration emit — an empty subclass body
    /// wouldn't generate `.register()`.
    pub isFlipped(): bool { true }
}

/// Per-view CADisplayLink callback target. Plain NSObject
/// subclass (mirrors the ClickHandler pattern in button.il) —
/// new ObjC selectors only register reliably on NSObject
/// descendants, so NSView itself can't carry the `tick:` method.
/// We look the matching GpuView up via `LINK_TARGETS`.
/// CADisplayLink callback target. Plain NSObject subclass
/// (mirrors the ClickHandler pattern in button.il) — NSView-
/// rooted classes register selector overrides cleanly but the
/// `tick:` callback path is friendlier to register on NSObject
/// directly. The owning GpuView's handle is found via
/// `LINK_TARGETS` (link target ptr → view handle).
class GpuViewLink : NSObject {
    /// `link` typed as `NSObject` (CADisplayLink is NSObject-
    /// rooted) so ilang's @objc method-type encoding stays the
    /// same shape as the well-tested ClickHandler pattern.
    pub tick(link: NSObject) {
        if let some(viewHandle) = LINK_TARGETS.get(this.handle) {
            renderOneFrame(viewHandle)
        }
    }
}

/// GpuViewLink ObjC pointer → owning GpuView's handle. Lets the
/// `tick:` callback dispatch into `GPUVIEWS` without putting the
/// view handle on the link target as a typed field.
let LINK_TARGETS = new Map<i64, i64>()

// ─── per-view state ────────────────────────────────────────────
//
// `surface` / `link` are kept as raw `i64` so the slot can live
// outside an `@extern(C)` block. `0` means "uninitialised /
// stopped"; cast back to `WGPUSurface` / `CADisplayLink` at the
// point of use.

class GpuViewSlot {
    pub view:       GpuViewNSView
    pub layer:      CAMetalLayer
    pub surface:    i64           // WGPUSurface, 0 = unset
    pub format:     u32
    pub width:      f64           // in points (matches view.frame)
    pub height:     f64
    pub link:       i64           // CADisplayLink ObjC ptr, 0 = stopped
    pub linkTarget: GpuViewLink   // keeps the tick: target alive
                                  // (CADisplayLink doesn't retain it)
    pub onRender:   fn(WGPURenderPassEncoder)

    pub init(view: GpuViewNSView, layer: CAMetalLayer,
             surface: i64, format: u32, w: f64, h: f64) {
        this.view       = view
        this.layer      = layer
        this.surface    = surface
        this.format     = format
        this.width      = w
        this.height     = h
        this.link       = 0
        this.linkTarget = GpuViewLink.__wrap_handle(0)
        this.onRender   = noOpRender
    }
}

let GPUVIEWS = new Map<i64, GpuViewSlot>()

fn noOpRender(pass: WGPURenderPassEncoder) {}

// ─── app-wide wgpu state ───────────────────────────────────────
//
// One WGPUInstance / Adapter / Device / Queue per process, stored
// as raw `i64` cells so the globals don't need an `@extern(C)`
// scope. Cast back to typed handles inside the `@extern(C)` block
// below.

let WGPU_INST:    i64[]  = [0]
let WGPU_ADAPTER: i64[]  = [0]
let WGPU_DEVICE:  i64[]  = [0]
let WGPU_QUEUE:   i64[]  = [0]
let WGPU_READY:   bool[] = [false]
// Scratch slots for the wgpu callback-info → ilang globals
// handoff (the C callbacks can't capture closures).
let ADAPTER_OUT:  i64[]  = [0]
let DEVICE_OUT:   i64[]  = [0]

@extern(C) {
    // wgpu log + adapter/device request callbacks. Each must be
    // a bare `@extern(C)` fn — the wgpu CallbackInfo struct holds
    // a raw C function pointer, no closure capture available.
    pub fn _gpuOnLog(level: i32, msgData: i64, msgLen: i64, ud: i64) {
        console.log("wgpu[", level as i64, "] ",
                    stringFromCstr(msgData as *const char))
    }
    pub fn _gpuOnAdapter(s: i32, adapter: i64, md: i64, ml: i64, u1: i64, u2: i64) {
        ADAPTER_OUT[0] = adapter
    }
    pub fn _gpuOnDevice(s: i32, device: i64, md: i64, ml: i64, u1: i64, u2: i64) {
        DEVICE_OUT[0] = device
    }

    /// First-call wgpu bring-up. `surface` is the first GpuView's
    /// surface — used as `compatibleSurface` so the adapter picks
    /// a backend that can actually present to a CAMetalLayer.
    /// Returns `true` on success; on failure logs + leaves
    /// `WGPU_READY` false (subsequent GpuViews will fail their
    /// configure step but the rest of the app stays alive).
    fn initWgpuDevice(surfacePtr: i64): bool {
        let surface = surfacePtr as WGPUSurface
        let inst = WGPU_INST[0] as WGPUInstance

        let aOpts = WGPURequestAdapterOptions {
            nextInChain:          0 as *void,
            featureLevel:         0 as u32,
            powerPreference:      0 as u32,
            forceFallbackAdapter: 0 as u32,
            backendType:          0 as u32,
            compatibleSurface:    surface,
        }
        let aInfo = WGPURequestAdapterCallbackInfo {
            nextInChain: 0 as *void,
            mode:        2 as u32,        // AllowProcessEvents
            callback:    _gpuOnAdapter,
            userdata1:   0 as *void,
            userdata2:   0 as *void,
        }
        ADAPTER_OUT[0] = 0
        wgpuInstanceRequestAdapter(inst, (&aOpts) as *void, aInfo)
        wgpuInstanceProcessEvents(inst)
        if ADAPTER_OUT[0] == 0 {
            console.log("GpuView: wgpuInstanceRequestAdapter callback never fired")
            return false
        }
        WGPU_ADAPTER[0] = ADAPTER_OUT[0]
        let adapter = ADAPTER_OUT[0] as WGPUAdapter

        let dInfo = WGPURequestDeviceCallbackInfo {
            nextInChain: 0 as *void,
            mode:        2 as u32,
            callback:    _gpuOnDevice,
            userdata1:   0 as *void,
            userdata2:   0 as *void,
        }
        DEVICE_OUT[0] = 0
        wgpuAdapterRequestDevice(adapter, 0 as *void, dInfo)
        wgpuInstanceProcessEvents(inst)
        if DEVICE_OUT[0] == 0 {
            console.log("GpuView: wgpuAdapterRequestDevice callback never fired")
            return false
        }
        WGPU_DEVICE[0] = DEVICE_OUT[0]
        let device = DEVICE_OUT[0] as WGPUDevice
        let queue = wgpuDeviceGetQueue(device)
        WGPU_QUEUE[0] = queue as i64
        WGPU_READY[0] = true
        true
    }

    /// Create the WGPUSurface for a layer (using the cached
    /// instance, creating one if needed). Returns the surface
    /// pointer as `i64` (0 on failure).
    fn makeSurfaceForLayer(layerPtr: i64): i64 {
        if WGPU_INST[0] == 0 {
            wgpuSetLogCallback(_gpuOnLog, 0 as *void)
            wgpuSetLogLevel(2 as u32)         // Warn — quiet on the happy path
            let inst = wgpuCreateInstance(0 as *void)
            WGPU_INST[0] = inst as i64
        }
        let metalSrc = WGPUSurfaceSourceMetalLayer {
            chain: WGPUChainedStruct { next: 0 as *void, sType: 4 as u32 },
            layer: layerPtr as *void,
        }
        let sdesc = WGPUSurfaceDescriptor {
            nextInChain: (&metalSrc) as *void,
            label:       WGPUStringView { data: 0 as *void, length: 0 as u64 },
        }
        let surf = wgpuInstanceCreateSurface(WGPU_INST[0] as WGPUInstance, &sdesc)
        surf as i64
    }

    /// Configure / re-configure the surface from its capabilities.
    /// Picks formats[0] / alphaModes[0] / presentModes[0] — wgpu's
    /// own enumeration order puts the preferred values first.
    /// Returns the chosen format (0 on failure).
    fn configureSurfaceFor(surfacePtr: i64, pxW: u32, pxH: u32): u32 {
        let surface = surfacePtr as WGPUSurface
        let adapter = WGPU_ADAPTER[0] as WGPUAdapter
        let caps = new WGPUSurfaceCapabilities()
        wgpuSurfaceGetCapabilities(surface, adapter, &caps)
        if caps.formatCount == 0 as u64 {
            console.log("GpuView: surface has no supported formats")
            return 0 as u32
        }
        let fmt     = readU32(caps.formats as *const void, 0)
        let alpha   = readU32(caps.alphaModes as *const void, 0)
        let present = readU32(caps.presentModes as *const void, 0)

        let cfg = WGPUSurfaceConfiguration {
            nextInChain:     0 as *void,
            device:          WGPU_DEVICE[0] as WGPUDevice,
            format:          fmt,
            usage:           0x10 as u64,    // RenderAttachment
            width:           pxW,
            height:          pxH,
            viewFormatCount: 0 as u64,
            viewFormats:     0 as *void,
            alphaMode:       alpha,
            presentMode:     present,
        }
        wgpuSurfaceConfigure(surface, &cfg)
        fmt
    }

    fn renderOneFrame(handle: i64) {
        if !WGPU_READY[0] { return }
        let slot = match GPUVIEWS.get(handle) {
            some(s) { s }
            none    { return }
        }
        let surface = slot.surface as WGPUSurface
        let st = new WGPUSurfaceTexture()
        wgpuSurfaceGetCurrentTexture(surface, &st)
        // `surface.getCurrentTexture` returns null on the first
        // few ticks (CAMetalLayer needs the window composited
        // first) and any time the window is occluded / minimised.
        // Silent skip is correct — next tick retries.
        if (st.texture as i64) == 0 { return }

        let device = WGPU_DEVICE[0] as WGPUDevice
        let queue  = WGPU_QUEUE[0] as WGPUQueue

        let tv = wgpuTextureCreateView(st.texture, 0 as *void)
        let enc = wgpuDeviceCreateCommandEncoder(device, 0 as *void)
        let att = WGPURenderPassColorAttachment {
            nextInChain:   0 as *void,
            view:          tv,
            depthSlice:    0xFFFFFFFF as u32,
            resolveTarget: 0 as WGPUTextureView,
            loadOp:        2 as u32,        // Clear
            storeOp:       1 as u32,        // Store
            clearValue:    WGPUColor { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
        }
        let rp = WGPURenderPassDescriptor {
            nextInChain:            0 as *void,
            label:                  WGPUStringView { data: 0 as *void, length: 0 as u64 },
            colorAttachmentCount:   1 as u64,
            colorAttachments:       &att,
            depthStencilAttachment: 0 as *void,
            occlusionQuerySet:      0 as *void,
            timestampWrites:        0 as *void,
        }
        let pass = wgpuCommandEncoderBeginRenderPass(enc, &rp)

        let cb = slot.onRender
        cb(pass)

        wgpuRenderPassEncoderEnd(pass)
        let cmd = wgpuCommandEncoderFinish(enc, 0 as *void)
        let cmds: WGPUCommandBuffer[] = [cmd]
        wgpuQueueSubmit(queue, 1 as u64, cmds)
        wgpuSurfacePresent(surface)

        wgpuCommandBufferRelease(cmd)
        wgpuRenderPassEncoderRelease(pass)
        wgpuCommandEncoderRelease(enc)
        wgpuTextureViewRelease(tv)
        wgpuTextureRelease(st.texture)
    }
}

// ─── public bridge API ─────────────────────────────────────────

pub fn createNativeGpuView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }

    // Build the layer-hosting NSView. `setLayer:` MUST come
    // before `setWantsLayer:` for the layer-hosting path
    // (otherwise AppKit allocates its own layer and ours is
    // ignored).
    GpuViewNSView.register()
    let view = GpuViewNSView.alloc().init()
    view.setFrame(makeRect(x, y, width, height))
    view.setHidden(!visible)

    let scale = winSlot.win.backingScaleFactor
    let layer = CAMetalLayer.alloc().init()
    layer.setFrame(makeRect(0.0, 0.0, width, height))
    layer.setContentsScale(scale)
    layer.setDrawableSize(makeSize(width * scale, height * scale))
    layer.setOpaque(true)

    view.setLayer(layer as NSObject)
    view.setWantsLayer(true)
    winSlot.win.contentView.addSubview(view as NSView)

    let surfacePtr = makeSurfaceForLayer(layer.handle)
    if surfacePtr == 0 {
        console.log("GpuView: wgpuInstanceCreateSurface failed")
        return -1
    }
    if !WGPU_READY[0] {
        if !initWgpuDevice(surfacePtr) { return -1 }
    }
    let pxW = (width  * scale) as u32
    let pxH = (height * scale) as u32
    let fmt = configureSurfaceFor(surfacePtr, pxW, pxH)
    if fmt == (0 as u32) { return -1 }

    let slot = new GpuViewSlot(view, layer, surfacePtr, fmt, width, height)
    let handle = view.handle
    GPUVIEWS[handle] = slot
    handle
}

pub fn setGpuViewFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = GPUVIEWS.get(handle) {
        slot.view.setFrame(makeRect(x, y, w, h))
        if w != slot.width || h != slot.height {
            slot.width  = w
            slot.height = h
            let scale = slot.view.window.backingScaleFactor
            slot.layer.setFrame(makeRect(0.0, 0.0, w, h))
            slot.layer.setDrawableSize(makeSize(w * scale, h * scale))
            if WGPU_READY[0] {
                configureSurfaceFor(slot.surface, (w * scale) as u32, (h * scale) as u32)
            }
        }
    }
}

pub fn setGpuViewVisible(handle: i64, visible: bool) {
    if let some(slot) = GPUVIEWS.get(handle) {
        slot.view.setHidden(!visible)
    }
}

pub fn setOnGpuViewRender(handle: i64, cb: fn(WGPURenderPassEncoder)) {
    if let some(slot) = GPUVIEWS.get(handle) {
        slot.onRender = cb
    }
}

pub fn getGpuViewSurface(handle: i64): WGPUSurface {
    match GPUVIEWS.get(handle) {
        some(s) { s.surface as WGPUSurface }
        none    { 0 as WGPUSurface }
    }
}
pub fn getGpuViewDevice(handle: i64): WGPUDevice {
    if WGPU_READY[0] { WGPU_DEVICE[0] as WGPUDevice } else { 0 as WGPUDevice }
}
pub fn getGpuViewQueue(handle: i64): WGPUQueue {
    if WGPU_READY[0] { WGPU_QUEUE[0] as WGPUQueue } else { 0 as WGPUQueue }
}
pub fn getGpuViewFormat(handle: i64): u32 {
    match GPUVIEWS.get(handle) {
        some(s) { s.format }
        none    { 0 as u32 }
    }
}

pub fn startGpuViewLoop(handle: i64) {
    if let some(slot) = GPUVIEWS.get(handle) {
        if slot.link != 0 { return }   // already running

        GpuViewLink.register()
        let target = GpuViewLink.alloc().init()
        LINK_TARGETS[target.handle] = handle

        let link = CADisplayLink.displayLinkWithTarget(
            target as NSObject, selectorPtr("tick:")
        )
        // Real run-loop modes, individually. `kCFRunLoopCommonModes`
        // looks like the obvious answer but it's a CFRunLoop GROUP
        // ALIAS, not a real mode — CADisplayLink's
        // `addToRunLoop:forMode:` silently refuses to fire when
        // handed it. So we register against each concrete mode the
        // app might be in:
        //   * `kCFRunLoopDefaultMode`        — normal event-loop idle
        //   * `NSEventTrackingRunLoopMode`   — window drag, scroll,
        //                                      mouse-down tracking
        //   * `NSModalPanelRunLoopMode`      — modal sheets / panels
        // Note: only the default mode is CFRunLoop-prefixed; AppKit's
        // own modes carry their raw NSString value as the constant.
        let runloop = NSRunLoop.main() as NSObject
        link.addToRunLoop(runloop, nsstr("kCFRunLoopDefaultMode"))
        link.addToRunLoop(runloop, nsstr("NSEventTrackingRunLoopMode"))
        link.addToRunLoop(runloop, nsstr("NSModalPanelRunLoopMode"))

        slot.link       = link.handle
        slot.linkTarget = target
    }
}

pub fn stopGpuViewLoop(handle: i64) {
    if let some(slot) = GPUVIEWS.get(handle) {
        if slot.link == 0 { return }
        CADisplayLink.__wrap_handle(slot.link).invalidate()
        LINK_TARGETS.delete(slot.linkTarget.handle)
        slot.link       = 0
        slot.linkTarget = GpuViewLink.__wrap_handle(0)
    }
}

/// Request a single frame outside the CADisplayLink loop. Useful
/// when `startGpuViewLoop` hasn't been called and the user wants
/// to redraw on demand (e.g. after editing model state).
pub fn requestGpuViewFrame(handle: i64) {
    renderOneFrame(handle)
}
