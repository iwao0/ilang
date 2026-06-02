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

use std.ffi { stringFromBytes }
use std.os as os
use sdl2 {
    Window, WindowFlag, InitFlag, SubSystem,
    init, quit, delay, pumpEvents,
    metalCreateView, metalGetLayer, metalDestroyView,
}
use wgpu29 {
    Instance, Surface, SurfaceConfig,
    ColorAttachmentOption, RenderPassOptions, RenderPipelineOptions,
    WGPUSurface, WGPUColor,
    createInstance, requestAdapter, setLogLevel, setLogCallback,
    createMetalSurface, createHwndSurface,
    createXlibSurface, createWaylandSurface,
    LoadOp, StoreOp, LogLevel,
    PowerPreference, BackendType,
}

// WGSL: hardcoded triangle (no vertex buffers — positions indexed by
// @builtin(vertex_index)). One module, vs_main + fs_main entry points.
@embed("triangle.wgsl") const WGSL: string

const WIN_W: i32 = 640
const WIN_H: i32 = 480

@extern(C) {
    pub fn onLog(level: i32, msgData: i64, msgLen: i64, ud: i64) {
        // WGPUStringView is (data, length) — length is the real
        // count, NOT a NUL-terminated cstring. Use stringFromBytes
        // (not stringFromCstr) to avoid reading past the buffer.
        console.log("WGPU[", level as i64, "] len=", msgLen,
                    " : ", stringFromBytes(msgData as *const void, msgLen))
    }
}

// Build a `Surface` for `win` on the current platform. `metalViewOut`
// receives the macOS `SDL_MetalView` cookie so `run()` can release it
// after the loop; non-macOS leaves it at `0`. Returns a `Surface`
// whose `handle == 0` on failure (unsupported platform, surface
// creation rejected by wgpu, …).
fn makeSurfaceForPlat(
    inst: Instance, win: Window, plat: string, metalViewOut: i64[]
): Surface {
    if plat == "macos" {
        let view = metalCreateView(win)
        metalViewOut[0] = view
        let layer = metalGetLayer(view)
        if layer == 0 { console.log("FAIL: metal layer") }
        return createMetalSurface(inst, layer)
    }
    if plat == "windows" {
        // Read HWND + HINSTANCE in one wmInfo() call instead of
        // hitting the per-handle accessors three times.
        let info = win.wmInfo()
        return createHwndSurface(inst, info.handle3, info.handle1)
    }
    if plat == "linux" {
        // SDL picks X11 or Wayland at runtime (driver / env), so
        // branch on the recorded subsystem rather than guessing.
        let info = win.wmInfo()
        if info.subsystem == SubSystem.wayland as u32 {
            return createWaylandSurface(inst, info.handle1, info.handle2)
        }
        return createXlibSurface(inst, info.handle1, info.handle2 as u64)
    }
    console.log("FAIL: unsupported platform ", plat)
    new Surface(0 as WGPUSurface)
}

fn run(): i64 {
    let plat = os.platform
    // ── SDL window ────────────────────────────────────────
    if init(InitFlag.video) != 0 {
        console.log("FAIL: SDL_Init")
        return 1
    }
    // macOS needs the Metal flag so SDL_Metal_CreateView works;
    // the other backends (Win32 HWND, X11, Wayland) just want a
    // shown window.
    let flags: WindowFlag = if plat == "macos" {
        WindowFlag.metal | WindowFlag.shown
    } else {
        WindowFlag.shown
    }
    let win = new Window(
        "ilang × wgpu — triangle", 100, 100, WIN_W, WIN_H, flags
    )
    if win.handle == 0 { console.log("FAIL: window"); return 2 }

    // Bring the window on screen + frontmost and let it settle, so
    // the native surface actually vends drawables.
    win.show()
    win.raise()
    let settle = 0
    while settle < 30 { pumpEvents(); delay(10 as u32); settle = settle + 1 }
    let px = win.sizeInPixels
    console.log("drawable px=", px.w as i64, "x", px.h as i64)

    // ── wgpu instance + surface (surface first, so the adapter
    //    can be requested compatible with it) ───────────────
    setLogCallback(onLog, 0)
    setLogLevel(LogLevel.warn)
    let inst = createInstance()

    let metalViewOut: i64[] = [0]
    let surface = makeSurfaceForPlat(inst, win, plat, metalViewOut)
    if surface.handle == 0 { console.log("FAIL: surface"); return 6 }

    // ── adapter (compatible with the surface) / device / queue ─
    let adapter = requestAdapter(
        inst, surface.handle, PowerPreference.undefined, BackendType.undefined
    )
    if adapter.handle == 0 { console.log("FAIL: adapter"); return 4 }
    let dev = adapter.requestDevice(inst)
    if dev.handle == 0 { console.log("FAIL: device"); return 5 }
    let queue = dev.getQueue()

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
        pumpEvents()
        let st = surface.getCurrentFrame()
        if st.texture.handle == 0 {
            misses = misses + 1
            if misses > 600 {
                console.log("FAIL: no drawable after", misses, "tries")
                return 7
            }
            delay(10 as u32)
            continue
        }
        if frame == 0 { console.log("first drawable acquired") }
        let tv = st.texture.createView()
        let enc = dev.createCommandEncoder()

        let att = new ColorAttachmentOption(
            tv, LoadOp.clear, StoreOp.store,
            WGPUColor { r: 0.10, g: 0.20, b: 0.55, a: 1.0 }
        )
        let pass = enc.beginRenderPass(new RenderPassOptions([att]))
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
        delay(8 as u32)
        frame = frame + 1
    }

    console.log("PASS: presented 120 frames")
    pipeline.release()
    shader.release()
    metalDestroyView(metalViewOut[0])
    // `win` is ARC-managed: deinit calls `SDL_DestroyWindow`.
    quit()
    0
}

run()
