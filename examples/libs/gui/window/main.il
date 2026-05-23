// 🪟 libs/gui — window with the full event surface. Opens a
// 480×320 window and logs every event the library exposes:
// close / resize / keyDown / keyUp / mouseMove / mouseDown /
// mouseUp.
//
// Run on macOS:
//   ilang run examples/libs/gui/window/main.il
//
// `[deps] @target` in `libs/gui/ilang.toml` swaps the backing
// implementation between `libs/gui/cocoa` and `libs/gui/win32`,
// so this file is identical on both hosts.

use gui

let w = new gui.Window()
w.title  = "ilang × gui"
w.width  = 480.0
w.height = 320.0

w.onClose.add(fn(e: gui.CloseEvent) {
    console.log("close")
})

w.onResize.add(fn(e: gui.SizeEvent) {
    console.log("resize", e.width, e.height)
})

w.onKeyDown.add(fn(e: gui.KeyEvent) {
    if e.code == gui.KeyCode.escape {
        console.log("escape pressed")
    } else {
        console.log("keyDown", e.code,
                    "ctrl=", e.ctrlKey, "shift=", e.shiftKey,
                    "alt=", e.altKey, "meta=", e.metaKey)
    }
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

// Push button — configure before `addChild` so the cached values
// are applied at native-realization time.
let b = new gui.Button()
b.text   = "Click me"
b.x      = 180.0
b.y      = 140.0
b.width  = 120.0
b.height = 32.0
b.onClick.add(fn(e: gui.MouseEvent) {
    console.log("clicked at", e.x, e.y)
})
w.addChild(b)

w.show()
gui.run()
