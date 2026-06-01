// PoC — `examples/wgpu_triangle` ported to `libs/gui`. Same WGSL
// triangle, but the surface is owned by a `gui.GpuView` inside a
// `gui.Window` (no SDL). The view's CADisplayLink loop fires
// `onRender` every display refresh; the user closes the window
// to exit.
//
// Setup (once): third_party/wgpu/fetch.sh
// Run (macOS):
//   DYLD_LIBRARY_PATH=third_party/wgpu/macos-aarch64/lib \
//     cargo run -p ilang -- run examples/libs/gui/gpuview/main.il

use gui
use wgpu {
    WGPUDevice, WGPURenderPassEncoder, WGPURenderPipeline,
    WGPUShaderModule,
    WGPUStringView, WGPUChainedStruct,
    WGPUShaderModuleDescriptor, WGPUShaderSourceWGSL,
    WGPUColorTargetState, WGPUVertexState, WGPUPrimitiveState,
    WGPUMultisampleState, WGPUFragmentState, WGPURenderPipelineDescriptor,
    wgpuDeviceCreateShaderModule, wgpuDeviceCreateRenderPipeline,
    wgpuRenderPassEncoderSetPipeline, wgpuRenderPassEncoderDraw,
}
use std.ffi { cstrFromString }

// WGSL: positions hardcoded in the vertex shader, indexed by
// @builtin(vertex_index). No vertex buffers needed.
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

@extern(C) {
    /// Build the WGSL shader module + render pipeline for the
    /// supplied device / target format. Caller owns the returned
    /// pipeline; in this PoC it lives until process exit.
    fn buildPipeline(device: WGPUDevice, format: u32): WGPURenderPipeline {
        let wgslSrc = WGPUShaderSourceWGSL {
            chain: WGPUChainedStruct { next: 0 as *void, sType: 2 as u32 },
            code: WGPUStringView {
                data:   cstrFromString(WGSL) as *void,
                length: 0xFFFFFFFFFFFFFFFF as u64,    // WGPU_STRLEN → strlen
            },
        }
        let smDesc = WGPUShaderModuleDescriptor {
            nextInChain: (&wgslSrc) as *void,
            label:       WGPUStringView { data: 0 as *void, length: 0 as u64 },
        }
        let shader = wgpuDeviceCreateShaderModule(device, &smDesc)

        let target = WGPUColorTargetState {
            nextInChain: 0 as *void,
            format:      format,
            blend:       0 as *void,
            writeMask:   0xF as u64,    // All channels
        }
        let fragState = WGPUFragmentState {
            nextInChain:   0 as *void,
            module:        shader,
            entryPoint:    WGPUStringView {
                data:   cstrFromString("fs_main") as *void,
                length: 0xFFFFFFFFFFFFFFFF as u64,
            },
            constantCount: 0 as u64,
            constants:     0 as *void,
            targetCount:   1 as u64,
            targets:       &target,
        }
        let pipeDesc = WGPURenderPipelineDescriptor {
            nextInChain: 0 as *void,
            label:       WGPUStringView { data: 0 as *void, length: 0 as u64 },
            layout:      0 as *void,
            vertex: WGPUVertexState {
                nextInChain:   0 as *void,
                module:        shader,
                entryPoint:    WGPUStringView {
                    data:   cstrFromString("vs_main") as *void,
                    length: 0xFFFFFFFFFFFFFFFF as u64,
                },
                constantCount: 0 as u64,
                constants:     0 as *void,
                bufferCount:   0 as u64,
                buffers:       0 as *void,
            },
            primitive: WGPUPrimitiveState {
                nextInChain:    0 as *void,
                topology:       4 as u32,    // TriangleList
                stripIndexFormat: 0 as u32,
                frontFace:      1 as u32,    // CCW
                cullMode:       1 as u32,    // None
                unclippedDepth: 0 as u32,
            },
            depthStencil: 0 as *void,
            multisample: WGPUMultisampleState {
                nextInChain:           0 as *void,
                count:                 1 as u32,
                mask:                  0xFFFFFFFF as u32,
                alphaToCoverageEnabled: 0 as u32,
            },
            fragment: &fragState,
        }
        wgpuDeviceCreateRenderPipeline(device, &pipeDesc)
    }

    fn drawTriangle(pass: WGPURenderPassEncoder, pipeline: WGPURenderPipeline) {
        wgpuRenderPassEncoderSetPipeline(pass, pipeline)
        wgpuRenderPassEncoderDraw(pass, 3 as u32, 1 as u32, 0 as u32, 0 as u32)
    }

    fn run(): i64 {
        let w = new gui.Window()
        w.title  = "ilang gui × wgpu — triangle"
        w.width  = 640.0
        w.height = 480.0

        let v = new gui.GpuView()
        v.x = 0.0; v.y = 0.0
        v.width  = 640.0
        v.height = 480.0
        w.addChild(v)            // attachTo runs → v.device / v.format valid

        if (v.device as i64) == 0 {
            console.log("GpuView unavailable on this platform")
            return 1
        }

        let pipeline = buildPipeline(v.device, v.format)
        if (pipeline as i64) == 0 {
            console.log("FAIL: wgpuDeviceCreateRenderPipeline returned null")
            return 2
        }
        v.onRender.add(fn(e: gui.RenderEvent) {
            drawTriangle(e.pass, pipeline)
        })
        v.startLoop()

        w.show()
        gui.run()
        0
    }
}

run()
