// 🪟 libs/gui — SplitView demo. Three coloured panels separated
// by two draggable splitter bars. The user can drag either
// splitter to redistribute width between the panes.
//
// Run with:
//   ilang run examples/libs/gui/splitview/main.il

use gui

let w = new gui.Window()
w.title  = "SplitView demo"
w.width  = 480.0
w.height = 280.0

let s = new gui.SplitView()
s.x      = 0.0
s.y      = 0.0
s.width  = 480.0
s.height = 280.0
s.orientation = gui.SplitOrientation.horizontal
w.addChild(s)

// Three panes — each shown via a Label so the splitter is
// visually obvious (without children, the panes would just be
// flat colour and the splitter wouldn't have anything to
// separate).
let left = new gui.Label()
left.text   = "Left pane"
left.x      = 10.0
left.y      = 20.0
left.width  = 140.0
left.height = 22.0
s.addChild(left)

let middle = new gui.Label()
middle.text   = "Middle pane"
middle.x      = 10.0
middle.y      = 20.0
middle.width  = 140.0
middle.height = 22.0
s.addChild(middle)

let right = new gui.Label()
right.text   = "Right pane"
right.x      = 10.0
right.y      = 20.0
right.width  = 140.0
right.height = 22.0
s.addChild(right)

// Place the splitters at thirds.
s.setSplitPosition(0 as i32, 160.0)
s.setSplitPosition(1 as i32, 320.0)

w.show()
gui.run()
