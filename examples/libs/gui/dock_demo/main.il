// libs/gui — dock layout demo. A top label, a bottom status bar,
// and a centre GpuView that fills the remaining area. Resize the
// window to see the layout track: the top / bottom strips stay
// pinned to their edges, the GpuView always covers what's left.
//
// Run on macOS:
//   DYLD_LIBRARY_PATH=third_party/wgpu/macos-aarch64/lib \
//     cargo run -p ilang -- run examples/libs/gui/dock_demo/main.il

use gui
use gui.gpuview { GpuView, RenderEvent }
use wgpu29 { RenderPass, RenderPipelineOptions }

@embed("triangle.wgsl") const WGSL: string

fn run(): i64 {
    let w = new gui.Window()
    w.title  = "ilang gui × dock"
    w.width  = 640.0
    w.height = 480.0

    // Top header — pinned to the top edge, full width, 36 px tall.
    let header = new gui.Label()
    header.text   = "Dock demo — resize me"
    header.height = 36.0
    header.dock   = gui.Dock.top
    header.margin = new gui.Margin(8.0)
    w.addChild(header)

    // Bottom status bar — pinned to the bottom, full width, 24 px.
    let status = new gui.Label()
    status.text   = "centre: GpuView · top: Label · bottom: Label"
    status.height = 24.0
    status.dock   = gui.Dock.bottom
    status.margin = new gui.Margin(4.0)
    w.addChild(status)

    // Centre fill — a GpuView that always takes the remaining area.
    let v = new GpuView()
    v.dock   = gui.Dock.fill
    v.margin = new gui.Margin(4.0)
    w.addChild(v)

    if v.device.handle != 0 {
        let shader   = v.device.createWgslShader(WGSL)
        let pipeline = v.device.createRenderPipeline(
            new RenderPipelineOptions(shader, shader, v.format)
        )
        if pipeline.handle != 0 {
            v.onRender.add(fn(e: RenderEvent) {
                let pass = new RenderPass(e.pass)
                pass.setPipeline(pipeline)
                pass.draw(3, 1, 0, 0)
            })
            v.startLoop()
        }
    }

    w.show()
    gui.run()
    0
}

run()
