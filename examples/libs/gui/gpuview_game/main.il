// Smoke test for the GpuView game-loop API. Subscribes to every
// input signal and prints what arrived; on each render tick, polls
// the snapshot getters and prints the held keys / mouse position.
//
// Run (macOS):
//   DYLD_LIBRARY_PATH=third_party/wgpu/macos-aarch64/lib \
//     cargo run -p ilang -- run examples/libs/gui/gpuview_game/main.il
//
// Move the mouse over the view, click anywhere, press WASD / arrow
// keys — each event prints; the render loop reports `time` / `dt`
// once per second.

use gui

fn run(): i64 {
    let w = new gui.Window()
    w.title  = "ilang gui × GpuView — input demo"
    w.width  = 640.0
    w.height = 480.0

    let v = new gui.GpuView()
    v.x = 0.0; v.y = 0.0
    v.width  = 640.0
    v.height = 480.0
    w.addChild(v)

    if v.device.handle == 0 {
        console.log("GpuView unavailable on this platform")
        return 1
    }

    v.onKeyDown.add(fn(e: gui.KeyEvent) {
        console.log("keyDown  code=", e.code, " shift=", e.shiftKey)
    })
    v.onKeyUp.add(fn(e: gui.KeyEvent) {
        console.log("keyUp    code=", e.code)
    })
    v.onMouseDown.add(fn(e: gui.MouseEvent) {
        console.log("mouseDown btn=", e.button as i64, " at ", e.x, ",", e.y)
    })
    v.onMouseUp.add(fn(e: gui.MouseEvent) {
        console.log("mouseUp   btn=", e.button as i64, " at ", e.x, ",", e.y)
    })
    v.onMouseWheel.add(fn(e: gui.MouseEvent) {
        console.log("wheel   dx=", e.wheelDx, " dy=", e.wheelDy)
    })
    v.onResize.add(fn(e: gui.SizeEvent) {
        console.log("resize   w=", e.width, " h=", e.height)
    })

    // Heartbeat — log time / dt + snapshot once a second.
    let lastReport: f64[] = [0.0]
    v.onRender.add(fn(e: gui.RenderEvent) {
        if e.time - lastReport[0] >= 1.0 {
            lastReport[0] = e.time
            let wHeld = v.isKeyDown(gui.KeyCode.keyW as string)
            console.log("frame  t=", e.time, " dt=", e.dt,
                        "  mouse(", v.mouseX, ",", v.mouseY,
                        ")  btns=", v.mouseButtons as i64,
                        "  W=", wHeld)
        }
    })

    v.startLoop()
    w.show()
    gui.run()
    0
}

run()
