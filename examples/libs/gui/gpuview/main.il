// PoC — `examples/wgpu_triangle` ported to `libs/gui`. Same WGSL
// triangle, but the surface is owned by a `gui.GpuView` inside a
// `gui.Window` (no SDL).
//
// Demonstrates the friendly wgpu wrapper layer
// (`wgpu.Device` / `wgpu.RenderPass` / …) — no `@extern(C) { ... }`
// block needed in user code.
//
// Setup (once): third_party/wgpu/fetch.sh
// Run (Windows, PowerShell):
//   $env:PATH = "$PWD\third_party\wgpu\windows-x86_64\lib;$env:PATH"
//   cargo run -p ilang -- run examples/libs/gui/gpuview/main.il
// Run (macOS):
//   DYLD_LIBRARY_PATH=third_party/wgpu/macos-aarch64/lib \
//     cargo run -p ilang -- run examples/libs/gui/gpuview/main.il
// Run (Linux, X11 or Wayland — needs GTK 4 + libwgpu_native):
//   LD_LIBRARY_PATH=third_party/wgpu/linux-x86_64/lib \
//     cargo run -p ilang -- run examples/libs/gui/gpuview/main.il

use gui
use gui.gpuview { GpuView, RenderEvent }
use wgpu29 { Pipeline, RenderPass, RenderPipelineOptions }

// WGSL: positions hardcoded in the vertex shader, indexed by
// @builtin(vertex_index). No vertex buffers needed.
@embed("triangle.wgsl") const WGSL: string

fn run(): i64 {
    let w = new gui.Window()
    w.title  = "ilang gui × wgpu — triangle"
    w.width  = 640.0
    w.height = 480.0

    let v = new GpuView()
    // Dock = fill → the view tracks the window: addChild runs the
    // first layout pass, and every later `onResize` re-runs it so
    // the surface always covers the client area.
    v.dock = gui.Dock.fill
    w.addChild(v)            // attachTo runs → v.device / v.format valid

    if v.device.handle == 0 {
        console.log("GpuView unavailable on this platform")
        return 1
    }

    let shader   = v.device.createWgslShader(WGSL)
    let pipeline = v.device.createRenderPipeline(
        new RenderPipelineOptions(shader, shader, v.format)
    )
    if pipeline.handle == 0 {
        console.log("FAIL: createRenderPipeline returned null")
        return 2
    }

    v.onRender.add(fn(e: RenderEvent) {
        let pass = new RenderPass(e.pass)
        pass.setPipeline(pipeline)
        pass.draw(3, 1, 0, 0)
    })
    v.startLoop()

    w.show()
    gui.run()
    0
}

run()
