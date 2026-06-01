// PoC — SDL2 window + wgpu-native, drawing a WGSL triangle for a
// fixed number of frames (no event loop / interaction).
//
// Proves the full WebGPU bring-up from ilang:
//   SDL window → native surface source → WGPUSurface → request
//   adapter/device → configure → WGSL shader module + render pipeline →
//   per-frame render pass (clear + draw 3 verts) → submit → present.
//
// The surface source is picked from `os.platform`: CAMetalLayer on
// macOS, HWND on Windows, and X11 / Wayland on Linux (chosen at
// runtime from SDL's reported subsystem).
//
// Setup (once): third_party/wgpu/fetch.sh   (downloads libwgpu_native)
// Run (macOS):
//   DYLD_LIBRARY_PATH=third_party/wgpu/macos-aarch64/lib \
//     cargo run -p ilang -- run examples/wgpu_triangle/main.il
// Run (Linux):
//   LD_LIBRARY_PATH=third_party/wgpu/linux-x86_64/lib \
//     cargo run -p ilang -- run examples/wgpu_triangle/main.il
// Run (Windows): put wgpu_native.dll on PATH (or next to the entry),
//   then: cargo run -p ilang -- run examples/wgpu_triangle/main.il

use std.ffi { cstrFromString, stringFromCstr }
use std.os as os
use sdl2 {
    SDL_Window, SysWMinfo,
    SDL_Init, SDL_Quit, SDL_Delay, SDL_PumpEvents,
    SDL_CreateWindow, SDL_DestroyWindow, SDL_GetWindowWMInfo,
    SDL_ShowWindow, SDL_RaiseWindow, SDL_GetWindowSizeInPixels,
    SDL_Metal_CreateView, SDL_Metal_GetLayer, SDL_Metal_DestroyView,
}
use wgpu29 {
    Instance, Queue, Surface, SurfaceConfig, RenderPass,
    RenderPipelineOptions,
    WGPUInstance, WGPUSurface, WGPUDevice, WGPUColor,
    WGPUTextureView, WGPUCommandEncoder,
    WGPUStringView, WGPUChainedStruct,
    WGPUSurfaceSourceMetalLayer, WGPUSurfaceSourceWindowsHWND,
    WGPUSurfaceSourceXlibWindow, WGPUSurfaceSourceWaylandSurface,
    WGPUSurfaceDescriptor,
    WGPURenderPassColorAttachment, WGPURenderPassDescriptor,
    requestAdapter,
    wgpuSetLogLevel, wgpuSetLogCallback,
    wgpuCreateInstance, wgpuInstanceCreateSurface, wgpuDeviceGetQueue,
    wgpuCommandEncoderBeginRenderPass,
    SType, LoadOp, StoreOp, LogLevel,
    PowerPreference, BackendType,
}

// WGSL: hardcoded triangle (no vertex buffers — positions indexed by
// @builtin(vertex_index)). One module, vs_main + fs_main entry points.
const WGSL: string = `
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(
        vec2<f32>( 0.0,  0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5)
    );
    return vec4<f32>(p[i], 0.0, 1.0);
}
@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.6, 0.1, 1.0);
}
`

const WIN_W: i32 = 640
const WIN_H: i32 = 480

