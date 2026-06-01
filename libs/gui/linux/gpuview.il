/// Linux (GTK 4) implementation of `GpuView`. Mirrors cocoa /
/// win32, with two important caveats:
///
/// 1. **One GpuView per GtkWindow.** GTK 4 only hands out a
///    `GdkSurface` for widgets that implement `GtkNative`
///    (GtkWindow, GtkPopover, …). Regular child widgets share
///    their toplevel's surface. So `createNativeGpuView` ignores
///    the requested x / y / width / height and configures wgpu
///    against the parent window's full surface. Multi-view per
///    window would need rendering to a texture + Cairo compose,
///    which is out of scope here.
///
/// 2. **X11 / Wayland chosen at runtime.** Linux's GTK can run
///    on either; we read `gdk_display_get_name(display)` and use
///    a "wayland-" prefix as the discriminator (X11 names look
///    like `:0`, `:0.0`, `localhost:0`). On Wayland we wrap the
///    `wl_surface`, on X11 the `XWindow` XID.
///
/// Frame ticking uses `gtk_widget_add_tick_callback`, which fires
/// once per frame from the surface's `GdkFrameClock` — GTK's
/// CADisplayLink / DwmFlush equivalent, already vsync-aligned.
///
/// **NOTE: This file has only been compiled, not run.** No Linux
/// machine was available at the time it was written. Treat every
/// path as untested until somebody actually launches the example
/// on a real X11 or Wayland session.

