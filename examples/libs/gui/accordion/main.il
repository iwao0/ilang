// 📂 libs/gui — Accordion demo.
//
// Two collapsible sections stacked vertically. Each header
// toggles its own content panel; the example reflows the second
// accordion's y when the first one expands / collapses so they
// don't overlap.
//
// Run with:
//   ilang run examples/libs/gui/accordion/main.il

use gui

let w = new gui.Window()
w.title  = "ilang × gui — Accordion"
w.width  = 360.0
w.height = 360.0

let basics = new gui.Accordion("Basics")
basics.x      = 20.0
basics.y      = 20.0
basics.width  = 320.0
basics.contentHeight = 80.0
basics.expanded = true
w.addChild(basics)

let cb1 = new gui.Checkbox()
cb1.text   = "Enable feature A"
cb1.x      = 12.0
cb1.y      = 12.0
cb1.width  = 220.0
cb1.height = 22.0
cb1.checked = true
basics.addChild(cb1)

let cb2 = new gui.Checkbox()
cb2.text   = "Enable feature B"
cb2.x      = 12.0
cb2.y      = 40.0
cb2.width  = 220.0
cb2.height = 22.0
basics.addChild(cb2)

let advanced = new gui.Accordion("Advanced")
advanced.x      = 20.0
advanced.y      = basics.y + basics.height + 8.0
advanced.width  = 320.0
advanced.contentHeight = 100.0
w.addChild(advanced)

let sw = new gui.Switch()
sw.x      = 12.0
sw.y      = 12.0
advanced.addChild(sw)

let info = new gui.Label()
info.text   = "Diagnostics enabled"
info.x      = 60.0
info.y      = 14.0
info.width  = 240.0
info.height = 22.0
advanced.addChild(info)

basics.onToggle.add(fn(e: gui.ToggleEvent) {
    console.log("basics expanded:", e.checked)
    advanced.y = basics.y + basics.height + 8.0
})
advanced.onToggle.add(fn(e: gui.ToggleEvent) {
    console.log("advanced expanded:", e.checked)
})

w.show()
gui.run()
