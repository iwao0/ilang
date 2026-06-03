// libs/gui — `Window` state-action / state-event / trait-flag /
// focus / Z-order / geometry demo.
//
// Row 1: `minimize()`, `maximize()`, `unmaximize()`, `setFullScreen()` toggle.
// Row 2: `setResizable` / `setMovable` / `setClosable` toggles.
// Row 3: `blur()`, `setAlwaysOnTop()` toggle, `moveTop()`, opacity toggle.
// Row 4: `center()`, `setPosition(100,100)`, `setMinimumSize(400,200)`.
// Row 5: `flashFrame(true)` (after a 3-second delay so you have
//        time to switch apps; AppKit ignores requestUserAttention
//        when the app is already active) / `flashFrame(false)`.
// Status label below reflects the most recent fired event.
//
// (`Restore` has no button — once iconified, none of its own
// buttons are clickable. Click the Dock icon to bring it back.)
//
// Run on macOS:
//   cargo run -p ilang -- run examples/libs/gui/window_state_demo/main.il

use gui
use cocoa { NSTimer }

fn run(): i64 {
    let w = new gui.Window()
    w.title  = "ilang gui × window state"
    w.width  = 600.0
    w.height = 360.0

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

    // Trait-flag toggles. Each click reads the current state via
    // the matching `isXxx()` query, flips it, and rewrites the
    // button label so the user can see the new value at a glance.
    let traits = new gui.HBox()
    traits.height = 36.0
    traits.margin = new gui.Margin(4.0)
    v.addChild(traits)

    let bResizable = new gui.Button()
    bResizable.text   = "Resizable: ON"
    bResizable.flex   = 1.0
    bResizable.margin = new gui.Margin(4.0)
    bResizable.onClick.add(fn(e: gui.MouseEvent) {
        let next = !w.isResizable()
        w.setResizable(next)
        bResizable.text = if next { "Resizable: ON" } else { "Resizable: OFF" }
    })
    traits.addChild(bResizable)

    let bMovable = new gui.Button()
    bMovable.text   = "Movable: ON"
    bMovable.flex   = 1.0
    bMovable.margin = new gui.Margin(4.0)
    bMovable.onClick.add(fn(e: gui.MouseEvent) {
        let next = !w.isMovable()
        w.setMovable(next)
        bMovable.text = if next { "Movable: ON" } else { "Movable: OFF" }
    })
    traits.addChild(bMovable)

    let bClosable = new gui.Button()
    bClosable.text   = "Closable: ON"
    bClosable.flex   = 1.0
    bClosable.margin = new gui.Margin(4.0)
    bClosable.onClick.add(fn(e: gui.MouseEvent) {
        let next = !w.isClosable()
        w.setClosable(next)
        bClosable.text = if next { "Closable: ON" } else { "Closable: OFF" }
    })
    traits.addChild(bClosable)

    // Focus / Z-order row. `Blur` deactivates the app; the user
    // can click back into the window to watch `onFocus` fire.
    // `Always-on-top` toggles the floating Z band. `Move top`
    // raises the window without taking focus.
    let zorder = new gui.HBox()
    zorder.height = 36.0
    zorder.margin = new gui.Margin(4.0)
    v.addChild(zorder)

    let bBlur = new gui.Button()
    bBlur.text   = "Blur"
    bBlur.flex   = 1.0
    bBlur.margin = new gui.Margin(4.0)
    bBlur.onClick.add(fn(e: gui.MouseEvent) { w.blur() })
    zorder.addChild(bBlur)

    let bTop = new gui.Button()
    bTop.text   = "AlwaysOnTop: OFF"
    bTop.flex   = 1.0
    bTop.margin = new gui.Margin(4.0)
    bTop.onClick.add(fn(e: gui.MouseEvent) {
        let next = !w.isAlwaysOnTop()
        w.setAlwaysOnTop(next)
        bTop.text = if next { "AlwaysOnTop: ON" } else { "AlwaysOnTop: OFF" }
    })
    zorder.addChild(bTop)

    let bMoveTop = new gui.Button()
    bMoveTop.text   = "Move Top"
    bMoveTop.flex   = 1.0
    bMoveTop.margin = new gui.Margin(4.0)
    bMoveTop.onClick.add(fn(e: gui.MouseEvent) { w.moveTop() })
    zorder.addChild(bMoveTop)

    // Opacity toggle. Reads `getOpacity()` and flips between fully
    // opaque (1.0) and half-transparent (0.5) — same idea as the
    // boolean trait toggles above, just on a continuous knob.
    let bOpacity = new gui.Button()
    bOpacity.text   = "Opacity: 1.0"
    bOpacity.flex   = 1.0
    bOpacity.margin = new gui.Margin(4.0)
    bOpacity.onClick.add(fn(e: gui.MouseEvent) {
        let next = if w.getOpacity() > 0.75 { 0.5 } else { 1.0 }
        w.setOpacity(next)
        bOpacity.text = if next < 0.75 { "Opacity: 0.5" } else { "Opacity: 1.0" }
    })
    zorder.addChild(bOpacity)

    // Geometry row — `center()`, fixed-position move, min-size set.
    let geom = new gui.HBox()
    geom.height = 36.0
    geom.margin = new gui.Margin(4.0)
    v.addChild(geom)

    let bCenter = new gui.Button()
    bCenter.text   = "Center"
    bCenter.flex   = 1.0
    bCenter.margin = new gui.Margin(4.0)
    bCenter.onClick.add(fn(e: gui.MouseEvent) { w.center() })
    geom.addChild(bCenter)

    let bPos = new gui.Button()
    bPos.text   = "Position (100, 100)"
    bPos.flex   = 1.0
    bPos.margin = new gui.Margin(4.0)
    bPos.onClick.add(fn(e: gui.MouseEvent) { w.setPosition(100.0, 100.0) })
    geom.addChild(bPos)

    let bMinSize = new gui.Button()
    bMinSize.text   = "MinSize 400x200"
    bMinSize.flex   = 1.0
    bMinSize.margin = new gui.Margin(4.0)
    bMinSize.onClick.add(fn(e: gui.MouseEvent) {
        w.setMinimumSize(400.0, 200.0)
    })
    geom.addChild(bMinSize)

    // Attention row — `flashFrame(true)` keeps the Dock icon
    // bouncing until the user activates the app (or `(false)`
    // cancels). To actually SEE the bounce: click "Flash" then
    // switch to another app.
    let attn = new gui.HBox()
    attn.height = 36.0
    attn.margin = new gui.Margin(4.0)
    v.addChild(attn)

    // `requestUserAttention:` is a no-op while the app is the
    // active one, so the click schedules a 3-second NSTimer
    // instead of calling `flashFrame(true)` straight away. Switch
    // to another app (Finder / browser / etc.) before the delay
    // elapses to see the Dock bounce.
    let bFlash = new gui.Button()
    bFlash.text   = "Flash in 3s"
    bFlash.flex   = 1.0
    bFlash.margin = new gui.Margin(4.0)
    bFlash.onClick.add(fn(e: gui.MouseEvent) {
        let timer = NSTimer.scheduledTimer(3.0, false, fn(t: i64) {
            w.flashFrame(true)
        })
    })
    attn.addChild(bFlash)

    let bUnflash = new gui.Button()
    bUnflash.text   = "Stop Flash"
    bUnflash.flex   = 1.0
    bUnflash.margin = new gui.Margin(4.0)
    bUnflash.onClick.add(fn(e: gui.MouseEvent) { w.flashFrame(false) })
    attn.addChild(bUnflash)

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
    w.onFocus.add(fn(e: gui.FocusEvent) {
        status.text = "onFocus fired"
    })
    w.onBlur.add(fn(e: gui.BlurEvent) {
        status.text = "onBlur fired"
    })
    // `onShow` fires synchronously from inside `w.show()` below —
    // so this listener writes the initial status text. `onHide`
    // has no trigger in this demo (the only way to hide would
    // wedge the user out of the window) but the listener is
    // wired up for completeness.
    w.onShow.add(fn(e: gui.ShowEvent) {
        status.text = "onShow fired"
    })
    w.onHide.add(fn(e: gui.HideEvent) {
        status.text = "onHide fired"
    })
    // Drag the title bar to fire `onMove`. The payload is the
    // new top-left of the window in screen coordinates.
    w.onMove.add(fn(e: gui.MoveEvent) {
        status.text = "onMove fired (x=" + (e.x as i64).toString() + ", y=" + (e.y as i64).toString() + ")"
    })

    w.show()
    gui.run()
    0
}

run()
