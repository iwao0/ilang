// 🔔 libs/gui — Notification demo. Two buttons:
//   - "Notify" fires an OS notification (banner / toast).
//   - "Notify with handler" fires another that would invoke
//     the onClick handler when the user activates the banner.
//
// Backend status (Round 1):
//   - macOS : STUB — the OS notification system requires a
//             registered bundle identifier and ilang runs as a
//             plain CLI without one. The buttons return
//             without showing anything. Round 2 will fix this
//             via an .app-bundle build path or a vendored
//             helper. The status label still updates so it's
//             obvious the click reached us.
//   - Linux : uses libnotify (notify_notification_new + show);
//             no click callback support yet.
//   - Win32 : stub — real toasts need AppUserModelID + WinRT.
//
// Run with:
//   ilang run examples/libs/gui/notification/main.il

use gui

let w = new gui.Window()
w.title  = "Notification demo"
w.width  = 400.0
w.height = 200.0

let status = new gui.Label()
status.text   = "Click a button to fire a notification."
status.x      = 20.0
status.y      = 110.0
status.width  = 360.0
status.height = 22.0
w.addChild(status)

let b1 = new gui.Button()
b1.text   = "Notify"
b1.x      = 20.0
b1.y      = 40.0
b1.width  = 140.0
b1.height = 32.0
b1.onClick.add(fn(e: gui.MouseEvent) {
    let n = new gui.Notification("Hello from ilang", "Plain banner — no click handler.")
    n.show()
    status.text = "Plain notification fired."
})
w.addChild(b1)

let b2 = new gui.Button()
b2.text   = "Notify with handler"
b2.x      = 180.0
b2.y      = 40.0
b2.width  = 200.0
b2.height = 32.0
b2.onClick.add(fn(e: gui.MouseEvent) {
    let n = new gui.Notification("Click me", "Activating this banner triggers onClick.")
    n.onClick.add(fn(e2: gui.MouseEvent) {
        status.text = "Notification banner clicked!"
    })
    n.show()
    status.text = "Click-handler notification fired."
})
w.addChild(b2)

w.show()
gui.run()
