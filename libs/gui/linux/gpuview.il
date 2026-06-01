/// Linux (GTK 4) implementation of `GpuView`. Each GpuView gets
/// its own `GtkDrawingArea` child of the parent window's
/// `GtkFixed`, sized / positioned exactly like Canvas. wgpu
/// renders into an off-screen `BGRA8Unorm` texture per view;
/// every frame the texture is copied into a CPU staging buffer
/// (wgpu MapAsync + DevicePoll) and the resulting bytes are
/// blitted onto the widget's Cairo context. GTK draws the rest
/// of the window normally — title bar, sibling widgets, etc. —
/// so wgpu never paints over the window chrome.
///
/// Why not a direct surface? GTK 4 only hands out a `GdkSurface`
/// for widgets implementing `GtkNative`, which means GtkWindow
/// alone. Going through a render-to-texture + Cairo blit lets
/// regular child widgets host wgpu content; in exchange we eat
/// a GPU→CPU readback per frame and the gamma curve becomes
/// linear-into-sRGB-Cairo (matters for shading-heavy content;
/// fine for solid-colour triangles).
///
/// **NOTE: compile-tested only.** No Linux machine was available
/// to verify the readback path or frame pacing.

use gtk {
    GtkWidget, GtkWindow, GtkFixed, GtkDrawingArea,
    gtk_drawing_area_new, gtk_fixed_put,
    gtk_widget_set_size_request, gtk_widget_set_visible,
    gtk_widget_queue_draw,
    gtk_widget_add_tick_callback, gtk_widget_remove_tick_callback,
    gtk_drawing_area_set_draw_func,
}
use cairo {
    cairo_t, cairo_surface_t,
    cairo_image_surface_create_for_data, cairo_format_stride_for_width,
    cairo_surface_flush, cairo_surface_mark_dirty, cairo_surface_destroy,
    cairo_set_source_surface, cairo_paint,
}
use core { WINDOWS }
use wgpu29 {
    WGPUInstance, WGPUAdapter, WGPUDevice, WGPUQueue, WGPUSurface,
    WGPUTexture, WGPUTextureView, WGPUBuffer,
    WGPUCommandBuffer, WGPURenderPassEncoder,
    WGPUStringView, WGPUChainedStruct, WGPUColor, WGPUExtent3D, WGPUOrigin3D,
    WGPURequestAdapterOptions,
    WGPURequestAdapterCallbackInfo, WGPURequestDeviceCallbackInfo,
    WGPUBufferMapCallbackInfo,
    WGPUTextureDescriptor, WGPUBufferDescriptor,
    WGPUTexelCopyTextureInfo, WGPUTexelCopyBufferInfo, WGPUTexelCopyBufferLayout,
    WGPURenderPassColorAttachment, WGPURenderPassDescriptor,
    TextureFormat, TextureUsage, BufferUsage, TextureDimension, TextureAspect,
    FeatureLevel, PowerPreference, BackendType,
    CallbackMode, LoadOp, StoreOp, LogLevel,
    wgpuSetLogLevel, wgpuSetLogCallback,
    wgpuCreateInstance, wgpuInstanceRequestAdapter, wgpuInstanceProcessEvents,
    wgpuAdapterRequestDevice, wgpuDeviceGetQueue,
    wgpuDeviceCreateTexture, wgpuDeviceCreateBuffer,
    wgpuTextureCreateView,
    wgpuDeviceCreateCommandEncoder, wgpuCommandEncoderBeginRenderPass,
    wgpuRenderPassEncoderEnd, wgpuCommandEncoderFinish, wgpuQueueSubmit,
    wgpuCommandEncoderCopyTextureToBuffer,
    wgpuBufferMapAsync, wgpuBufferGetConstMappedRange, wgpuBufferUnmap,
    wgpuDevicePoll,
    wgpuTextureViewRelease, wgpuTextureRelease, wgpuBufferRelease,
    wgpuRenderPassEncoderRelease, wgpuCommandEncoderRelease,
    wgpuCommandBufferRelease,
}
use libc { calloc, memcpy }
use std.ffi { stringFromCstr }