@extern(C) {
    pub fn onLog(level: i32, msgData: i64, msgLen: i64, ud: i64) {
        console.log("WGPU[", level as i64, "] len=", msgLen,
                    " : ", stringFromCstr(msgData as *const char))
    }

    // The OS-specific SurfaceSource chains off the descriptor's
    // nextInChain. The chained-struct setup needs raw `*void`
    // arithmetic + `wgpuInstanceCreateSurface`, so it stays in
    // @extern(C); the rest of `run()` consumes only the friendly
    // wgpu29 wrappers.
    fn makeSurfaceForPlat(
        instH: i64, winH: i64, plat: string, metalViewOut: i64[]
    ): i64 {
        let inst = instH as WGPUInstance
        let win = winH as *SDL_Window
        if plat == "macos" {
            let view = SDL_Metal_CreateView(win)
            metalViewOut[0] = view as i64
            let layer = SDL_Metal_GetLayer(view)
            if (layer as i64) == 0 { console.log("FAIL: metal layer") }
            let metalSrc = WGPUSurfaceSourceMetalLayer {
                chain: WGPUChainedStruct { next: 0 as *void, sType: SType.surfaceSourceMetalLayer },
                layer: layer,
            }
            let sdesc = WGPUSurfaceDescriptor {
                nextInChain: (&metalSrc) as *void,
                label: WGPUStringView { data: 0 as *void, length: 0 as u64 },
            }
            return wgpuInstanceCreateSurface(inst, &sdesc) as i64
        }
        if plat == "windows" {
            // Pull HWND + HINSTANCE out of SDL's SysWMinfo. Seed the
            // version triple so SDL agrees to populate the union.
            let info = new SysWMinfo()
            info.versionMajor = 2 as u8
            info.versionMinor = 0 as u8
            info.versionPatch = 0 as u8
            SDL_GetWindowWMInfo(win, &info)
            let winSrc = WGPUSurfaceSourceWindowsHWND {
                chain: WGPUChainedStruct { next: 0 as *void, sType: SType.surfaceSourceWindowsHWND },
                hinstance: info.handle3 as *void,    // HINSTANCE
                hwnd: info.handle1 as *void,         // HWND
            }
            let sdesc = WGPUSurfaceDescriptor {
                nextInChain: (&winSrc) as *void,
                label: WGPUStringView { data: 0 as *void, length: 0 as u64 },
            }
            return wgpuInstanceCreateSurface(inst, &sdesc) as i64
        }
        if plat == "linux" {
            // SDL picks X11 or Wayland at runtime (driver / env), so
            // branch on the recorded subsystem rather than guessing.
            let info = new SysWMinfo()
            info.versionMajor = 2 as u8
            info.versionMinor = 0 as u8
            info.versionPatch = 0 as u8
            SDL_GetWindowWMInfo(win, &info)
            if info.subsystem == 6 as u32 {          // SDL_SYSWM_WAYLAND
                let wlSrc = WGPUSurfaceSourceWaylandSurface {
                    chain: WGPUChainedStruct { next: 0 as *void, sType: SType.surfaceSourceWaylandSurface },
                    display: info.handle1 as *void,  // wl_display*
                    surface: info.handle2 as *void,  // wl_surface*
                }
                let sdesc = WGPUSurfaceDescriptor {
                    nextInChain: (&wlSrc) as *void,
                    label: WGPUStringView { data: 0 as *void, length: 0 as u64 },
                }
                return wgpuInstanceCreateSurface(inst, &sdesc) as i64
            }
            // SDL_SYSWM_X11 (2)
            let x11Src = WGPUSurfaceSourceXlibWindow {
                chain: WGPUChainedStruct { next: 0 as *void, sType: SType.surfaceSourceXlibWindow },
                display: info.handle1 as *void,      // Display*
                window: info.handle2 as u64,         // X Window XID
            }
            let sdesc = WGPUSurfaceDescriptor {
                nextInChain: (&x11Src) as *void,
                label: WGPUStringView { data: 0 as *void, length: 0 as u64 },
            }
            return wgpuInstanceCreateSurface(inst, &sdesc) as i64
        }
        console.log("FAIL: unsupported platform ", plat)
        0 as i64
    }

fn run(): i64 {
    let plat = os.platform
    // ── SDL window ────────────────────────────────────────
    if SDL_Init(0x20 as u32) != 0 {            // SDL_INIT_VIDEO
        console.log("FAIL: SDL_Init")
        return 1
    }
    // macOS needs the Metal flag so SDL_Metal_CreateView works;
    // the other backends (Win32 HWND, X11, Wayland) just want a
    // shown window.
    let flags: u32 = if plat == "macos" {
        (0x20000000 | 0x4) as u32              // METAL | SHOWN
    } else {
        0x4 as u32                             // SHOWN
    }
    let win = SDL_CreateWindow(
        cstrFromString("ilang × wgpu — triangle"),
        100, 100, WIN_W, WIN_H, flags
    )
    if (win as i64) == 0 { console.log("FAIL: window"); return 2 }

    // Bring the window on screen + frontmost and let it settle, so
    // the native surface actually vends drawables.
    SDL_ShowWindow(win)
    SDL_RaiseWindow(win)
    let settle = 0
    while settle < 30 { SDL_PumpEvents(); SDL_Delay(10 as u32); settle = settle + 1 }
    let pw: i32[] = [0]
    let ph: i32[] = [0]
    SDL_GetWindowSizeInPixels(win, pw, ph)
    console.log("drawable px=", pw[0] as i64, "x", ph[0] as i64)

    // ── wgpu instance + surface (surface first, so the adapter
    //    can be requested compatible with it) ───────────────
    wgpuSetLogCallback(onLog, 0 as *void)
    wgpuSetLogLevel(LogLevel.warn as u32)
    let inst = new Instance(wgpuCreateInstance(0 as *void))

    let metalViewOut: i64[] = [0]
    let surfaceHandle = makeSurfaceForPlat(
        inst.handle, win as i64, plat, metalViewOut
    )
    if surfaceHandle == 0 { console.log("FAIL: surface"); return 6 }
    let surface = new Surface(surfaceHandle as WGPUSurface)

    // ── adapter (compatible with the surface) / device / queue ─
    let adapter = requestAdapter(
        inst, surface.handle, PowerPreference.undefined, BackendType.undefined
    )
    if adapter.handle == 0 { console.log("FAIL: adapter"); return 4 }
    let dev = adapter.requestDevice(inst)
    if dev.handle == 0 { console.log("FAIL: device"); return 5 }
    // No friendly `Device.getQueue()` yet — wrap the raw handle
    // by hand. See the rewrite report's gap list.
    let queue = new Queue(wgpuDeviceGetQueue(dev.handle as WGPUDevice))

    // Query what the surface actually supports rather than
    // hard-coding enum values (the bundled header's values don't
    // all match the dylib). Use the first reported format / alpha
    // / present mode.
    let caps = surface.getCapabilities(adapter)
    let fmt = caps.formats[0]
    let alpha = caps.alphaModes[0]
    let present = caps.presentModes[0]
    console.log("caps formatCount=", caps.formats.length,
                " fmt=", fmt as i64, " alpha=", alpha as i64,
                " present=", present as i64,
                " usages=", caps.usages as i64)

    let cfg = new SurfaceConfig(dev, fmt, WIN_W as i64, WIN_H as i64)
    cfg.alphaMode = alpha
    cfg.presentMode = present
    surface.configure(cfg)
    console.log("surface configured")

    // ── WGSL shader module + render pipeline ──────────────
    let shader = dev.createWgslShader(WGSL)
    if shader.handle == 0 { console.log("FAIL: shader"); return 8 }

    let pipeline = dev.createRenderPipeline(
        new RenderPipelineOptions(shader, shader, fmt)
    )
    if pipeline.handle == 0 { console.log("FAIL: pipeline"); return 9 }
    console.log("pipeline ready — drawing triangle")

    // ── render N frames, clearing to a blue-ish colour ────
    // The CAMetalLayer only yields a drawable once the window is
    // actually composited on screen, which is timing/occlusion
    // dependent. So each frame we pump events and retry
    // getCurrentFrame until a texture is available (bounded).
    let frame = 0
    let misses = 0
    while frame < 120 {
        SDL_PumpEvents()
        let st = surface.getCurrentFrame()
        if st.texture.handle == 0 {
            misses = misses + 1
            if misses > 600 {
                console.log("FAIL: no drawable after", misses, "tries")
                return 7
            }
            SDL_Delay(10 as u32)
            continue
        }
        if frame == 0 { console.log("first drawable acquired") }
        let tv = st.texture.createView()
        let enc = dev.createCommandEncoder()

        // NOTE: bypassing CommandEncoder.beginRenderPass /
        // ColorAttachmentOption — passing a WGPUColor by-value
        // through a class init's positional arg trips the
        // codegen ABI (it splats the 4×f64 struct into 4 slots
        // while the caller supplies 1). See the rewrite report.
        let att = WGPURenderPassColorAttachment {
            nextInChain: 0 as *void,
            view: tv.handle as WGPUTextureView,
            depthSlice: 0xFFFFFFFF as u32,
            resolveTarget: 0 as WGPUTextureView,
            loadOp: LoadOp.clear,
            storeOp: StoreOp.store,
            clearValue: WGPUColor { r: 0.10, g: 0.20, b: 0.55, a: 1.0 },
        }
        let rp = WGPURenderPassDescriptor {
            nextInChain: 0 as *void,
            label: WGPUStringView { data: 0 as *void, length: 0 as u64 },
            colorAttachmentCount: 1 as u64,
            colorAttachments: &att,
            depthStencilAttachment: 0 as *void,
            occlusionQuerySet: 0 as *void,
            timestampWrites: 0 as *void,
        }
        let passH = wgpuCommandEncoderBeginRenderPass(
            enc.handle as WGPUCommandEncoder, &rp
        )
        let pass = new RenderPass(passH)
        pass.setPipeline(pipeline)
        pass.draw(3, 1, 0, 0)
        pass.end()
        let cmd = enc.finish()
        queue.submitOne(cmd)
        surface.present()

        cmd.release()
        enc.release()
        tv.release()
        st.texture.release()
        SDL_Delay(8 as u32)
        frame = frame + 1
    }

    console.log("PASS: presented 120 frames")
    pipeline.release()
    shader.release()
    if metalViewOut[0] != 0 { SDL_Metal_DestroyView(metalViewOut[0] as *void) }
    SDL_DestroyWindow(win)
    SDL_Quit()
    0
}
}

run()
