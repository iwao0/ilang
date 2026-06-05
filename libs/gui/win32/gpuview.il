/// Win32 implementation of `GpuView`. The view is a child HWND
/// (class `IlangGuiGpuView`) hosted inside the parent gui-core
/// window; its WGPUSurface comes from `WGPUSurfaceSourceWindowsHWND`.
///
/// Frame ticking is VSync-locked via a dedicated render thread
/// that loops on `DwmFlush()` (reliable under desktop compositing,
/// unlike `IDXGIOutput::WaitForVBlank`) and posts a custom
/// `GPU_REDRAW` to the main thread. All wgpu calls stay on the
/// main (GUI) thread — the render thread only touches the
/// free-threaded `DwmFlush` and `PostMessageW`.
///
/// A 1-cell `pending` flag coalesces posts: the render thread
/// posts at most one GPU_REDRAW until the wnd-proc has drained
/// the previous one. That way, when the main thread is busy in
/// a modal move / drag / resize loop, the render thread spins
/// silently on `DwmFlush` instead of flooding the message queue
/// and starving real input.
///
/// Lifecycle:
///   gui-core `attachTo` → `createNativeGpuView`
///     → lazy-init app-wide WGPUInstance / Adapter / Device
///       (first call only; adapter requested with
///       `compatibleSurface` = this view's surface)
///     → register window class + create child HWND
///     → create + configure WGPUSurface
///     → return HWND-as-i64
///   user → `setOnGpuViewRender` to stash the render callback
///   user → `startGpuViewLoop` spawns the render thread; each
///     `DwmFlush` wake-up posts `GPU_REDRAW` to the wnd-proc,
///     which clears `pending` before calling `renderOneFrame`.

