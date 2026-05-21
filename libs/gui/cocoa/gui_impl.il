/// macOS implementation of the `gui-core` bridge API.
///
/// `gui-core` (consumer-facing package) calls the functions in
/// this file via `[deps] @target("macos")`. Everything Cocoa-
/// specific stays here — gui-core never sees `NSWindow`,
/// `NSObject`, or any `objc_msgSend`. Public API contract (must
/// stay in sync with `libs/gui-win32`):
///
///   createNativeWindow(title, w, h): i64
///   setOnClose(handle, cb)
///   runEventLoop()
///
/// Today's slice supports one window per process and wires only
/// `Close`. Multi-window plus `Resize` / `Key` / `Mouse` land in
/// follow-up patches.

use cocoa {
    NSObject, NSApplication, NSWindow, NSNotification,
    NSWindowStyleMask, NSApplicationActivationPolicy, nil,
    sharedApplication, makeWindow,
}

/// Single-window callback state. `WIN_ON_CLOSE` defaults to a
/// no-op closure so `windowWillClose:` is safe to call even if
/// gui-core hasn't registered yet. The delegate is parked in
/// `DELEGATE_KEEP_ALIVE` for the process lifetime — NSWindow
/// references its delegate weakly, so the ilang side has to
/// own a +1.
let WIN_ON_CLOSE: fn() = fn() {}
let DELEGATE_KEEP_ALIVE: NSObject[] = []

/// Custom `NSWindowDelegate` subclass. Methods only (the @objc
/// auto-lift doesn't surface ilang field storage), so the close
/// callback is reached via the module-level `WIN_ON_CLOSE` slot.
class GuiDelegate : NSObject {
    pub windowWillClose(n: NSNotification) {
        let _ = n
        let cb = WIN_ON_CLOSE
        cb()
        // Drop out of `[NSApp run]` so the ilang-side `gui.run()`
        // returns. `terminate:` ends the process; `stop:` would
        // be cleaner but is a no-op outside an event handler.
        let app = NSApplication.sharedApplication()
        app.terminate(nil)
    }
}

/// Open an NSWindow at the screen centre with the standard
/// titled / closable / miniaturizable / resizable mask. Returns
/// the NSWindow pointer cast to i64 so gui-core can pass it
/// back through `setOnClose` — even though today's slice
/// ignores the handle and uses the single-window slot.
pub fn createNativeWindow(title: string, width: f64, height: f64): i64 {
    let app = sharedApplication()
    app.setActivationPolicy(NSApplicationActivationPolicy.regular)

    let mask = NSWindowStyleMask.titled
        | NSWindowStyleMask.closable
        | NSWindowStyleMask.miniaturizable
        | NSWindowStyleMask.resizable
    let win = makeWindow(title, 0.0, 0.0, width, height, mask)
    win.center()

    GuiDelegate.register()
    let d = GuiDelegate.alloc().init()
    win.setDelegate(d)

    win.makeKeyAndOrderFront(nil)
    app.activate()

    DELEGATE_KEEP_ALIVE.push(d as NSObject)
    // Handle value is opaque to gui-core; pick a non-zero stand-in
    // until multi-window support keys by NSWindow pointer.
    1
}

/// Replace the close callback. Today's single-window slot
/// ignores `handle`.
pub fn setOnClose(handle: i64, cb: fn()) {
    let _ = handle
    WIN_ON_CLOSE = cb
}

/// Run `[NSApp run]` until `terminate:` is sent (today: from
/// `GuiDelegate.windowWillClose:`). Blocks the calling thread.
pub fn runEventLoop() {
    let app = NSApplication.sharedApplication()
    app.run()
}
