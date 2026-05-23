// 🍔 libs/gui — MenuBar example.
//
// Two top-level menus ("File" and "Help") on a single window, each
// with a few items. On macOS the menus appear in the system menu
// bar; on Windows they sit at the top of the window. Selecting an
// item logs to the console.
//
// Run with:
//   ilang run examples/libs/gui/menus/main.il

use gui

let w = new gui.Window()
w.title  = "Menu demo"
w.width  = 480.0
w.height = 240.0

let file = new gui.Menu("File")
file.addItem("New", "N", fn() { console.log("File → New") })
file.addItem("Open", "O", fn() { console.log("File → Open") })
file.addSeparator()
file.addItem("Quit", "Q", fn() { console.log("File → Quit") })

let edit = new gui.Menu("Edit")
edit.addItem("Cut",   "X", fn() { console.log("Edit → Cut") })
edit.addItem("Copy",  "C", fn() { console.log("Edit → Copy") })
edit.addItem("Paste", "V", fn() { console.log("Edit → Paste") })

let help = new gui.Menu("Help")
help.addItem("About", fn() { console.log("Help → About") })

w.setMenuBar([file, edit, help])

w.onClose.add(fn(e: gui.CloseEvent) {
    console.log("window closed")
})

w.show()
gui.run()