use windows {
    HWND, HINSTANCE, HMENU, HANDLE,
    HICON, HCURSOR, HBRUSH, RECT,
    TOOLINFOW, TRACKMOUSEEVENT, TrackMouseFlags,
    TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    SendMessageW, TrackMouseEvent, SetFocus, EnableWindow,
    GetModuleHandleA,
    WNDCLASSEXA, ClassStyle, WindowStyle, WindowMessage, ShowCmd,
    RegisterClassExW, CreateWindowExW, DefWindowProcW,
    DestroyWindow, ShowWindow, MoveWindow,
    GetClientRect, PostMessageW,
    CreateThread, WaitForSingleObject, CloseHandle,
    VirtualKey, GetKeyState,
}
use dwmapi { DwmFlush }
use core { WINDOWS, windowHwnd }
use util { vkToString }
use std.time { monotonicNs }
use wgpu29 {
    WGPUInstance, WGPUAdapter, WGPUDevice, WGPUQueue, WGPUSurface,
    WGPUTexture, WGPUTextureView, WGPUCommandBuffer, WGPURenderPassEncoder,
    WGPUStringView, WGPUChainedStruct, WGPUColor,
    WGPURequestAdapterOptions,
    WGPURequestAdapterCallbackInfo, WGPURequestDeviceCallbackInfo,
    WGPUSurfaceSourceWindowsHWND,
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

// ─── per-view state ────────────────────────────────────────────
//
// `surface` / `thread` kept as raw `i64` so the slot can live
// outside `@extern(C)`. `stop` and `pending` are 1-element
// `bool[]` so the render thread and main thread share a stable
// backing cell — ilang arrays are heap-allocated, so the slot's
// reference and the thread's captured reference both alias the
// same storage.

class GpuViewSlot {
    pub hwnd:     HWND
    pub winHandle: i64
    pub tooltipHwnd: HWND
    pub mouseTracking: bool
    pub surface:  i64           // WGPUSurface, 0 = unset
    pub format:   u32
    pub width:    f64
    pub height:   f64
    pub thread:   i64           // HANDLE, 0 = no thread
    pub stop:     bool[]        // [0] = render thread should stop
    pub pending:  bool[]        // [0] = a GPU_REDRAW is in flight
    pub onRender: fn(WGPURenderPassEncoder, f64, f64)
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
    pub startNs:  i64
    pub lastNs:   i64

    pub init(hwnd: HWND, winHandle: i64, surface: i64, format: u32, w: f64, h: f64) {
        this.hwnd         = hwnd
        this.winHandle    = winHandle
        this.tooltipHwnd  = 0 as HWND
        this.mouseTracking = false
        this.surface      = surface
        this.format       = format
        this.width        = w
        this.height       = h
        this.thread       = 0
        this.stop         = [false]
        this.pending      = [false]
        this.onRender     = gpuNoOpRender
        this.onFocus      = gpuNoOp0
        this.onBlur       = gpuNoOp0
        this.onKeyDown    = gpuNoOpKey
        this.onKeyUp      = gpuNoOpKey
        this.onMouseEnter = gpuNoOpEnterLeave
        this.onMouseLeave = gpuNoOpEnterLeave
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
fn gpuNoOp0() {}
fn gpuNoOpEnterLeave(x: f64, y: f64) {}
fn gpuNoOpKey(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn gpuNoOpMove(x: f64, y: f64, b: u32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn gpuNoOpButton(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn gpuNoOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn gpuNoOpResize(w: f64, h: f64) {}

// ─── app-wide state ────────────────────────────────────────────

let WGPU_INST:    i64  = 0
let WGPU_ADAPTER: i64  = 0
let WGPU_DEVICE:  i64  = 0
let WGPU_QUEUE:   i64  = 0
let WGPU_READY:   bool = false
let ADAPTER_OUT:  i64  = 0
let DEVICE_OUT:   i64  = 0

/// `WM_USER + 1` — VBlank wake-up posted by the render thread.
const GPU_REDRAW: u32 = 0x0401
/// Class registered on first GpuView creation.
let CLASS_INITED: bool = false

@extern(C) {
    // ─── wgpu log + adapter/device request callbacks ──────────
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

    fn initWgpuDevice(surfacePtr: i64): bool {
        let surface = surfacePtr as WGPUSurface
        let inst = WGPU_INST as WGPUInstance

        let aOpts = WGPURequestAdapterOptions {
            nextInChain:          0 as *void,
            featureLevel:         FeatureLevel.undefined,
            powerPreference:      PowerPreference.undefined,
            forceFallbackAdapter: 0 as u32,            // WGPUBool
            backendType:          BackendType.undefined,
            compatibleSurface:    surface,
        }
        let aInfo = WGPURequestAdapterCallbackInfo {
            nextInChain: 0 as *void,
            mode:        CallbackMode.allowProcessEvents,        // AllowProcessEvents
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

    /// Build the HWND surface chain and create the WGPUSurface.
    /// Returns the surface pointer as `i64` (0 on failure).
    fn makeSurfaceForHwnd(hwnd: HWND, hInst: HINSTANCE): i64 {
        if WGPU_INST == 0 {
            wgpuSetLogCallback(_gpuOnLog, 0 as *void)
            wgpuSetLogLevel(LogLevel.warn as u32)
            let inst = wgpuCreateInstance(0 as *void)
            WGPU_INST = inst as i64
        }
        let winSrc = WGPUSurfaceSourceWindowsHWND {
            chain:     WGPUChainedStruct { next: 0 as *void, sType: SType.surfaceSourceWindowsHWND },
            hinstance: hInst as *void,
            hwnd:      hwnd as *void,
        }
        let sdesc = WGPUSurfaceDescriptor {
            nextInChain: (&winSrc) as *void,
            label:       WGPUStringView { data: 0 as *void, length: 0 as u64 },
        }
        let surf = wgpuInstanceCreateSurface(WGPU_INST as WGPUInstance, &sdesc)
        surf as i64
    }

    /// Pick formats[0] / alphaModes[0] / presentModes[0] from the
    /// surface caps and configure it for `pxW × pxH`. Returns the
    /// chosen format (0 on failure).
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

    fn renderOneFrame(hwndI64: i64) {
        if !WGPU_READY { return }
        let slot = match GPUVIEWS.get(hwndI64) {
            some(s) { s }
            none    { return }
        }
        let surface = slot.surface as WGPUSurface
        let st = new WGPUSurfaceTexture()
        wgpuSurfaceGetCurrentTexture(surface, &st)
        // Null on the first few ticks while the swap chain warms
        // up, or any time the window is occluded / minimised.
        // Silent skip — next VBlank retries.
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

    /// Window procedure. Handles `GPU_REDRAW` (the vsync wake-up
    /// from the render thread) and `WM_SIZE` (surface reconfigure
    /// on resize). Everything else falls through to `DefWindowProcW`.
    pub fn gpuViewWndProc(hwnd: i64, msg: u32, wParam: u64, lParam: i64): i64 {
        if msg == GPU_REDRAW {
            renderOneFrame(hwnd)
            // Clear `pending` AFTER the render so the render thread
            // can't queue another GPU_REDRAW while we're still here.
            // GetMessage prioritises posted messages over real
            // input — clearing too early lets a fresh GPU_REDRAW
            // re-enter ahead of every WM_MOUSEMOVE / WM_NCLBUTTON*
            // and the title bar effectively becomes undraggable.
            if let some(slot) = GPUVIEWS.get(hwnd) { slot.pending[0] = false }
            return 0
        }
        // lParam packs (x, y) for mouse messages — extract once.
        let lo = (lParam & 0xFFFF) as f64
        let hi = ((lParam >> 16) & 0xFFFF) as f64
        match msg as WindowMessage {
            WindowMessage.size {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    // lParam low word = new width, high word = height
                    let w = lo
                    let h = hi
                    if w > 0.0 && h > 0.0 && (w != slot.width || h != slot.height) {
                        slot.width  = w
                        slot.height = h
                        if WGPU_READY {
                            configureSurfaceFor(slot.surface, w as u32, h as u32)
                        }
                        slot.onResize(w, h)
                    }
                }
                return 0
            }
            WindowMessage.eraseBkgnd {
                // wgpu paints every pixel — skip GDI erase to
                // avoid flicker before the first present lands.
                return 1
            }
            WindowMessage.setFocus {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    slot.onFocus()
                }
                return 0
            }
            WindowMessage.killFocus {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    slot.onBlur()
                }
                return 0
            }
            WindowMessage.keydown {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    slot.onKeyDown(vkToString(wParam as u32 as VirtualKey),
                       gpuModCtrl(), gpuModShift(), gpuModAlt(), gpuModMeta())
                }
                return 0
            }
            WindowMessage.keyup {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    slot.onKeyUp(vkToString(wParam as u32 as VirtualKey),
                       gpuModCtrl(), gpuModShift(), gpuModAlt(), gpuModMeta())
                }
                return 0
            }
            WindowMessage.mouseMove {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    if !slot.mouseTracking {
                        let tme = TRACKMOUSEEVENT {
                            cbSize:      24 as u32,
                            dwFlags:     TrackMouseFlags.leave as u32,
                            hwndTrack:   hwnd as HWND,
                            dwHoverTime: 0 as u32,
                        }
                        TrackMouseEvent(&tme)
                        slot.mouseTracking = true
                        let enter = slot.onMouseEnter
                        enter(lo, hi)
                    }
                    // wParam holds the MK_* bitfield (bit0=L, 1=R, 4=middle).
                    // Pack into the (L=bit0, R=bit1, M=bit2) layout the
                    // public class exposes.
                    let mk = wParam as u32
                    let buttons =
                        (if (mk & 0x1 as u32) != 0 as u32 { 0x1 as u32 } else { 0 as u32 }) |
                        (if (mk & 0x2 as u32) != 0 as u32 { 0x2 as u32 } else { 0 as u32 }) |
                        (if (mk & 0x10 as u32) != 0 as u32 { 0x4 as u32 } else { 0 as u32 })
                    slot.onMouseMove(lo, hi, buttons,
                       gpuModCtrl(), gpuModShift(), gpuModAlt(), gpuModMeta())
                }
                return 0
            }
            WindowMessage.mouseLeave {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    slot.mouseTracking = false
                    slot.onMouseLeave(lo, hi)
                }
                return 0
            }
            WindowMessage.lbuttondown {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    SetFocus(slot.hwnd)
                    slot.onMouseDown(lo, hi, 0 as i32, gpuModCtrl(), gpuModShift(), gpuModAlt(), gpuModMeta())
                }
                return 0
            }
            WindowMessage.lbuttonup {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    slot.onMouseUp(lo, hi, 0 as i32, gpuModCtrl(), gpuModShift(), gpuModAlt(), gpuModMeta())
                }
                return 0
            }
            WindowMessage.rButtonDown {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    let ck = gpuModCtrl()
                    let sk = gpuModShift()
                    let ak = gpuModAlt()
                    let mk = gpuModMeta()
                    slot.onMouseDown(lo, hi, 2 as i32, ck, sk, ak, mk)
                    let ctx = slot.onContextMenu
                    ctx(lo, hi, 2 as i32, ck, sk, ak, mk)
                }
                return 0
            }
            WindowMessage.rButtonUp {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    slot.onMouseUp(lo, hi, 2 as i32, gpuModCtrl(), gpuModShift(), gpuModAlt(), gpuModMeta())
                }
                return 0
            }
            WindowMessage.mButtonDown {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    slot.onMouseDown(lo, hi, 1 as i32, gpuModCtrl(), gpuModShift(), gpuModAlt(), gpuModMeta())
                }
                return 0
            }
            WindowMessage.mButtonUp {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    slot.onMouseUp(lo, hi, 1 as i32, gpuModCtrl(), gpuModShift(), gpuModAlt(), gpuModMeta())
                }
                return 0
            }
            WindowMessage.mouseWheel {
                if let some(slot) = GPUVIEWS.get(hwnd) {
                    // HIWORD(wParam) holds a signed 16-bit delta in
                    // multiples of WHEEL_DELTA (120). Normalise to
                    // ±1 per detent so the cross-platform shape is
                    // stable (matches NSEvent.deltaY conventions).
                    let raw = ((wParam >> 16) & 0xFFFF) as i32
                    let signed = if raw >= 0x8000 as i32 { raw - 0x10000 as i32 } else { raw }
                    let dy = (signed as f64) / 120.0
                    // For WM_MOUSEWHEEL, lParam coords are screen-space
                    // (not client). For the first batch we ship the
                    // raw values — most game code treats wheel as a
                    // discrete delta and ignores cursor pos anyway.
                    // TODO: ScreenToClient(hwnd, &pt) for a faithful
                    // local point.
                    slot.onMouseWheel(0.0, dy, lo, hi,
                       gpuModCtrl(), gpuModShift(), gpuModAlt(), gpuModMeta())
                }
                return 0
            }
            _ {}
        }
        DefWindowProcW(hwnd as HWND, msg, wParam, lParam)
    }

    /// Modifier-key polling for the GpuView wnd-proc. Duplicates
    /// the helpers in `wnd_proc.il` (which live inside that file's
    /// `@extern(C)` block as non-pub fns) so the GpuView class can
    /// stand alone.
    fn gpuKeyDown(vk: i32): bool {
        (GetKeyState(vk) as i32 & 0x8000) != 0
    }
    fn gpuModShift(): bool { gpuKeyDown(VirtualKey.shift as i32) }
    fn gpuModCtrl():  bool { gpuKeyDown(VirtualKey.control as i32) }
    fn gpuModAlt():   bool { gpuKeyDown(VirtualKey.alt as i32) }
    fn gpuModMeta():  bool { gpuKeyDown(VirtualKey.lWin as i32) || gpuKeyDown(VirtualKey.rWin as i32) }

    /// Render thread body. Blocks on `DwmFlush` (~vsync, reliable
    /// under DWM unlike `IDXGIOutput::WaitForVBlank`), then posts
    /// at most one `GPU_REDRAW` to the wnd-proc — the `pending`
    /// flag is set here and cleared on the main thread inside
    /// `gpuViewWndProc`, coalescing posts whenever the main thread
    /// is busy (e.g. inside a modal move loop).
    pub fn gpuRenderThread(param: i64): u32 {
        let hwnd = param
        let slot = match GPUVIEWS.get(hwnd) {
            some(s) { s }
            none    { return 1 }
        }
        let stop    = slot.stop
        let pending = slot.pending
        while !stop[0] {
            DwmFlush()
            if !pending[0] {
                pending[0] = true
                PostMessageW(hwnd as HWND, GPU_REDRAW, 0 as u64, 0 as i64)
            }
        }
        0 as u32
    }

