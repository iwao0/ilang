// 📦 libs/gui — Panel demo.
//
// Two `Panel`s sit side-by-side. Each hosts its own widgets at
// panel-local x / y. Toggling visibility on one panel hides all
// of its children with a single call; moving the panel moves
// every child along with it.
//
// Run with:
//   ilang run examples/libs/gui/panel/main.il

use gui

let w = new gui.Window()
w.title  = "ilang × gui — Panel"
w.width  = 520.0
w.height = 280.0

// Left panel
let left = new gui.Panel()
left.x = 20.0; left.y = 20.0
left.width = 220.0; left.height = 200.0
w.addChild(left)

let leftTitle = new gui.Label()
leftTitle.text = "Left panel"
leftTitle.x = 8.0; leftTitle.y = 8.0
leftTitle.width = 200.0; leftTitle.height = 20.0
left.addChild(leftTitle)

let cb1 = new gui.Checkbox()
cb1.text = "Option A"; cb1.x = 8.0; cb1.y = 36.0
cb1.width = 180.0; cb1.height = 22.0
left.addChild(cb1)

let cb2 = new gui.Checkbox()
cb2.text = "Option B"; cb2.x = 8.0; cb2.y = 62.0
cb2.width = 180.0; cb2.height = 22.0
left.addChild(cb2)

// Right panel
let right = new gui.Panel()
right.x = 260.0; right.y = 20.0
right.width = 220.0; right.height = 200.0
w.addChild(right)

let rightTitle = new gui.Label()
rightTitle.text = "Right panel"
rightTitle.x = 8.0; rightTitle.y = 8.0
rightTitle.width = 200.0; rightTitle.height = 20.0
right.addChild(rightTitle)

let toggle = new gui.Button()
toggle.text = "Hide left"
toggle.x = 8.0; toggle.y = 36.0
toggle.width = 120.0; toggle.height = 28.0
right.addChild(toggle)

toggle.onClick.add(fn(e: gui.MouseEvent) {
    left.isVisible = !left.isVisible
    toggle.text = if left.isVisible { "Hide left" } else { "Show left" }
})

w.show()
gui.run()