// Cairo ARGB32 (BGRA byte order on little-endian) — matches
// `WGPUTextureFormat_BGRA8Unorm = 0x1B`.
const CAIRO_FORMAT_ARGB32: i32 = 0
const BGRA8_UNORM:         TextureFormat = TextureFormat.bgra8Unorm

// ─── per-view state ────────────────────────────────────────────

class GpuViewSlot {
    pub area:       GtkDrawingArea
    pub texture:    i64           // WGPUTexture (CopySrc + RenderAttachment)
    pub view:       i64           // WGPUTextureView for the texture
    pub buffer:     i64           // WGPUBuffer (MapRead + CopyDst)
    pub cpuBytes:   i64           // malloc'd staging buffer for Cairo
    pub cairoSurf:  i64           // cairo_surface_t wrapping cpuBytes
    pub width:      i32
    pub height:     i32
    pub wgpuStride: i32           // padded to a multiple of 256
    pub cairoStride: i32          // from cairo_format_stride_for_width
    pub tickId:     u32           // 0 = no tick callback
    pub onRender:   fn(WGPURenderPassEncoder)

    pub init(
        area: GtkDrawingArea,
        tex: i64, view: i64, buf: i64,
        cpuBytes: i64, cairoSurf: i64,
        w: i32, h: i32, wgpuStride: i32, cairoStride: i32
    ) {
        this.area        = area
        this.texture     = tex
        this.view        = view
        this.buffer      = buf
        this.cpuBytes    = cpuBytes
        this.cairoSurf   = cairoSurf
        this.width       = w
        this.height      = h
        this.wgpuStride  = wgpuStride
        this.cairoStride = cairoStride
        this.tickId      = 0
        this.onRender    = noOpRender
    }
}

let GPUVIEWS = new Map<i64, GpuViewSlot>()

fn noOpRender(pass: WGPURenderPassEncoder) {}

// Unique handle counter — GtkDrawingArea pointers double as keys
// but we also want to stash one on a slot.
let NEXT_HANDLE: i64[] = [1]

fn allocHandle(): i64 {
    let h = NEXT_HANDLE[0]
    NEXT_HANDLE[0] = h + 1
    h
}

// ─── app-wide wgpu state ───────────────────────────────────────

let WGPU_INST:    i64[]  = [0]
let WGPU_ADAPTER: i64[]  = [0]
let WGPU_DEVICE:  i64[]  = [0]
let WGPU_QUEUE:   i64[]  = [0]
let WGPU_READY:   bool[] = [false]
let ADAPTER_OUT:  i64[]  = [0]
let DEVICE_OUT:   i64[]  = [0]
// Map-async status hand-off cell. The bare-fn callback can't
// capture, so it stores the status here and the synchronous
// caller reads it after `wgpuDevicePoll(..., wait=1, ...)`.
let MAP_STATUS:   i64[] = [-1]