fn registerGpuViewClass() {
    if CLASS_INITED { return }
    CLASS_INITED = true

    let hInstance = GetModuleHandleA(0 as *const char)
    let classNameW = "IlangGuiGpuView".encodeUtf16()
    let classNamePtr: *const u16 = classNameW
    let wc = WNDCLASSEXA {
        cbSize:        80,
        style:         (ClassStyle.hredraw | ClassStyle.vredraw) as u32,
        lpfnWndProc:   gpuViewWndProc,
        cbClsExtra:    0,
        cbWndExtra:    0,
        hInstance:     hInstance as HINSTANCE,
        hIcon:         0 as HICON,
        hCursor:       0 as HCURSOR,
        hbrBackground: 0 as HBRUSH,
        lpszMenuName:  0 as *const char,
        lpszClassName: (classNamePtr as i64) as *const char,
        hIconSm:       0 as HICON,
    }
    RegisterClassExW(&wc)
}

// ─── public bridge API ─────────────────────────────────────────

pub fn createNativeGpuView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64, enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    registerGpuViewClass()

    let classW = "IlangGuiGpuView".encodeUtf16()
    let titleW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let disabledBits: u32 = if !enabled { WindowStyle.disabled as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32) | visibleBits | disabledBits
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW,
        titleW,
        style,
        x as i32, y as i32, width as i32, height as i32,
        parent,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )

    let surfacePtr = makeSurfaceForHwnd(hwnd, hInstance as HINSTANCE)
    if surfacePtr == 0 {
        console.log("GpuView: wgpuInstanceCreateSurface failed")
        DestroyWindow(hwnd)
        return -1
    }
    if !WGPU_READY {
        if !initWgpuDevice(surfacePtr) {
            DestroyWindow(hwnd)
            return -1
        }
    }
    let fmt = configureSurfaceFor(surfacePtr, width as u32, height as u32)
    if fmt == (0 as u32) {
        DestroyWindow(hwnd)
        return -1
    }

    let slot = new GpuViewSlot(hwnd, windowHandle, surfacePtr, fmt, width, height)
    let handle = hwnd as i64
    GPUVIEWS[handle] = slot
    handle
}

