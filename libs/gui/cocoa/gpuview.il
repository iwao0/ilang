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
    NSObject, NSView, NSString, NSRunLoop, NSEvent,
    NSEventModifierFlags, KeyCode,
    NSTrackingArea, NSTrackingAreaOptions,
    CAMetalLayer, CADisplayLink,
    NSPoint, NSRect, NSSize,
    makeRect, makeSize, nsstr, selectorPtr, nil,
}
use core {
    WINDOWS, noOp1, noOpEnterLeave,
    keyCodeToString, modShift, modCtrl, modAlt, modMeta,
}
use std.time { monotonicNs }
use wgpu29 {
    WGPUInstance, WGPUAdapter, WGPUDevice, WGPUQueue, WGPUSurface,
    WGPUTexture, WGPUTextureView, WGPUCommandBuffer, WGPURenderPassEncoder,
    WGPUStringView, WGPUChainedStruct, WGPUColor,
    WGPURequestAdapterOptions,
    WGPURequestAdapterCallbackInfo, WGPURequestDeviceCallbackInfo,
    WGPUSurfaceSourceMetalLayer,
    WGPUSurfaceDescriptor,
    WGPUSurfaceConfiguration, WGPUSurfaceTexture, WGPUSurfaceCapabilities,
    WGPURenderPassColorAttachment, WGPURenderPassDescriptor,
    TextureFormat, TextureUsage, FeatureLevel, PowerPreference, BackendType,
    CallbackMode, SType, LoadOp, StoreOp, LogLevel, PresentMode,
    CompositeAlphaMode,
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

// ─── ObjC subclass: NSView + tick: selector + input ───────────
//
// The CADisplayLink target receives `-(void)tick:(CADisplayLink*)
// link` on the main thread; we use `this.handle` to dispatch back
// into the matching GpuViewSlot. Making the view itself the link
// target avoids a separate helper class (and an extra map lookup).
//
// The keyboard / mouse overrides marshal each NSEvent into a
// per-slot callback the public-class bridge installed. Coords
// are converted to view-local pixels (top-left origin — the view
// is `isFlipped`).

class GpuViewNSView : NSView {
    /// Top-left origin to match the rest of gui-core's coordinate
    /// convention (Window/Canvas all use flipped). Also forces
    /// the ObjC class registration emit — an empty subclass body
    /// wouldn't generate `.register()`.
    pub isFlipped(): bool { true }

    /// Take first responder so keyDown:/keyUp: arrive at this view
    /// instead of falling back to the window's content view.
    /// `isEnabled=false` opts the view out of input dispatch.
    pub acceptsFirstResponder(): bool {
        match GPUVIEWS.get(this.handle) {
            some(slot) { slot.isEnabled }
            none       { true }
        }
    }
    pub becomeFirstResponder(): bool {
        if let some(slot) = GPUVIEWS.get(this.handle) {
            slot.onFocus()
        }
        true
    }
    pub resignFirstResponder(): bool {
        if let some(slot) = GPUVIEWS.get(this.handle) {
            slot.onBlur()
        }
        true
    }

    pub keyDown(event: NSEvent) {
        if let some(slot) = GPUVIEWS.get(this.handle) {
            if !slot.isEnabled { return }
            let flags = event.modifierFlags as u64
            slot.onKeyDown(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub keyUp(event: NSEvent) {
        if let some(slot) = GPUVIEWS.get(this.handle) {
            if !slot.isEnabled { return }
            let flags = event.modifierFlags as u64
            slot.onKeyUp(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub mouseMoved(event: NSEvent)     { this.dispatchMouse(event, 0, 0) }
    pub mouseDragged(event: NSEvent)   { this.dispatchMouse(event, 0, 0) }
    /// See GuiView's note in core.il: NSView's mouseDown / mouseUp
    /// are typed `NSObject` in the binding to avoid pulling NSEvent
    /// into that scope; the override has to match. The `as?` checked
    /// downcast doesn't work on @objc objects (ilang reads `obj+0`
    /// as the ilang class id, but it's actually the objc isa
    /// pointer there) — rebind via `__wrap_handle_unowned` instead.
    pub override mouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        this.dispatchMouse(e, 1, 0)
    }
    pub override mouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        this.dispatchMouse(e, 2, 0)
    }
    pub override rightMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        this.dispatchMouse(e, 1, 2)
        if let some(slot) = GPUVIEWS.get(this.handle) {
            if !slot.isEnabled { return }
            let p = this.convertPointFromView(e.locationInWindow, nil)
            let flags = e.modifierFlags as u64
            let ctx = slot.onContextMenu
            ctx(p.x, p.y, 2,
                modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub override rightMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        this.dispatchMouse(e, 2, 2)
    }
    pub rightMouseDragged(event: NSEvent) { this.dispatchMouse(event, 0, 2) }
    pub override otherMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        this.dispatchMouse(e, 1, 1)
    }
    pub override otherMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        this.dispatchMouse(e, 2, 1)
    }
    pub otherMouseDragged(event: NSEvent) { this.dispatchMouse(event, 0, 1) }

    pub override mouseEntered(event: NSObject) {
        if let some(slot) = GPUVIEWS.get(this.handle) {
            if !slot.isEnabled { return }
            let e = NSEvent.__wrap_handle_unowned(event.handle)
            let p = this.convertPointFromView(e.locationInWindow, nil)
            slot.onMouseEnter(p.x, p.y)
        }
    }
    pub override mouseExited(event: NSObject) {
        if let some(slot) = GPUVIEWS.get(this.handle) {
            if !slot.isEnabled { return }
            let e = NSEvent.__wrap_handle_unowned(event.handle)
            let p = this.convertPointFromView(e.locationInWindow, nil)
            slot.onMouseLeave(p.x, p.y)
        }
    }

    pub scrollWheel(event: NSEvent) {
        if let some(slot) = GPUVIEWS.get(this.handle) {
            if !slot.isEnabled { return }
            let p = this.convertPointFromView(event.locationInWindow, nil)
            let flags = event.modifierFlags as u64
            slot.onMouseWheel(event.deltaX, event.deltaY, p.x, p.y,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }

    /// `kind`: 0=move, 1=down, 2=up. `button`: W3C MouseEvent.button.
    dispatchMouse(event: NSEvent, kind: i32, button: i32) {
        if let some(slot) = GPUVIEWS.get(this.handle) {
            if !slot.isEnabled { return }
            let p = this.convertPointFromView(event.locationInWindow, nil)
            let flags = event.modifierFlags as u64
            // Note: NSWindow's `setAcceptsMouseMovedEvents(true)` is
            // already on for the parent — that delivers global motion
            // even when the cursor strays outside the view. AppKit
            // sends `mouseMoved:` straight to the firstResponder
            // (window's content view), not subviews, so this
            // override only fires once the user clicks into the view
            // and we install ourselves as firstResponder — for now,
            // motion outside the view simply doesn't fire here, no
            // clip is required.
            match kind {
                0 {
                    // For move events the button parameter is the
                    // current button bitfield encoded by the caller;
                    // we pass 0 here and let the public class merge
                    // with its own snapshot. Pass as u32.
                    slot.onMouseMove(p.x, p.y, 0 as u32,
                       modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
                }
                1 {
                    slot.onMouseDown(p.x, p.y, button,
                       modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
                }
                2 {
                    slot.onMouseUp(p.x, p.y, button,
                       modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
                }
                _ {}
            }
        }
    }
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
    pub isEnabled:  bool
    pub link:       i64           // CADisplayLink ObjC ptr, 0 = stopped
    pub linkTarget: GpuViewLink   // keeps the tick: target alive
                                  // (CADisplayLink doesn't retain it)
    pub onRender:   fn(WGPURenderPassEncoder, f64, f64)
    // Input bridges installed by the public class. Each takes the
    // shape the matching `setOnGpuViewXxx` declares.
    pub onFocus:      fn()
    pub onBlur:       fn()
    pub onKeyDown:    fn(string, bool, bool, bool, bool)
    pub onKeyUp:      fn(string, bool, bool, bool, bool)
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseMove:  fn(f64, f64, u32, bool, bool, bool, bool)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub onResize:     fn(f64, f64)
    // Frame-time bookkeeping. All in monotonic nanoseconds so the
    // delta is robust across NTP slew / DST. `startNs == 0` flags
    // "no frame yet" — first frame sets it to the current ns and
    // reports `time=0, dt=0`.
    pub startNs:    i64
    pub lastNs:     i64

    pub init(view: GpuViewNSView, layer: CAMetalLayer,
             surface: i64, format: u32, w: f64, h: f64) {
        this.view         = view
        this.layer        = layer
        this.surface      = surface
        this.format       = format
        this.width        = w
        this.height       = h
        this.isEnabled    = true
        this.link         = 0
        this.linkTarget   = GpuViewLink.__wrap_handle(0)
        this.onRender     = gpuNoOpRender
        this.onFocus      = noOp1
        this.onBlur       = noOp1
        this.onKeyDown    = gpuNoOpKey
        this.onKeyUp      = gpuNoOpKey
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseMove  = gpuNoOpMove
        this.onMouseDown  = gpuNoOpButton
        this.onMouseUp    = gpuNoOpButton
        this.onContextMenu = gpuNoOpButton
        this.onMouseWheel = gpuNoOpWheel
        this.onResize     = gpuNoOpResize
        this.startNs      = 0 as i64
        this.lastNs       = 0 as i64
    }
}

let GPUVIEWS = new Map<i64, GpuViewSlot>()

fn gpuNoOpRender(pass: WGPURenderPassEncoder, t: f64, dt: f64) {}
fn gpuNoOpKey(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn gpuNoOpMove(x: f64, y: f64, b: u32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn gpuNoOpButton(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn gpuNoOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn gpuNoOpResize(w: f64, h: f64) {}

// ─── app-wide wgpu state ───────────────────────────────────────
//
// One WGPUInstance / Adapter / Device / Queue per process, stored
// as raw `i64` cells so the globals don't need an `@extern(C)`
// scope. Cast back to typed handles inside the `@extern(C)` block
// below.

let WGPU_INST:    i64  = 0
let WGPU_ADAPTER: i64  = 0
let WGPU_DEVICE:  i64  = 0
let WGPU_QUEUE:   i64  = 0
let WGPU_READY:   bool = false
// Scratch slots for the wgpu callback-info → ilang globals
// handoff (the C callbacks can't capture closures).
let ADAPTER_OUT:  i64  = 0
let DEVICE_OUT:   i64  = 0

@extern(C) {
    // wgpu log + adapter/device request callbacks. Each must be
    // a bare `@extern(C)` fn — the wgpu CallbackInfo struct holds
    // a raw C function pointer, no closure capture available.
    pub fn _gpuOnLog(level: i32, msgData: i64, msgLen: i64, ud: i64) {
        console.log("wgpu[", level as i64, "] ",
                    stringFromCstr(msgData as *const char))
    }
    pub fn _gpuOnAdapter(s: i32, adapter: i64, md: i64, ml: i64, u1: i64, u2: i64) {
        ADAPTER_OUT = adapter
    }
    pub fn _gpuOnDevice(s: i32, device: i64, md: i64, ml: i64, u1: i64, u2: i64) {
        DEVICE_OUT = device
    }

    /// First-call wgpu bring-up. `surface` is the first GpuView's
    /// surface — used as `compatibleSurface` so the adapter picks
    /// a backend that can actually present to a CAMetalLayer.
    /// Returns `true` on success; on failure logs + leaves
    /// `WGPU_READY` false (subsequent GpuViews will fail their
    /// configure step but the rest of the app stays alive).
    fn initWgpuDevice(surfacePtr: i64): bool {
        let surface = surfacePtr as WGPUSurface
        let inst = WGPU_INST as WGPUInstance

        let aOpts = WGPURequestAdapterOptions {
            nextInChain:          0 as *void,
            featureLevel:         FeatureLevel.undefined,
            powerPreference:      PowerPreference.undefined,
            forceFallbackAdapter: 0 as u32,           // WGPUBool
            backendType:          BackendType.undefined,
            compatibleSurface:    surface,
        }
        let aInfo = WGPURequestAdapterCallbackInfo {
            nextInChain: 0 as *void,
            mode:        CallbackMode.allowProcessEvents,
            callback:    _gpuOnAdapter,
            userdata1:   0 as *void,
            userdata2:   0 as *void,
        }
        ADAPTER_OUT = 0
        wgpuInstanceRequestAdapter(inst, (&aOpts) as *void, aInfo)
        wgpuInstanceProcessEvents(inst)
        if ADAPTER_OUT == 0 {
            console.log("GpuView: wgpuInstanceRequestAdapter callback never fired")
            return false
        }
        WGPU_ADAPTER = ADAPTER_OUT
        let adapter = ADAPTER_OUT as WGPUAdapter

        let dInfo = WGPURequestDeviceCallbackInfo {
            nextInChain: 0 as *void,
            mode:        CallbackMode.allowProcessEvents,
            callback:    _gpuOnDevice,
            userdata1:   0 as *void,
            userdata2:   0 as *void,
        }
        DEVICE_OUT = 0
        wgpuAdapterRequestDevice(adapter, 0 as *void, dInfo)
        wgpuInstanceProcessEvents(inst)
        if DEVICE_OUT == 0 {
            console.log("GpuView: wgpuAdapterRequestDevice callback never fired")
            return false
        }
        WGPU_DEVICE = DEVICE_OUT
        let device = DEVICE_OUT as WGPUDevice
        let queue = wgpuDeviceGetQueue(device)
        WGPU_QUEUE = queue as i64
        WGPU_READY = true
        true
    }

    /// Create the WGPUSurface for a layer (using the cached
    /// instance, creating one if needed). Returns the surface
    /// pointer as `i64` (0 on failure).
    fn makeSurfaceForLayer(layerPtr: i64): i64 {
        if WGPU_INST == 0 {
            wgpuSetLogCallback(_gpuOnLog, 0 as *void)
            wgpuSetLogLevel(LogLevel.warn as u32)         // quiet on the happy path
            let inst = wgpuCreateInstance(0 as *void)
            WGPU_INST = inst as i64
        }
        let metalSrc = WGPUSurfaceSourceMetalLayer {
            chain: WGPUChainedStruct { next: 0 as *void, sType: SType.surfaceSourceMetalLayer },
            layer: layerPtr as *void,
        }
        let sdesc = WGPUSurfaceDescriptor {
            nextInChain: (&metalSrc) as *void,
            label:       WGPUStringView { data: 0 as *void, length: 0 as u64 },
        }
        let surf = wgpuInstanceCreateSurface(WGPU_INST as WGPUInstance, &sdesc)
        surf as i64
    }

    /// Configure / re-configure the surface from its capabilities.
    /// Picks formats[0] / alphaModes[0] / presentModes[0] — wgpu's
    /// own enumeration order puts the preferred values first.
    /// Returns the chosen format (0 on failure).
    fn configureSurfaceFor(surfacePtr: i64, pxW: u32, pxH: u32): u32 {
        let surface = surfacePtr as WGPUSurface
        let adapter = WGPU_ADAPTER as WGPUAdapter
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
            device:          WGPU_DEVICE as WGPUDevice,
            format:          fmt as TextureFormat,
            usage:           TextureUsage.renderAttachment,
            width:           pxW,
            height:          pxH,
            viewFormatCount: 0 as u64,
            viewFormats:     0 as *void,
            alphaMode:       alpha as CompositeAlphaMode,
            presentMode:     present as PresentMode,
        }
        wgpuSurfaceConfigure(surface, &cfg)
        fmt
    }

    fn renderOneFrame(handle: i64) {
        if !WGPU_READY { return }
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

        let device = WGPU_DEVICE as WGPUDevice
        let queue  = WGPU_QUEUE as WGPUQueue

        let tv = wgpuTextureCreateView(st.texture, 0 as *void)
        let enc = wgpuDeviceCreateCommandEncoder(device, 0 as *void)
        let att = WGPURenderPassColorAttachment {
            nextInChain:   0 as *void,
            view:          tv,
            depthSlice:    0xFFFFFFFF as u32,
            resolveTarget: 0 as WGPUTextureView,
            loadOp:        LoadOp.clear,
            storeOp:       StoreOp.store,
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

        // Sample the monotonic clock once per frame. First-frame
        // semantics: `time=0, dt=0`. Computed here (native side)
        // rather than ilang-side so multi-listener fan-out sees a
        // single consistent value.
        let nowNs = monotonicNs()
        if slot.startNs == 0 as i64 {
            slot.startNs = nowNs
            slot.lastNs  = nowNs
        }
        let time = ((nowNs - slot.startNs) as f64) / 1.0e9
        let dt   = ((nowNs - slot.lastNs) as f64) / 1.0e9
        slot.lastNs = nowNs

        slot.onRender(pass, time, dt)

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
    x: f64, y: f64, width: f64, height: f64, enabled: bool, visible: bool
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
    // Pin layer contents to top-left so AppKit clips (instead of
    // stretching the last-drawn frame) when bounds change between
    // our redraws during live window-resize. Default
    // kCAGravityResize would visibly stretch the GpuView upward as
    // the window grows, overlapping sibling controls.
    layer.setContentsGravity(nsstr("topLeft"))
    layer.setFrame(makeRect(0.0, 0.0, width, height))
    layer.setContentsScale(scale)
    layer.setDrawableSize(makeSize(width * scale, height * scale))
    layer.setOpaque(true)

    view.setLayer(layer as NSObject)
    view.setWantsLayer(true)
    // Tracking area for mouseEntered: / mouseExited:.
    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        view as NSObject, nil
    )
    (view as NSView).addTrackingArea(area as NSObject)
    // Place the GpuView UNDER any sibling controls (NSWindowBelow
    // = -1, relativeTo nil = "below all siblings"). Without this,
    // GpuView lands on top because it's added last, and during
    // live window resize AppKit visibly slides / stretches the
    // CAMetalLayer over the Label widgets that should be above
    // it. Keeping it at the bottom makes Label / Button / etc.
    // always overdraw it.
    winSlot.win.contentView.addSubviewPositioned(view as NSView, -1 as i64, NSView.__wrap_handle(0))
    // Steal first responder so keyDown:/keyUp: go straight to
    // the GpuView. Users who want the surrounding window to keep
    // keyboard focus can flip this back themselves via the
    // public `Window.handle` afterwards.
    winSlot.win.makeFirstResponder(view as NSObject)

    let surfacePtr = makeSurfaceForLayer(layer.handle)
    if surfacePtr == 0 {
        console.log("GpuView: wgpuInstanceCreateSurface failed")
        return -1
    }
    if !WGPU_READY {
        if !initWgpuDevice(surfacePtr) { return -1 }
    }
    let pxW = (width  * scale) as u32
    let pxH = (height * scale) as u32
    let fmt = configureSurfaceFor(surfacePtr, pxW, pxH)
    if fmt == (0 as u32) { return -1 }

    let slot = new GpuViewSlot(view, layer, surfacePtr, fmt, width, height)
    slot.isEnabled = enabled
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
            // Don't touch `layer.frame`. The NSView is layer-hosting,
            // so AppKit syncs `view.frame` → hosted layer's frame
            // automatically. Overriding `layer.setFrame(0, 0, w, h)`
            // here would race against that sync during live resize
            // and visibly push the CAMetalLayer to the parent's
            // top-left (overlapping sibling controls). Only the
            // metal drawable size has to be updated explicitly.
            slot.layer.setDrawableSize(makeSize(w * scale, h * scale))
            if WGPU_READY {
                configureSurfaceFor(slot.surface, (w * scale) as u32, (h * scale) as u32)
            }
            slot.onResize(w, h)
        }
    }
}

pub fn setGpuViewVisible(handle: i64, visible: bool) {
    if let some(slot) = GPUVIEWS.get(handle) {
        slot.view.setHidden(!visible)
    }
}

/// NSView has no native `enabled` state — track the flag in
/// `GpuViewSlot` and gate input dispatch on it.
pub fn setGpuViewEnabled(handle: i64, enabled: bool) {
    if let some(slot) = GPUVIEWS.get(handle) {
        slot.isEnabled = enabled
        if !enabled {
            let win = (slot.view as NSView).window
            if win.firstResponder.handle == (slot.view as NSView).handle {
                let _ = win.makeFirstResponder(nil)
            }
        }
    }
}

pub fn setGpuViewTooltip(handle: i64, text: string) {
    if let some(slot) = GPUVIEWS.get(handle) {
        if text == "" {
            (slot.view as NSView).setToolTip(nil)
        } else {
            (slot.view as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}

pub fn setOnGpuViewRender(handle: i64, cb: fn(WGPURenderPassEncoder, f64, f64)) {
    if let some(slot) = GPUVIEWS.get(handle) {
        slot.onRender = cb
    }
}

pub fn setOnGpuViewKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = GPUVIEWS.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnGpuViewKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = GPUVIEWS.get(handle) { slot.onKeyUp = cb }
}
pub fn setOnGpuViewFocus(handle: i64, cb: fn()) {
    if let some(slot) = GPUVIEWS.get(handle) { slot.onFocus = cb }
}
pub fn setOnGpuViewBlur(handle: i64, cb: fn()) {
    if let some(slot) = GPUVIEWS.get(handle) { slot.onBlur = cb }
}
pub fn setOnGpuViewMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = GPUVIEWS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnGpuViewMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = GPUVIEWS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnGpuViewContextMenu(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if let some(slot) = GPUVIEWS.get(handle) { slot.onContextMenu = cb }
}
pub fn focusGpuView(handle: i64) {
    if let some(slot) = GPUVIEWS.get(handle) {
        let win = (slot.view as NSView).window
        let _ = win.makeFirstResponder(slot.view as NSObject)
    }
}
pub fn blurGpuView(handle: i64) {
    if let some(slot) = GPUVIEWS.get(handle) {
        let win = (slot.view as NSView).window
        if win.firstResponder.handle == (slot.view as NSView).handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}
pub fn setOnGpuViewMouseMove(handle: i64, cb: fn(f64, f64, u32, bool, bool, bool, bool)) {
    if let some(slot) = GPUVIEWS.get(handle) { slot.onMouseMove = cb }
}
pub fn setOnGpuViewMouseDown(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if let some(slot) = GPUVIEWS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnGpuViewMouseUp(handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)) {
    if let some(slot) = GPUVIEWS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnGpuViewMouseWheel(handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)) {
    if let some(slot) = GPUVIEWS.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnGpuViewResize(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = GPUVIEWS.get(handle) { slot.onResize = cb }
}

pub fn getGpuViewSurface(handle: i64): WGPUSurface {
    match GPUVIEWS.get(handle) {
        some(s) { s.surface as WGPUSurface }
        none    { 0 as WGPUSurface }
    }
}
pub fn getGpuViewDevice(handle: i64): WGPUDevice {
    if WGPU_READY { WGPU_DEVICE as WGPUDevice } else { 0 as WGPUDevice }
}
pub fn getGpuViewQueue(handle: i64): WGPUQueue {
    if WGPU_READY { WGPU_QUEUE as WGPUQueue } else { 0 as WGPUQueue }
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