@extern(C) {
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
    pub fn _gpuOnMap(status: i32, md: i64, ml: i64, u1: i64, u2: i64) {
        MAP_STATUS[0] = status as i64
    }

    /// First-call wgpu bring-up. No surface required — offscreen
    /// textures are device-local, so we just need an adapter +
    /// device. We still ask for `AllowProcessEvents` mode so the
    /// adapter / device callbacks fire inside ProcessEvents.
    fn initWgpuDevice(): bool {
        if WGPU_INST[0] == 0 {
            wgpuSetLogCallback(_gpuOnLog, 0 as *void)
            wgpuSetLogLevel(LogLevel.warn as u32)
            let inst = wgpuCreateInstance(0 as *void)
            WGPU_INST[0] = inst as i64
        }
        let inst = WGPU_INST[0] as WGPUInstance

        let aOpts = WGPURequestAdapterOptions {
            nextInChain:          0 as *void,
            featureLevel:         FeatureLevel.undefined,
            powerPreference:      PowerPreference.undefined,
            forceFallbackAdapter: 0 as u32,            // WGPUBool
            backendType:          BackendType.undefined,
            compatibleSurface:    0 as WGPUSurface,
        }
        let aInfo = WGPURequestAdapterCallbackInfo {
            nextInChain: 0 as *void,
            mode:        CallbackMode.allowProcessEvents,
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

        let dInfo = WGPURequestDeviceCallbackInfo {
            nextInChain: 0 as *void,
            mode:        CallbackMode.allowProcessEvents,
            callback:    _gpuOnDevice,
            userdata1:   0 as *void,
            userdata2:   0 as *void,
        }
        DEVICE_OUT[0] = 0
        wgpuAdapterRequestDevice(ADAPTER_OUT[0] as WGPUAdapter, 0 as *void, dInfo)
        wgpuInstanceProcessEvents(inst)
        if DEVICE_OUT[0] == 0 {
            console.log("GpuView: wgpuAdapterRequestDevice callback never fired")
            return false
        }
        WGPU_DEVICE[0] = DEVICE_OUT[0]
        let queue = wgpuDeviceGetQueue(DEVICE_OUT[0] as WGPUDevice)
        WGPU_QUEUE[0] = queue as i64
        WGPU_READY[0] = true
        true
    }

    /// Round `n` up to the next multiple of 256 — WebGPU requires
    /// `bytesPerRow` on copy ops to be `% 256 == 0`.
    fn alignTo256(n: i32): i32 {
        ((n + 255 as i32) / 256 as i32) * 256 as i32
    }

    /// Render → copy texture → buffer → map → memcpy into the
    /// view's CPU staging bytes → mark cairo dirty → queue redraw.
    /// All wgpu calls stay on the main (GTK) thread.
    fn renderOneFrame(handle: i64) {
        if !WGPU_READY[0] { return }
        let slot = match GPUVIEWS.get(handle) {
            some(s) { s }
            none    { return }
        }
        let device = WGPU_DEVICE[0] as WGPUDevice
        let queue  = WGPU_QUEUE[0] as WGPUQueue

        let enc = wgpuDeviceCreateCommandEncoder(device, 0 as *void)
        let tv  = slot.view as WGPUTextureView
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
        let cb = slot.onRender
        cb(pass)
        wgpuRenderPassEncoderEnd(pass)

        // Encode the texture → staging buffer copy in the same
        // command encoder. depth=1 single-layer 2D copy.
        let src = WGPUTexelCopyTextureInfo {
            texture:  slot.texture as WGPUTexture,
            mipLevel: 0 as u32,
            origin:   WGPUOrigin3D { x: 0 as u32, y: 0 as u32, z: 0 as u32 },
            aspect:   TextureAspect.all,
        }
        let dst = WGPUTexelCopyBufferInfo {
            layout: WGPUTexelCopyBufferLayout {
                offset:       0 as u64,
                bytesPerRow:  slot.wgpuStride as u32,
                rowsPerImage: slot.height as u32,
            },
            buffer: slot.buffer as WGPUBuffer,
        }
        let ext = WGPUExtent3D {
            width:              slot.width  as u32,
            height:             slot.height as u32,
            depthOrArrayLayers: 1 as u32,
        }
        wgpuCommandEncoderCopyTextureToBuffer(enc, &src, &dst, &ext)

        let cmd = wgpuCommandEncoderFinish(enc, 0 as *void)
        let cmds: WGPUCommandBuffer[] = [cmd]
        wgpuQueueSubmit(queue, 1 as u64, cmds)

        // Map for read + spin-poll the device until the callback
        // sets MAP_STATUS. wait=1 makes DevicePoll block until the
        // queue is drained, which is when the map callback fires.
        let mInfo = WGPUBufferMapCallbackInfo {
            nextInChain: 0 as *void,
            mode:        CallbackMode.allowProcessEvents,         // AllowProcessEvents
            callback:    _gpuOnMap,
            userdata1:   0 as *void,
            userdata2:   0 as *void,
        }
        MAP_STATUS[0] = -1
        let bufSize = (slot.wgpuStride as u64) * (slot.height as u64)
        wgpuBufferMapAsync(
            slot.buffer as WGPUBuffer, 1 as u64,    // MapMode_Read
            0 as u64, bufSize, mInfo
        )
        wgpuDevicePoll(device, 1 as u32, 0 as *const u64)
        if MAP_STATUS[0] != 1 {
            // 1 = Success (WGPUMapAsyncStatus_Success). Anything
            // else means the buffer isn't readable this frame —
            // skip the blit so we don't show stale bytes.
            console.log("GpuView: map status=", MAP_STATUS[0])
            wgpuCommandBufferRelease(cmd)
            wgpuRenderPassEncoderRelease(pass)
            wgpuCommandEncoderRelease(enc)
            return
        }

        let mapped = wgpuBufferGetConstMappedRange(
            slot.buffer as WGPUBuffer, 0 as u64, bufSize
        )
        // Detach any snapshot cairo took when the surface was used
        // as a draw source in `gpuDraw` last frame. Without this,
        // `mark_dirty` below aborts on the
        // `! _cairo_surface_has_snapshots` assertion.
        cairo_surface_flush(slot.cairoSurf as cairo_surface_t)
        // Row-by-row copy: wgpu's stride is padded to 256 bytes,
        // cairo's may not match. Identical strides reduce to a
        // single memcpy but the loop costs nothing on top.
        let y = 0 as i32
        while y < slot.height {
            let srcRow = (mapped as i64) + (y as i64) * (slot.wgpuStride as i64)
            let dstRow = slot.cpuBytes + (y as i64) * (slot.cairoStride as i64)
            memcpy(dstRow as *void, srcRow as *const void,
                   (slot.width as u64) * 4 as u64)
            y = y + 1 as i32
        }
        wgpuBufferUnmap(slot.buffer as WGPUBuffer)

        cairo_surface_mark_dirty(slot.cairoSurf as cairo_surface_t)
        gtk_widget_queue_draw(slot.area as GtkWidget)

        wgpuCommandBufferRelease(cmd)
        wgpuRenderPassEncoderRelease(pass)
        wgpuCommandEncoderRelease(enc)
    }

    /// GtkTickCallback — `gboolean(GtkWidget*, GdkFrameClock*, gpointer)`.
    pub fn gpuTick(widget: i64, clock: i64, userData: i64): i32 {
        renderOneFrame(userData)
        1 as i32
    }

    /// `GtkDrawingAreaDrawFunc` — `void(GtkDrawingArea*, cairo_t*,
    /// int width, int height, gpointer)`. Blits the current
    /// staging-buffer image with Cairo. If we haven't produced a
    /// frame yet, the cpu bytes are zeroed → cairo paints solid
    /// transparent black, which is what we want.
    pub fn gpuDraw(area: i64, cr: i64, w: i32, h: i32, userData: i64) {
        if let some(slot) = GPUVIEWS.get(userData) {
            cairo_set_source_surface(
                cr as cairo_t,
                slot.cairoSurf as cairo_surface_t,
                0.0, 0.0
            )
            cairo_paint(cr as cairo_t)
        }
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

    if !WGPU_READY[0] {
        if !initWgpuDevice() { return -1 }
    }
    let device = WGPU_DEVICE[0] as WGPUDevice

    // Allocate the offscreen render texture (BGRA8Unorm). Usage
    // = RenderAttachment | CopySrc.
    let pxW = width  as i32
    let pxH = height as i32
    let texDesc = WGPUTextureDescriptor {
        nextInChain:     0 as *void,
        label:           WGPUStringView { data: 0 as *void, length: 0 as u64 },
        usage:           TextureUsage.renderAttachment | TextureUsage.copySrc,
        dimension:       TextureDimension._2D,
        size:            WGPUExtent3D {
            width:              pxW as u32,
            height:             pxH as u32,
            depthOrArrayLayers: 1 as u32,
        },
        format:          BGRA8_UNORM,
        mipLevelCount:   1 as u32,
        sampleCount:     1 as u32,
        viewFormatCount: 0 as u64,
        viewFormats:     0 as *void,
    }
    let tex = wgpuDeviceCreateTexture(device, &texDesc)
    if (tex as i64) == 0 {
        console.log("GpuView: wgpuDeviceCreateTexture returned null")
        return -1
    }
    let tv = wgpuTextureCreateView(tex, 0 as *void)

    // Staging buffer for CPU readback. bytesPerRow must be %256;
    // wgpuStride captures that, then bufSize = wgpuStride * height.
    let wgpuStride  = alignTo256(pxW * 4 as i32)
    let cairoStride = cairo_format_stride_for_width(CAIRO_FORMAT_ARGB32, pxW)
    let bufSize     = (wgpuStride as u64) * (pxH as u64)
    let bufDesc = WGPUBufferDescriptor {
        nextInChain:      0 as *void,
        label:            WGPUStringView { data: 0 as *void, length: 0 as u64 },
        usage:            BufferUsage.mapRead | BufferUsage.copyDst,
        size:             bufSize,
        mappedAtCreation: 0 as u32,
    }
    let buf = wgpuDeviceCreateBuffer(device, &bufDesc)
    if (buf as i64) == 0 {
        console.log("GpuView: wgpuDeviceCreateBuffer returned null")
        wgpuTextureViewRelease(tv)
        wgpuTextureRelease(tex)
        return -1
    }

    // Persistent CPU buffer the cairo image surface wraps. We hold
    // onto the malloc'd pointer for the lifetime of the GpuView —
    // ilang's u8[] would also work, but the pointer needs to be
    // stable across `gtk_widget_queue_draw` calls and we don't
    // need to expose it back to ilang code anyway.
    let cpuSize = (cairoStride as u64) * (pxH as u64)
    // Zero-init so the first repaint (before any frame lands)
    // shows a transparent surface instead of malloc garbage.
    let cpuBytes = calloc(cpuSize, 1 as u64)
    let cairoSurf = cairo_image_surface_create_for_data(
        cpuBytes, CAIRO_FORMAT_ARGB32, pxW, pxH, cairoStride
    )

    // Hook the drawing area into the parent's GtkFixed at (x, y)
    // with a fixed size. The same widget is the render-tick
    // target later.
    let area = gtk_drawing_area_new() as GtkDrawingArea
    gtk_widget_set_size_request(area as GtkWidget, pxW, pxH)
    gtk_fixed_put(slot.fixed, area as GtkWidget, x, y)
    gtk_widget_set_visible(area as GtkWidget, if visible { 1 as i32 } else { 0 as i32 })

    let handle = allocHandle()
    gtk_drawing_area_set_draw_func(
        area, gpuDraw, handle as *void, 0 as i64
    )

    let view = new GpuViewSlot(
        area, tex as i64, tv as i64, buf as i64,
        cpuBytes as i64, cairoSurf as i64,
        pxW, pxH, wgpuStride, cairoStride
    )
    GPUVIEWS[handle] = view
    handle
}

pub fn setGpuViewFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    // Position update is doable via gtk_fixed_move, but a size
    // change would require reallocating texture + buffer + cairo
    // surface — out of scope for this PoC. Frame is treated as
    // fixed after construction.
}

pub fn setGpuViewVisible(handle: i64, visible: bool) {
    if let some(slot) = GPUVIEWS.get(handle) {
        gtk_widget_set_visible(
            slot.area as GtkWidget,
            if visible { 1 as i32 } else { 0 as i32 }
        )
    }
}

pub fn setOnGpuViewRender(handle: i64, cb: fn(WGPURenderPassEncoder)) {
    if let some(slot) = GPUVIEWS.get(handle) {
        slot.onRender = cb
    }
}

pub fn getGpuViewSurface(handle: i64): WGPUSurface {
    // Linux GpuView is offscreen — there's no WGPUSurface to
    // hand out. Callers shouldn't reach for this on Linux.
    0 as WGPUSurface
}
pub fn getGpuViewDevice(handle: i64): WGPUDevice {
    if WGPU_READY[0] { WGPU_DEVICE[0] as WGPUDevice } else { 0 as WGPUDevice }
}
pub fn getGpuViewQueue(handle: i64): WGPUQueue {
    if WGPU_READY[0] { WGPU_QUEUE[0] as WGPUQueue } else { 0 as WGPUQueue }
}
pub fn getGpuViewFormat(handle: i64): u32 {
    BGRA8_UNORM
}

pub fn startGpuViewLoop(handle: i64) {
    if let some(slot) = GPUVIEWS.get(handle) {
        if slot.tickId != 0 as u32 { return }
        let id = gtk_widget_add_tick_callback(
            slot.area as GtkWidget,
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
        gtk_widget_remove_tick_callback(slot.area as GtkWidget, slot.tickId)
        slot.tickId = 0 as u32
    }
}

pub fn requestGpuViewFrame(handle: i64) {
    renderOneFrame(handle)
}

} // @extern(C)