use gtk {
    GtkWidget, GtkWindow,
    GdkDisplay, GdkSurface, GtkNative,
    gtk_widget_realize, gtk_widget_get_realized,
    gtk_widget_get_native, gtk_native_get_surface,
    gtk_widget_get_display,
    gtk_widget_get_width, gtk_widget_get_height,
    gtk_widget_add_tick_callback, gtk_widget_remove_tick_callback,
    gdk_display_get_name,
    gdk_x11_surface_get_xid, gdk_x11_display_get_xdisplay,
    gdk_wayland_surface_get_wl_surface, gdk_wayland_display_get_wl_display,
    gdk_surface_get_width, gdk_surface_get_height,
}
use core { WINDOWS }
use wgpu {
    WGPUInstance, WGPUAdapter, WGPUDevice, WGPUQueue, WGPUSurface,
    WGPUTexture, WGPUTextureView, WGPUCommandBuffer, WGPURenderPassEncoder,
    WGPUStringView, WGPUChainedStruct, WGPUColor,
    WGPURequestAdapterOptions,
    WGPURequestAdapterCallbackInfo, WGPURequestDeviceCallbackInfo,
    WGPUSurfaceSourceXlibWindow, WGPUSurfaceSourceWaylandSurface,
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

// ─── per-view state ────────────────────────────────────────────
//
// `surface` kept as raw `i64` so the slot can live outside an
// `@extern(C)` block. `tickId` is the value returned by
// `gtk_widget_add_tick_callback`; 0 = not currently ticking.

class GpuViewSlot {
    pub win:      GtkWindow
    pub surface:  i64           // WGPUSurface, 0 = unset
    pub format:   u32
    pub width:    i32           // last configured surface size, px
    pub height:   i32
    pub tickId:   u32           // 0 = no tick callback
    pub onRender: fn(WGPURenderPassEncoder)

    pub init(win: GtkWindow, surface: i64, format: u32, w: i32, h: i32) {
        this.win      = win
        this.surface  = surface
        this.format   = format
        this.width    = w
        this.height   = h
        this.tickId   = 0
        this.onRender = noOpRender
    }
}

let GPUVIEWS = new Map<i64, GpuViewSlot>()

fn noOpRender(pass: WGPURenderPassEncoder) {}

// ─── app-wide state ────────────────────────────────────────────

let WGPU_INST:    i64[]  = [0]
let WGPU_ADAPTER: i64[]  = [0]
let WGPU_DEVICE:  i64[]  = [0]
let WGPU_QUEUE:   i64[]  = [0]
let WGPU_READY:   bool[] = [false]
let ADAPTER_OUT:  i64[]  = [0]
let DEVICE_OUT:   i64[]  = [0]

@extern(C) {
    // ─── wgpu log + adapter/device request callbacks ──────────
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

    /// Build the X11 / Wayland surface source for `gdkSurface` and
    /// create the WGPUSurface. Returns the surface pointer as
    /// `i64` (0 on failure). `isWayland` decides which source
    /// chain shape to emit; both backends ride the same wgpu
    /// instance.
    fn makeSurfaceFor(
        gdkSurface: GdkSurface, display: GdkDisplay, isWayland: bool
    ): i64 {
        if WGPU_INST[0] == 0 {
            wgpuSetLogCallback(_gpuOnLog, 0 as *void)
            wgpuSetLogLevel(2 as u32)         // Warn
            let inst = wgpuCreateInstance(0 as *void)
            WGPU_INST[0] = inst as i64
        }
        let inst = WGPU_INST[0] as WGPUInstance

        if isWayland {
            let wlDisplay = gdk_wayland_display_get_wl_display(display)
            let wlSurface = gdk_wayland_surface_get_wl_surface(gdkSurface)
            if (wlDisplay as i64) == 0 || (wlSurface as i64) == 0 {
                console.log("GpuView: gdk_wayland_*_get_wl_* returned null")
                return 0
            }
            let wlSrc = WGPUSurfaceSourceWaylandSurface {
                chain:   WGPUChainedStruct { next: 0 as *void, sType: 7 as u32 },
                display: wlDisplay,
                surface: wlSurface,
            }
            let sdesc = WGPUSurfaceDescriptor {
                nextInChain: (&wlSrc) as *void,
                label:       WGPUStringView { data: 0 as *void, length: 0 as u64 },
            }
            (wgpuInstanceCreateSurface(inst, &sdesc)) as i64
        } else {
            let xDisplay = gdk_x11_display_get_xdisplay(display)
            let xid      = gdk_x11_surface_get_xid(gdkSurface)
            if (xDisplay as i64) == 0 || xid == 0 as u64 {
                console.log("GpuView: gdk_x11_*_get_* returned null")
                return 0
            }
            let x11Src = WGPUSurfaceSourceXlibWindow {
                chain:   WGPUChainedStruct { next: 0 as *void, sType: 6 as u32 },
                display: xDisplay,
                window:  xid,
            }
            let sdesc = WGPUSurfaceDescriptor {
                nextInChain: (&x11Src) as *void,
                label:       WGPUStringView { data: 0 as *void, length: 0 as u64 },
            }
            (wgpuInstanceCreateSurface(inst, &sdesc)) as i64
        }
    }

    /// Pick formats[0] / alphaModes[0] / presentModes[0] from the
    /// surface caps and configure it for `pxW × pxH`. Returns the
    /// chosen format (0 on failure).
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

        // GTK can change the surface dimensions out from under
        // us (window resize); reconfigure on size mismatch before
        // acquiring the next drawable.
        let widget = slot.win as GtkWidget
        let curW = gtk_widget_get_width(widget)
        let curH = gtk_widget_get_height(widget)
        if curW > 0 as i32 && curH > 0 as i32
           && (curW != slot.width || curH != slot.height) {
            slot.width  = curW
            slot.height = curH
            configureSurfaceFor(slot.surface, curW as u32, curH as u32)
        }

        let surface = slot.surface as WGPUSurface
        let st = new WGPUSurfaceTexture()
        wgpuSurfaceGetCurrentTexture(surface, &st)
        // Null on the first few ticks before the swap chain is
        // composited, or any time the window is occluded /
        // minimised. Silent skip — next tick retries.
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

    /// GtkTickCallback — `gboolean(GtkWidget*, GdkFrameClock*, gpointer)`.
    /// `userData` is the GpuView handle we registered with.
    /// Returning 1 (G_SOURCE_CONTINUE) keeps the callback firing.
    pub fn gpuTick(widget: i64, clock: i64, userData: i64): i32 {
        renderOneFrame(userData)
        1 as i32
    }

// ─── public bridge API ─────────────────────────────────────────

pub fn createNativeGpuView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64, visible: bool
): i64 {
    let slot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let win = slot.win
    let widget = win as GtkWidget

    // Force surface allocation so `gtk_native_get_surface` returns
    // non-null. Realize is a no-op once the widget is already
    // realized, so we don't guard it.
    gtk_widget_realize(widget)

    let native = gtk_widget_get_native(widget)
    if (native as i64) == 0 {
        console.log("GpuView: gtk_widget_get_native returned null")
        return -1
    }
    let gdkSurface = gtk_native_get_surface(native)
    if (gdkSurface as i64) == 0 {
        console.log("GpuView: gtk_native_get_surface returned null (window not realized?)")
        return -1
    }

    let display = gtk_widget_get_display(widget)
    let dname = stringFromCstr(gdk_display_get_name(display))
    let isWayland = dname.startsWith("wayland")
    console.log("GpuView: display='", dname, "' → ",
                if isWayland { "Wayland" } else { "X11" })

    let surfacePtr = makeSurfaceFor(gdkSurface, display, isWayland)
    if surfacePtr == 0 {
        console.log("GpuView: wgpuInstanceCreateSurface failed")
        return -1
    }
    if !WGPU_READY[0] {
        if !initWgpuDevice(surfacePtr) { return -1 }
    }

    // Configure for the surface's current pixel size; the public
    // `width` / `height` args are advisory only on Linux because
    // the wgpu surface always covers the whole window.
    let pxW = gtk_widget_get_width(widget)
    let pxH = gtk_widget_get_height(widget)
    if pxW == 0 as i32 || pxH == 0 as i32 {
        // Not laid out yet — fall back to the requested size so
        // configuring doesn't fail. renderOneFrame will redo this
        // once GTK has a real allocation.
        let fmt = configureSurfaceFor(surfacePtr, width as u32, height as u32)
        if fmt == (0 as u32) { return -1 }
        let view = new GpuViewSlot(win, surfacePtr, fmt, width as i32, height as i32)
        GPUVIEWS[windowHandle] = view
    } else {
        let fmt = configureSurfaceFor(surfacePtr, pxW as u32, pxH as u32)
        if fmt == (0 as u32) { return -1 }
        let view = new GpuViewSlot(win, surfacePtr, fmt, pxW, pxH)
        GPUVIEWS[windowHandle] = view
    }
    // Re-use the parent window handle as the view handle — Linux
    // is 1 GpuView per window anyway.
    windowHandle
}

pub fn setGpuViewFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    // Advisory only on Linux; the surface covers the whole window.
    // Resizes are picked up inside `renderOneFrame` from the live
    // widget allocation.
}

pub fn setGpuViewVisible(handle: i64, visible: bool) {
    // No separate widget to hide — the GpuView IS the window's
    // content. Hiding would mean hiding the whole window, which
    // the user can do via `Window.isVisible` directly.
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
        if slot.tickId != 0 as u32 { return }    // already running
        let id = gtk_widget_add_tick_callback(
            slot.win as GtkWidget,
            gpuTick,
            handle as *void,
            0 as i64
        )
        slot.tickId = id
    }
}

pub fn stopGpuViewLoop(handle: i64) {
    if let some(slot) = GPUVIEWS.get(handle) {
        if slot.tickId == 0 as u32 { return }
        gtk_widget_remove_tick_callback(slot.win as GtkWidget, slot.tickId)
        slot.tickId = 0 as u32
    }
}

pub fn requestGpuViewFrame(handle: i64) {
    renderOneFrame(handle)
}

} // @extern(C)
