// 🪟 libs/gui — ScrollView demo. Drops a 280×220 scroll view
// into a 320×260 window and parks several buttons across a
// 600×500 content area; the buttons in the lower-right corner
// only become reachable once the user scrolls.
//
// Run with:
//   ilang run examples/libs/gui/scrollview/main.il

use gui

let w = new gui.Window()
w.title  = "ScrollView demo"
w.width  = 320.0
w.height = 260.0

let s = new gui.ScrollView()
s.x      = 20.0
s.y      = 20.0
s.width  = 280.0
s.height = 220.0
w.addChild(s)

// Children scatter across a 600×500 logical content area.
let labels = ["top-left", "top-right", "middle", "bot-left", "bot-right"]
let xs:     f64[] = [10.0, 460.0, 240.0, 10.0, 440.0]
let ys:     f64[] = [10.0,  10.0, 230.0, 440.0, 440.0]

let i = 0
while i < labels.length {
    let b = new gui.Button()
    b.text   = labels[i]
    b.x      = xs[i]
    b.y      = ys[i]
    b.width  = 130.0
    b.height = 30.0
    s.addChild(b)
    i = i + 1
}

w.show()
gui.run()
