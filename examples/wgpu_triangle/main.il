// PoC — SDL2 window + wgpu-native, drawing a WGSL triangle for a
// fixed number of frames (no event loop / interaction).
//
// Proves the full WebGPU bring-up from ilang on macOS:
//   SDL window → CAMetalLayer → WGPUSurface → request adapter/device →
//   configure → WGSL shader module + render pipeline → per-frame render
//   pass (clear + draw 3 verts) → submit → present.
//
// Setup (once): third_party/wgpu/fetch.sh   (downloads libwgpu_native)
// Run (macOS):
//   DYLD_LIBRARY_PATH=third_party/wgpu/macos-aarch64/lib \
//     cargo run -p ilang -- run examples/wgpu_triangle/main.il

use std.ffi { cstrFromString, readU32, stringFromCstr }
use sdl2 {
    SDL_Window,
    SDL_Init, SDL_Quit, SDL_Delay, SDL_PumpEvents,
    SDL_CreateWindow, SDL_DestroyWindow,
    SDL_ShowWindow, SDL_RaiseWindow, SDL_GetWindowSizeInPixels,
    SDL_Metal_CreateView, SDL_Metal_GetLayer, SDL_Metal_DestroyView,
}
use wgpu {
    WGPUInstance, WGPUAdapter, WGPUDevice, WGPUQueue, WGPUSurface,
    WGPUTexture, WGPUTextureView, WGPUCommandEncoder, WGPUCommandBuffer,
    WGPURenderPassEncoder,
    WGPUStringView, WGPUChainedStruct, WGPUColor,
    WGPURequestAdapterOptions,
    WGPURequestAdapterCallbackInfo, WGPURequestDeviceCallbackInfo,
    WGPUSurfaceSourceMetalLayer, WGPUSurfaceDescriptor,
    WGPUSurfaceConfiguration, WGPUSurfaceTexture, WGPUSurfaceCapabilities,
    WGPURenderPassColorAttachment, WGPURenderPassDescriptor,
    WGPUShaderModuleDescriptor, WGPUShaderSourceWGSL,
    WGPUColorTargetState, WGPUVertexState, WGPUPrimitiveState,
    WGPUMultisampleState, WGPUFragmentState, WGPURenderPipelineDescriptor,
    wgpuSetLogLevel, wgpuSetLogCallback,
    wgpuCreateInstance, wgpuInstanceRequestAdapter, wgpuInstanceProcessEvents,
    wgpuAdapterRequestDevice, wgpuDeviceGetQueue,
    wgpuSurfaceGetCapabilities,
    wgpuInstanceCreateSurface, wgpuSurfaceConfigure,
    wgpuSurfaceGetCurrentTexture, wgpuTextureCreateView, wgpuSurfacePresent,
    wgpuDeviceCreateShaderModule, wgpuDeviceCreateRenderPipeline,
    wgpuRenderPassEncoderSetPipeline, wgpuRenderPassEncoderDraw,
    wgpuDeviceCreateCommandEncoder, wgpuCommandEncoderBeginRenderPass,
    wgpuRenderPassEncoderEnd, wgpuCommandEncoderFinish, wgpuQueueSubmit,
    wgpuTextureViewRelease, wgpuTextureRelease, wgpuRenderPassEncoderRelease,
    wgpuCommandEncoderRelease, wgpuCommandBufferRelease,
    wgpuShaderModuleRelease, wgpuRenderPipelineRelease,
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

let ADAPTER: i64[] = [0]
let DEVICE:  i64[] = [0]

const WIN_W: i32 = 640
const WIN_H: i32 = 480

@extern(C) {
    pub fn onLog(level: i32, msgData: i64, msgLen: i64, ud: i64) {
        console.log("WGPU[", level as i64, "] len=", msgLen,
                    " : ", stringFromCstr(msgData as *const char))
    }
    pub fn onAdapter(s: i32, adapter: i64, md: i64, ml: i64, u1: i64, u2: i64) {
        ADAPTER[0] = adapter
    }
    pub fn onDevice(s: i32, device: i64, md: i64, ml: i64, u1: i64, u2: i64) {
        DEVICE[0] = device
    }

    fn run(): i64 {
        // ── SDL window + Metal layer ──────────────────────────
        if SDL_Init(0x20 as u32) != 0 {            // SDL_INIT_VIDEO
            console.log("FAIL: SDL_Init")
            return 1
        }
        let flags: u32 = (0x20000000 | 0x4) as u32  // METAL | SHOWN
        let win = SDL_CreateWindow(
            cstrFromString("ilang × wgpu — clear"),
            100, 100, WIN_W, WIN_H, flags
        )
        if (win as i64) == 0 { console.log("FAIL: window"); return 2 }
        let view = SDL_Metal_CreateView(win)
        let layer = SDL_Metal_GetLayer(view)
        if (layer as i64) == 0 { console.log("FAIL: metal layer"); return 3 }

        // Bring the window on screen + frontmost and let it settle, so
        // the CAMetalLayer actually vends drawables.
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
        wgpuSetLogLevel(2 as u32)        // Warn
        let inst = wgpuCreateInstance(0 as *void)
        let metalSrc = WGPUSurfaceSourceMetalLayer {
            chain: WGPUChainedStruct { next: 0 as *void, sType: 4 as u32 },
            layer: layer,
        }
        let sdesc = WGPUSurfaceDescriptor {
            nextInChain: (&metalSrc) as *void,
            label: WGPUStringView { data: 0 as *void, length: 0 as u64 },
        }
        let surface = wgpuInstanceCreateSurface(inst, &sdesc)
        if (surface as i64) == 0 { console.log("FAIL: surface"); return 6 }

        // ── adapter (compatible with the surface) / device / queue ─
        let aOpts = WGPURequestAdapterOptions {
            nextInChain: 0 as *void,
            featureLevel: 0 as u32,
            powerPreference: 0 as u32,
            forceFallbackAdapter: 0 as u32,
            backendType: 0 as u32,
            compatibleSurface: surface as *void,
        }
        let aInfo = WGPURequestAdapterCallbackInfo {
            nextInChain: 0 as *void, mode: 2 as u32,
            callback: onAdapter, userdata1: 0 as *void, userdata2: 0 as *void,
        }
        wgpuInstanceRequestAdapter(inst, (&aOpts) as *void, aInfo)
        wgpuInstanceProcessEvents(inst)
        if ADAPTER[0] == 0 { console.log("FAIL: adapter"); return 4 }

        let dInfo = WGPURequestDeviceCallbackInfo {
            nextInChain: 0 as *void, mode: 2 as u32,
            callback: onDevice, userdata1: 0 as *void, userdata2: 0 as *void,
        }
        wgpuAdapterRequestDevice(ADAPTER[0] as WGPUAdapter, 0 as *void, dInfo)
        wgpuInstanceProcessEvents(inst)
        if DEVICE[0] == 0 { console.log("FAIL: device"); return 5 }
        let dev = DEVICE[0] as WGPUDevice
        let queue = wgpuDeviceGetQueue(dev)

        // Query what the surface actually supports rather than
        // hard-coding enum values (the bundled header's values don't
        // all match the dylib). Use the first reported format / alpha
        // / present mode.
        let caps = new WGPUSurfaceCapabilities()
        wgpuSurfaceGetCapabilities(surface, ADAPTER[0] as WGPUAdapter, &caps)
        let fmt = readU32(caps.formats as *const void, 0)
        let alpha = readU32(caps.alphaModes as *const void, 0)
        let present = readU32(caps.presentModes as *const void, 0)
        console.log("caps formatCount=", caps.formatCount as i64,
                    " fmt=", fmt as i64, " alpha=", alpha as i64,
                    " present=", present as i64)

        let cfg = WGPUSurfaceConfiguration {
            nextInChain: 0 as *void,
            device: dev as *void,
            format: fmt,
            usage: 0x10 as u64,         // RenderAttachment
            width: WIN_W as u32,
            height: WIN_H as u32,
            viewFormatCount: 0 as u64,
            viewFormats: 0 as *void,
            alphaMode: alpha,
            presentMode: present,
        }
        wgpuSurfaceConfigure(surface, &cfg)
        console.log("surface configured")

        // ── WGSL shader module + render pipeline ──────────────
        let wgslSrc = WGPUShaderSourceWGSL {
            chain: WGPUChainedStruct { next: 0 as *void, sType: 2 as u32 }, // ShaderSourceWGSL
            code: WGPUStringView {
                data: cstrFromString(WGSL) as *void,
                length: 0xFFFFFFFFFFFFFFFF as u64,   // WGPU_STRLEN → strlen
            },
        }
        let smDesc = WGPUShaderModuleDescriptor {
            nextInChain: (&wgslSrc) as *void,
            label: WGPUStringView { data: 0 as *void, length: 0 as u64 },
        }
        let shader = wgpuDeviceCreateShaderModule(dev, &smDesc)
        if (shader as i64) == 0 { console.log("FAIL: shader"); return 8 }

        let target = WGPUColorTargetState {
            nextInChain: 0 as *void,
            format: fmt,
            blend: 0 as *void,
            writeMask: 0xF as u64,        // All
        }
        let fragState = WGPUFragmentState {
            nextInChain: 0 as *void,
            module: shader as *void,
            entryPoint: WGPUStringView {
                data: cstrFromString("fs_main") as *void, length: 0xFFFFFFFFFFFFFFFF as u64,
            },
            constantCount: 0 as u64,
            constants: 0 as *void,
            targetCount: 1 as u64,
            targets: &target,
        }
        let pipeDesc = WGPURenderPipelineDescriptor {
            nextInChain: 0 as *void,
            label: WGPUStringView { data: 0 as *void, length: 0 as u64 },
            layout: 0 as *void,           // auto layout
            vertex: WGPUVertexState {
                nextInChain: 0 as *void,
                module: shader as *void,
                entryPoint: WGPUStringView {
                    data: cstrFromString("vs_main") as *void, length: 0xFFFFFFFFFFFFFFFF as u64,
                },
                constantCount: 0 as u64,
                constants: 0 as *void,
                bufferCount: 0 as u64,
                buffers: 0 as *void,
            },
            primitive: WGPUPrimitiveState {
                nextInChain: 0 as *void,
                topology: 4 as u32,        // TriangleList
                stripIndexFormat: 0 as u32,
                frontFace: 1 as u32,       // CCW
                cullMode: 1 as u32,        // None
                unclippedDepth: 0 as u32,
            },
            depthStencil: 0 as *void,
            multisample: WGPUMultisampleState {
                nextInChain: 0 as *void,
                count: 1 as u32,
                mask: 0xFFFFFFFF as u32,
                alphaToCoverageEnabled: 0 as u32,
            },
            fragment: &fragState,
        }
        let pipeline = wgpuDeviceCreateRenderPipeline(dev, &pipeDesc)
        if (pipeline as i64) == 0 { console.log("FAIL: pipeline"); return 9 }
        console.log("pipeline ready — drawing triangle")

        // ── render N frames, clearing to a blue-ish colour ────
        // The CAMetalLayer only yields a drawable once the window is
        // actually composited on screen, which is timing/occlusion
        // dependent. So each frame we pump events and retry
        // getCurrentTexture until a texture is available (bounded).
        let frame = 0
        let misses = 0
        while frame < 120 {
            SDL_PumpEvents()
            let st = new WGPUSurfaceTexture()
            wgpuSurfaceGetCurrentTexture(surface, &st)
            if (st.texture as i64) == 0 {
                misses = misses + 1
                if misses > 600 {
                    console.log("FAIL: no drawable after", misses, "tries")
                    return 7
                }
                SDL_Delay(10 as u32)
                continue
            }
            if frame == 0 { console.log("first drawable acquired") }
            let tv = wgpuTextureCreateView(st.texture as WGPUTexture, 0 as *void)
            let enc = wgpuDeviceCreateCommandEncoder(dev, 0 as *void)

            let att = WGPURenderPassColorAttachment {
                nextInChain: 0 as *void,
                view: tv as *void,
                depthSlice: 0xFFFFFFFF as u32,    // UNDEFINED
                resolveTarget: 0 as *void,
                loadOp: 2 as u32,                 // Clear
                storeOp: 1 as u32,                // Store
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
            let pass = wgpuCommandEncoderBeginRenderPass(enc, &rp)
            wgpuRenderPassEncoderSetPipeline(pass, pipeline)
            wgpuRenderPassEncoderDraw(pass, 3 as u32, 1 as u32, 0 as u32, 0 as u32)
            wgpuRenderPassEncoderEnd(pass)
            let cmd = wgpuCommandEncoderFinish(enc, 0 as *void)
            let cmds: i64[] = [cmd as i64]
            wgpuQueueSubmit(queue, 1 as u64, cmds)
            wgpuSurfacePresent(surface)

            wgpuCommandBufferRelease(cmd)
            wgpuRenderPassEncoderRelease(pass)
            wgpuCommandEncoderRelease(enc)
            wgpuTextureViewRelease(tv)
            wgpuTextureRelease(st.texture as WGPUTexture)
            SDL_Delay(8 as u32)
            frame = frame + 1
        }

        console.log("PASS: presented 120 frames")
        wgpuRenderPipelineRelease(pipeline)
        wgpuShaderModuleRelease(shader)
        SDL_Metal_DestroyView(view)
        SDL_DestroyWindow(win)
        SDL_Quit()
        0
    }
}

run()