pub fn setGpuViewFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = GPUVIEWS.get(handle) {
        MoveWindow(slot.hwnd, x as i32, y as i32, w as i32, h as i32, 1)
        // WM_SIZE inside MoveWindow will reconfigure the surface.
        // Update the cached dimensions here too for the
        // public-class getters.
        slot.width  = w
        slot.height = h
    }
}

pub fn setGpuViewVisible(handle: i64, visible: bool) {
    if let some(slot) = GPUVIEWS.get(handle) {
        let cmd = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}

pub fn setGpuViewEnabled(handle: i64, enabled: bool) {
    if let some(slot) = GPUVIEWS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}

pub fn setGpuViewTooltip(handle: i64, text: string) {
    if let some(slot) = GPUVIEWS.get(handle) {
        if text == "" {
            if slot.tooltipHwnd as i64 != 0 as i64 {
                let info = TOOLINFOW {
                    cbSize:     72 as u32,
                    uFlags:     TTF_IDISHWND,
                    hwnd:       slot.winHandle as HWND,
                    uId:        slot.hwnd as u64,
                    rect:       RECT { left: 0 as i32, top: 0 as i32, right: 0 as i32, bottom: 0 as i32 },
                    hinst:      0 as HINSTANCE,
                    lpszText:   0 as *const u16,
                    lParam:     0 as i64,
                    lpReserved: 0 as *void,
                }
                let p: *const TOOLINFOW = &info
                SendMessageW(slot.tooltipHwnd, TTM_DELTOOLW, 0 as u64, p as i64)
            }
            return
        }
        let textW = text.encodeUtf16()
        let info = TOOLINFOW {
            cbSize:     72 as u32,
            uFlags:     TTF_IDISHWND | TTF_SUBCLASS,
            hwnd:       slot.winHandle as HWND,
            uId:        slot.hwnd as u64,
            rect:       RECT { left: 0 as i32, top: 0 as i32, right: 0 as i32, bottom: 0 as i32 },
            hinst:      0 as HINSTANCE,
            lpszText:   textW,
            lParam:     0 as i64,
            lpReserved: 0 as *void,
        }
        let p: *const TOOLINFOW = &info
        if slot.tooltipHwnd as i64 == 0 as i64 {
            let classW = "tooltips_class32".encodeUtf16()
            let hInstance = GetModuleHandleA(0 as *const char)
            let tt = CreateWindowExW(
                0,
                classW,
                0 as *const u16,
                (WindowStyle.popup as u32) | TTS_ALWAYSTIP,
                0, 0, 0, 0,
                slot.winHandle as HWND,
                0 as HMENU,
                hInstance as HINSTANCE,
                0 as *void
            )
            slot.tooltipHwnd = tt
            SendMessageW(tt, TTM_ADDTOOLW, 0 as u64, p as i64)
        } else {
            SendMessageW(slot.tooltipHwnd, TTM_UPDATETIPTEXTW, 0 as u64, p as i64)
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
    if let some(slot) = GPUVIEWS.get(handle) { SetFocus(slot.hwnd) }
}
pub fn blurGpuView(handle: i64) {
    if let some(slot) = GPUVIEWS.get(handle) {
        SetFocus(0 as HWND)
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
        if slot.thread != 0 { return }    // already running
        slot.stop[0]    = false
        slot.pending[0] = false
        let t = CreateThread(
            0 as *void,
            0 as u64,
            gpuRenderThread,
            handle,
            0 as u32,
            0 as *u32
        )
        slot.thread = t as i64
    }
}

pub fn stopGpuViewLoop(handle: i64) {
    if let some(slot) = GPUVIEWS.get(handle) {
        if slot.thread == 0 { return }
        slot.stop[0] = true
        // Render thread exits after at most one more DwmFlush
        // (~16ms). 250ms is plenty of slack.
        let t = slot.thread as HANDLE
        WaitForSingleObject(t, 250 as u32)
        CloseHandle(t)
        slot.thread = 0
    }
}

pub fn requestGpuViewFrame(handle: i64) {
    renderOneFrame(handle)
}

} // @extern(C)
