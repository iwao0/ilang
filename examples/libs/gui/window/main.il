// 🪟 libs/gui — window with the full event surface. Opens a
// 480×320 window and logs every event the library exposes:
// close / resize / keyDown / keyUp / mouseMove / mouseDown /
// mouseUp.
//
// Run on macOS:
//   ilang run examples/libs/gui/window/main.il
//
// `[deps] @target` in `libs/gui/core/ilang.toml` swaps the backing
// implementation between `libs/gui/cocoa` and `libs/gui/win32`,
// so this file is identical on both hosts.

use gui

let w = new gui.Window("ilang × gui", 480.0, 320.0)

w.onClose.add(fn(e: gui.CloseEvent) {
    let _ = e
    console.log("close")
})

w.onResize.add(fn(e: gui.SizeEvent) {
    console.log("resize", e.width, e.height)
})

w.onKeyDown.add(fn(e: gui.KeyEvent) {
    console.log("keyDown", e.code,
                "ctrl=", e.ctrlKey, "shift=", e.shiftKey,
                "alt=", e.altKey, "meta=", e.metaKey)
})
w.onKeyUp.add(fn(e: gui.KeyEvent) {
    console.log("keyUp", e.code)
})

w.onMouseMove.add(fn(e: gui.MouseEvent) {
    console.log("mouseMove", e.x, e.y)
})
w.onMouseDown.add(fn(e: gui.MouseEvent) {
    console.log("mouseDown", e.x, e.y, "button=", e.button)
})
w.onMouseUp.add(fn(e: gui.MouseEvent) {
    console.log("mouseUp", e.x, e.y, "button=", e.button)
})

gui.run()
