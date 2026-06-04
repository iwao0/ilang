// 📊 libs/gui — StatusBar demo. Three-segment status bar
// docked at the bottom of a 360×220 window. A button updates
// the leftmost segment each time it's clicked.
//
// Run with:
//   ilang run examples/libs/gui/statusbar/main.il

use gui

let w = new gui.Window()
w.title  = "StatusBar demo"
w.width  = 360.0
w.height = 220.0

let bar = new gui.StatusBar(["Ready", "Line 1", "UTF-8"])
w.setStatusBar(bar)

let counter: i32[] = [0 as i32]

let b = new gui.Button()
b.text   = "Update status"
b.x      = 20.0
b.y      = 30.0
b.width  = 160.0
b.height = 32.0
b.onClick.add(fn(e: gui.MouseEvent) {
    counter[0] = counter[0] + 1 as i32
    bar.setText(0 as i32, "Clicks: " + counter[0].toString())
})
w.addChild(b)

let label = new gui.Label()
label.text   = "Click the button — the bar's left segment changes."
label.x      = 20.0
label.y      = 80.0
label.width  = 320.0
label.height = 22.0
w.addChild(label)

w.show()
gui.run()
