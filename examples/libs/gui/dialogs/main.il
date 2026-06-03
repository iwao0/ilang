// 💬 libs/gui — alert / confirm / openFile / saveFile.
//
// Four buttons drive each of the new Window dialog APIs. A status
// label below the row shows the latest callback's payload so the
// user can see what came back from each dialog.
//
// Run with:
//   ilang run examples/libs/gui/dialogs/main.il

use gui

let w = new gui.Window()
w.title  = "Dialog demo"
w.width  = 480.0
w.height = 200.0

let status = new gui.Label()
status.text   = "Click a button to open a dialog."
status.x      = 20.0
status.y      = 130.0
status.width  = 440.0
status.height = 22.0
w.addChild(status)

let bAlert = new gui.Button()
bAlert.text   = "Alert"
bAlert.x      = 20.0
bAlert.y      = 60.0
bAlert.width  = 100.0
bAlert.height = 30.0
bAlert.onClick.add(fn(e: gui.MouseEvent) {
    w.alert("Hello from gui.alert!", fn() {
        status.text = "alert: dismissed"
    })
})
w.addChild(bAlert)

let bConfirm = new gui.Button()
bConfirm.text   = "Confirm"
bConfirm.x      = 130.0
bConfirm.y      = 60.0
bConfirm.width  = 100.0
bConfirm.height = 30.0
bConfirm.onClick.add(fn(e: gui.MouseEvent) {
    w.confirm("Proceed?", fn(ok: bool) {
        status.text = if ok { "confirm: OK" } else { "confirm: cancel" }
    })
})
w.addChild(bConfirm)

let bOpen = new gui.Button()
bOpen.text   = "Open File"
bOpen.x      = 240.0
bOpen.y      = 60.0
bOpen.width  = 100.0
bOpen.height = 30.0
bOpen.onClick.add(fn(e: gui.MouseEvent) {
    w.openFile(fn(path: string) {
        status.text = if path == "" { "open: cancelled" } else { "open: " + path }
    })
})
w.addChild(bOpen)

let bSave = new gui.Button()
bSave.text   = "Save File"
bSave.x      = 350.0
bSave.y      = 60.0
bSave.width  = 100.0
bSave.height = 30.0
bSave.onClick.add(fn(e: gui.MouseEvent) {
    w.saveFile("untitled.txt", fn(path: string) {
        status.text = if path == "" { "save: cancelled" } else { "save: " + path }
    })
})
w.addChild(bSave)

w.show()
gui.run()
