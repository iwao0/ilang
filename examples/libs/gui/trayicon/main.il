// 📌 libs/gui — TrayIcon demo. Installs a menu-bar tray icon
// (macOS) with the label "◆" and a 3-item context menu. A
// status label in the main window mirrors the latest tray
// action so the user can see clicks land.
//
// macOS: the icon appears in the top-right system menu bar.
// Left-clicking the bar item fires `tray.onClick`; opening
// the context menu (right-click on Windows, single-click on
// macOS when a menu is installed) shows the item list.
//
// Run with:
//   ilang run examples/libs/gui/trayicon/main.il

use gui

let w = new gui.Window()
w.title  = "Tray demo"
w.width  = 360.0
w.height = 200.0

let status = new gui.Label()
status.text   = "Click the tray icon (◆ in the menu bar)."
status.x      = 20.0
status.y      = 80.0
status.width  = 320.0
status.height = 22.0
w.addChild(status)

let tray = new gui.TrayIcon()
tray.setTitle("◆")
tray.setTooltip("ilang TrayIcon demo")
tray.onClick.add(fn(e: gui.MouseEvent) {
    status.text = "Tray: left-click"
})

let menu = new gui.Menu("")
menu.addItem("Hello", fn() {
    status.text = "Tray menu: Hello"
})
menu.addItem("World", fn() {
    status.text = "Tray menu: World"
})
menu.addItem("Quit", fn() {
    status.text = "Tray menu: Quit (close window to exit)"
})
tray.setMenu(menu)

tray.show()
w.show()
gui.run()
