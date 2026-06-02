// libs/gui — `Window` state-action / state-event demo.
// Four buttons drive `minimize()`, `maximize()`, `unmaximize()`,
// and a `setFullScreen()` toggle; the status label below reflects
// the most recent state-event the window emitted.
//
// (`Restore` has no button — once the window is iconified, none
// of its own buttons are clickable. Click the Dock icon to bring
// it back; `onRestore` will fire and the label will update.)
//
// Run on macOS:
//   cargo run -p ilang -- run examples/libs/gui/window_state_demo/main.il

use gui

fn run(): i64 {
    let w = new gui.Window()
    w.title  = "ilang gui × window state"
    w.width  = 560.0
    w.height = 200.0

    let v = new gui.VBox()
    v.dock = gui.Dock.fill
    w.addChild(v)

    let row = new gui.HBox()
    row.height = 36.0
    row.margin = new gui.Margin(4.0)
    v.addChild(row)

    let bMin = new gui.Button()
    bMin.text   = "Minimize"
    bMin.flex   = 1.0
    bMin.margin = new gui.Margin(4.0)
    bMin.onClick.add(fn(e: gui.MouseEvent) { w.minimize() })
    row.addChild(bMin)

    let bMax = new gui.Button()
    bMax.text   = "Maximize"
    bMax.flex   = 1.0
    bMax.margin = new gui.Margin(4.0)
    bMax.onClick.add(fn(e: gui.MouseEvent) { w.maximize() })
    row.addChild(bMax)

    let bUnmax = new gui.Button()
    bUnmax.text   = "Unmaximize"
    bUnmax.flex   = 1.0
    bUnmax.margin = new gui.Margin(4.0)
    bUnmax.onClick.add(fn(e: gui.MouseEvent) { w.unmaximize() })
    row.addChild(bUnmax)

    let bFs = new gui.Button()
    bFs.text   = "Toggle FullScreen"
    bFs.flex   = 1.0
    bFs.margin = new gui.Margin(4.0)
    bFs.onClick.add(fn(e: gui.MouseEvent) {
        w.setFullScreen(!w.isFullScreen())
    })
    row.addChild(bFs)

    let status = new gui.Label()
    status.text   = "(no state event yet)"
    status.flex   = 1.0
    status.margin = new gui.Margin(8.0)
    v.addChild(status)

    w.onMinimize.add(fn(e: gui.MinimizeEvent) {
        status.text = "onMinimize fired"
    })
    w.onRestore.add(fn(e: gui.RestoreEvent) {
        status.text = "onRestore fired"
    })
    w.onMaximize.add(fn(e: gui.MaximizeEvent) {
        status.text = "onMaximize fired"
    })
    w.onUnmaximize.add(fn(e: gui.UnmaximizeEvent) {
        status.text = "onUnmaximize fired"
    })
    w.onEnterFullScreen.add(fn(e: gui.EnterFullScreenEvent) {
        status.text = "onEnterFullScreen fired"
    })
    w.onLeaveFullScreen.add(fn(e: gui.LeaveFullScreenEvent) {
        status.text = "onLeaveFullScreen fired"
    })

    w.show()
    gui.run()
    0
}

run()
