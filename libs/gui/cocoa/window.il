/// Top-level NSWindow creation + lifecycle.
///
/// Allocates the window hidden — gui-core's `Window.show()`
/// flips it on screen — and installs `GuiDelegate` / `GuiView`
/// so the close / resize / key / mouse channels in `core.il`
/// start firing.

use cocoa {
    NSObject, NSApplication, NSWindow, NSView,
    NSWindowStyleMask, NSApplicationActivationPolicy,
    nsstr, makeSize, nil,
    sharedApplication, makeWindow,
}
use core { WINDOWS, WindowSlot, GuiDelegate, GuiAppDelegate, GuiView, registerView }

/// `app.setDelegate(...)` only needs to fire once per process. The
/// 1-element array stands in for an Option holder so the
/// initialiser stays inside ilang's idiom.
let APP_DELEGATE_INSTALLED: bool[] = [false]

pub fn createNativeWindow(title: string, width: f64, height: f64): i64 {
    let app = sharedApplication()
    app.setActivationPolicy(NSApplicationActivationPolicy.regular)
    if !APP_DELEGATE_INSTALLED[0] {
        APP_DELEGATE_INSTALLED[0] = true
        GuiAppDelegate.register()
        let ad = GuiAppDelegate.alloc().init()
        app.setDelegate(ad as NSObject)
    }

    let mask = NSWindowStyleMask.titled
        | NSWindowStyleMask.closable
        | NSWindowStyleMask.miniaturizable
        | NSWindowStyleMask.resizable
    let win = makeWindow(title, 0.0, 0.0, width, height, mask)
    win.center()

    GuiDelegate.register()
    let d = GuiDelegate.alloc().init()
    win.setDelegate(d)

    GuiView.register()
    let v = GuiView.alloc().init()
    win.setContentView(v)
    win.setAcceptsMouseMovedEvents(true)
    win.makeFirstResponder(v)

    // Created HIDDEN. The user controls first-paint via
    // `Window.show()` so they can mutate title / size / add
    // children before the window flashes on screen.

    let handle     = win.handle
    let viewHandle = (v as NSObject).handle
    WINDOWS[handle] = new WindowSlot(win, d as NSObject, v as NSObject, viewHandle)
    registerView(viewHandle, handle)
    handle
}

pub fn setWindowTitle(handle: i64, title: string) {
    if let some(slot) = WINDOWS.get(handle) { slot.win.setTitle(nsstr(title)) }
}
pub fn setWindowSize(handle: i64, width: f64, height: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.setContentSize(makeSize(width, height))
    }
}
pub fn showNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.makeKeyAndOrderFront(nil)
        let app = NSApplication.sharedApplication()
        app.activate()
    }
}
pub fn hideNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) { slot.win.orderOut(nil) }
}

pub fn runEventLoop() {
    let app = NSApplication.sharedApplication()
    app.run()
}
