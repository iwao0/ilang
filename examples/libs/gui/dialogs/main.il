// 💬 libs/gui — alert / confirm / openFile / saveFile + message /
// colorPicker / fontPicker (Round 2). Seven buttons across two
// rows drive each of the Window dialog APIs; a status label
// below shows the latest callback's payload.
//
// Run with:
//   ilang run examples/libs/gui/dialogs/main.il

use gui

let w = new gui.Window()
w.title  = "Dialog demo"
w.width  = 480.0
w.height = 260.0

let status = new gui.Label()
status.text   = "Click a button to open a dialog."
status.x      = 20.0
status.y      = 190.0
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

// ─── Round 2 row ──────────────────────────────────────────────

let bMessage = new gui.Button()
bMessage.text   = "Message"
bMessage.x      = 20.0
bMessage.y      = 110.0
bMessage.width  = 100.0
bMessage.height = 30.0
bMessage.onClick.add(fn(e: gui.MouseEvent) {
    w.message(
        "Save changes before closing?",
        ["Save", "Discard", "Cancel"],
        gui.MessageIcon.question,
        fn(idx: i32) {
            status.text = if idx == 0 as i32 {
                "message: Save"
            } elif idx == 1 as i32 {
                "message: Discard"
            } elif idx == 2 as i32 {
                "message: Cancel"
            } else {
                "message: dismissed"
            }
        }
    )
})
w.addChild(bMessage)

let bColor = new gui.Button()
bColor.text   = "Color"
bColor.x      = 130.0
bColor.y      = 110.0
bColor.width  = 100.0
bColor.height = 30.0
bColor.onClick.add(fn(e: gui.MouseEvent) {
    w.colorPicker(0xFFFF3344 as u32, fn(color: u32, accepted: bool) {
        status.text = if accepted {
            "color: " + color.toString()
        } else {
            "color: cancelled"
        }
    })
})
w.addChild(bColor)

let bFont = new gui.Button()
bFont.text   = "Font"
bFont.x      = 240.0
bFont.y      = 110.0
bFont.width  = 100.0
bFont.height = 30.0
bFont.onClick.add(fn(e: gui.MouseEvent) {
    let seed = new gui.Font("sans-serif", 13.0, 400 as i32, false)
    w.fontPicker(seed, fn(f: gui.Font, accepted: bool) {
        status.text = if accepted {
            "font: " + f.family + " " + (f.size as i32).toString()
        } else {
            "font: cancelled"
        }
    })
})
w.addChild(bFont)

w.show()
gui.run()
