// 🪟 libs/gui — minimal window. Opens a 480×320 window, registers
// an `onClose` listener that prints to stdout when the user
// closes it, and runs the platform event loop until close.
//
// Run on macOS:
//   ilang run examples/libs/gui/window/main.il
//
// Behind the scenes `[deps] @target` in `libs/gui/core/ilang.toml`
// swaps the backing implementation between `libs/gui/cocoa` and
// `libs/gui/win32`, so this file is identical on both hosts —
// no `@target` or OS-specific imports here.

use gui

let w = gui.createWindow("ilang × gui", 480.0, 320.0)

w.onClose.add(fn(e: gui.CloseEvent) {
    console.log("window closing")
})

gui.run()
