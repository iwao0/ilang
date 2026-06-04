// 🛠 libs/gui — Toolbar demo. Six items along the top edge:
// three left-aligned buttons + separator + flexible space +
// trailing "Help" button. A status label below the toolbar
// shows which item was last clicked.
//
// Run with:
//   ilang run examples/libs/gui/toolbar/main.il

use gui

let w = new gui.Window()
w.title  = "Toolbar demo"
w.width  = 440.0
w.height = 220.0

let status = new gui.Label()
status.text   = "Click a toolbar item."
status.x      = 20.0
status.y      = 80.0
status.width  = 400.0
status.height = 22.0
w.addChild(status)

let bar = new gui.Toolbar([
    gui.ToolbarItem.button("New",  fn(e: gui.MouseEvent) {
        status.text = "Toolbar: New"
    }),
    gui.ToolbarItem.button("Open", fn(e: gui.MouseEvent) {
        status.text = "Toolbar: Open"
    }),
    gui.ToolbarItem.button("Save", fn(e: gui.MouseEvent) {
        status.text = "Toolbar: Save"
    }),
    gui.ToolbarItem.separator(),
    gui.ToolbarItem.flexibleSpace(),
    gui.ToolbarItem.button("Help", fn(e: gui.MouseEvent) {
        status.text = "Toolbar: Help"
    }),
])
w.setToolbar(bar)

w.show()
gui.run()
