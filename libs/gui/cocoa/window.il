/// Top-level NSWindow creation + lifecycle.
///
/// Allocates the window hidden — gui-core's `Window.show()`
/// flips it on screen — and installs `GuiDelegate` / `GuiView`
/// so the close / resize / key / mouse channels in `core.il`
/// start firing.

use cocoa {
    NSObject, NSApplication, NSWindow, NSView, NSScreen,
    NSWindowStyleMask, NSApplicationActivationPolicy,
    NSPoint, NSRect, NSSize,
    nsstr, makePoint, makeRect, makeSize, nil,
    sharedApplication, makeWindow,
}
/// `NSFloatingWindowLevel` raw value (NSWindow.Level.floating).
/// Above the normal application band but below status / menu bar.
const FLOATING_LEVEL: i64 = 3 as i64
const NORMAL_LEVEL:   i64 = 0 as i64
const FULLSCREEN_BIT: u64 = NSWindowStyleMask.fullScreen as u64
const RESIZABLE_BIT:  u64 = NSWindowStyleMask.resizable  as u64
const CLOSABLE_BIT:   u64 = NSWindowStyleMask.closable   as u64
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

// ─── window state actions ──────────────────────────────────────

pub fn minimizeNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.miniaturize(nil)
    }
}
pub fn restoreNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.deminiaturize(nil)
    }
}

/// `zoom:` is a TOGGLE on Cocoa (no separate maximize / unmaximize
/// actions). Guard the call on the current `isZoomed` bit so
/// `maximizeNativeWindow` is idempotent: calling it on an already
/// zoomed window is a no-op rather than an "un-zoom".
pub fn maximizeNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        if !slot.win.isZoomed {
            slot.win.zoom(nil)
        }
    }
}
pub fn unmaximizeNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        if slot.win.isZoomed {
            slot.win.zoom(nil)
        }
    }
}

/// Full-screen via `toggleFullScreen:` — also a TOGGLE, so we
/// check the `fullScreen` style-mask bit and only flip when the
/// current state disagrees with `on`.
pub fn setNativeWindowFullScreen(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        let isFs = (slot.win.styleMask as u64 & FULLSCREEN_BIT) != 0 as u64
        if isFs != on { slot.win.toggleFullScreen(nil) }
    }
}

pub fn isNativeWindowMinimized(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.isMiniaturized
    } else {
        false
    }
}
pub fn isNativeWindowMaximized(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.isZoomed
    } else {
        false
    }
}
pub fn isNativeWindowFullScreen(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        (slot.win.styleMask as u64 & FULLSCREEN_BIT) != 0 as u64
    } else {
        false
    }
}

// ─── window-trait flags ────────────────────────────────────────

/// Toggle a single bit on the window's style mask. Cocoa keeps
/// the rest of the chrome (title bar, miniaturise button, …) in
/// sync; the AppKit re-frame happens implicitly.
fn setStyleBit(slot: WindowSlot, bit: u64, on: bool) {
    let mask = slot.win.styleMask as u64
    let nextMask = if on { mask | bit } else { mask & ~bit }
    if nextMask != mask {
        slot.win.setStyleMask(nextMask as NSWindowStyleMask)
    }
}

pub fn setNativeWindowResizable(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        setStyleBit(slot, RESIZABLE_BIT, on)
    }
}
pub fn isNativeWindowResizable(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        (slot.win.styleMask as u64 & RESIZABLE_BIT) != 0 as u64
    } else {
        false
    }
}

pub fn setNativeWindowClosable(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        setStyleBit(slot, CLOSABLE_BIT, on)
    }
}
pub fn isNativeWindowClosable(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        (slot.win.styleMask as u64 & CLOSABLE_BIT) != 0 as u64
    } else {
        false
    }
}

pub fn setNativeWindowMovable(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.setMovable(on)
    }
}
pub fn isNativeWindowMovable(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.isMovable
    } else {
        false
    }
}

// ─── focus / Z-order ───────────────────────────────────────────

/// `focus` = make the window key + raise to the front of its
/// peer band, then bounce the app into the foreground so the
/// transition is visible regardless of current activation state.
pub fn focusNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.makeKeyAndOrderFront(nil)
        sharedApplication().activate()
    }
}

/// `blur` has no per-window AppKit primitive; the closest thing
/// is deactivating the application so the OS hands the key
/// window slot to whoever is next in line. `handle` is accepted
/// for API symmetry across the platform backends — the
/// deactivate is process-wide. Mirrors Electron's
/// `BrowserWindow.blur()` semantics on macOS.
pub fn blurNativeWindow(handle: i64) {
    sharedApplication().deactivate()
}

pub fn isNativeWindowFocused(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.isKeyWindow
    } else {
        false
    }
}

/// Always-on-top = raise the window level into the floating
/// band. Lowering it back to the normal band restores ordinary
/// stacking with peer windows.
pub fn setNativeWindowAlwaysOnTop(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.setLevel(if on { FLOATING_LEVEL } else { NORMAL_LEVEL })
    }
}
pub fn isNativeWindowAlwaysOnTop(handle: i64): bool {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.level >= FLOATING_LEVEL
    } else {
        false
    }
}

/// Raise the window to the top of its peer band without touching
/// keyboard focus (vs. `focus`, which also makes it key). Useful
/// when a background window needs to surface after a notification
/// without stealing input from the user's active context.
pub fn moveTopNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.orderFront(nil)
    }
}

pub fn setNativeWindowOpacity(handle: i64, v: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.setAlphaValue(v)
    }
}
pub fn getNativeWindowOpacity(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.alphaValue
    } else {
        1.0
    }
}

// ─── geometry / position / bounds ──────────────────────────────
//
// The public Window API works in W3C-style top-left screen
// coordinates with content-sized rectangles. AppKit uses
// bottom-left screen coordinates and frame-sized rectangles (the
// frame includes the title bar / shadow). Both transformations
// happen here so the rest of the codebase doesn't have to think
// about it.

fn screenHeight(): f64 {
    NSScreen.mainScreen().frame.size.height
}

/// Top-left → AppKit's bottom-left `frame.origin.y` for a frame
/// of height `frameH`. Inverse on getter.
fn frameOriginYFromTopLeftY(topY: f64, frameH: f64): f64 {
    screenHeight() - (topY + frameH)
}

pub fn centerNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.center()
    }
}

pub fn setNativeWindowPosition(handle: i64, x: f64, y: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        let frameH = slot.win.frame.size.height
        slot.win.setFrameOrigin(makePoint(x, frameOriginYFromTopLeftY(y, frameH)))
    }
}
pub fn getNativeWindowX(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        slot.win.frame.origin.x
    } else {
        0.0
    }
}
pub fn getNativeWindowY(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        let f = slot.win.frame
        screenHeight() - (f.origin.y + f.size.height)
    } else {
        0.0
    }
}

/// `(x, y)` is the frame top-left in W3C screen coordinates;
/// `(w, h)` is the CONTENT size. Translate content size → frame
/// size, invert y, write the whole frame in one call.
pub fn setNativeWindowBounds(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        let frameRect = slot.win.frameRectForContentRect(makeRect(0.0, 0.0, w, h))
        let frameW = frameRect.size.width
        let frameH = frameRect.size.height
        slot.win.setFrame(
            makeRect(x, frameOriginYFromTopLeftY(y, frameH), frameW, frameH),
            true
        )
    }
}

/// AppKit's `setMinSize:` / `setMaxSize:` measure the FRAME
/// (title bar + content). The public API takes content sizes, so
/// we translate via `frameRectForContentRect:` on the way in and
/// `contentRectForFrameRect:` on the way out.
pub fn setNativeWindowMinSize(handle: i64, w: f64, h: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        let frameRect = slot.win.frameRectForContentRect(makeRect(0.0, 0.0, w, h))
        slot.win.setMinSize(frameRect.size)
    }
}
pub fn getNativeWindowMinWidth(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        let s = slot.win.minSize
        slot.win.contentRectForFrameRect(makeRect(0.0, 0.0, s.width, s.height))
            .size.width
    } else {
        0.0
    }
}
pub fn getNativeWindowMinHeight(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        let s = slot.win.minSize
        slot.win.contentRectForFrameRect(makeRect(0.0, 0.0, s.width, s.height))
            .size.height
    } else {
        0.0
    }
}
pub fn setNativeWindowMaxSize(handle: i64, w: f64, h: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        let frameRect = slot.win.frameRectForContentRect(makeRect(0.0, 0.0, w, h))
        slot.win.setMaxSize(frameRect.size)
    }
}
pub fn getNativeWindowMaxWidth(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        let s = slot.win.maxSize
        slot.win.contentRectForFrameRect(makeRect(0.0, 0.0, s.width, s.height))
            .size.width
    } else {
        0.0
    }
}
pub fn getNativeWindowMaxHeight(handle: i64): f64 {
    if let some(slot) = WINDOWS.get(handle) {
        let s = slot.win.maxSize
        slot.win.contentRectForFrameRect(makeRect(0.0, 0.0, s.width, s.height))
            .size.height
    } else {
        0.0
    }
}

// ─── attention / progress ──────────────────────────────────────

/// `NSRequestUserAttentionType.NSCriticalRequest` raw value —
/// the bounce-until-acknowledged variant. Informational requests
/// (`10`) bounce once and fall back; users wanting that behaviour
/// can call `requestUserAttention` directly on the shared app.
const CRITICAL_REQUEST: i64 = 0 as i64

/// `flashFrame(true)` bounces the Dock icon until the user
/// activates the app (or `flashFrame(false)` cancels the pending
/// request). The request ID returned by AppKit is stored on the
/// slot so the cancel path can hand it back to
/// `cancelUserAttentionRequest:`.
pub fn flashNativeWindow(handle: i64, on: bool) {
    if let some(slot) = WINDOWS.get(handle) {
        let app = sharedApplication()
        if on {
            if slot.flashRequestId == 0 as i64 {
                slot.flashRequestId = app.requestUserAttention(CRITICAL_REQUEST)
            }
        } else {
            if slot.flashRequestId != 0 as i64 {
                app.cancelUserAttentionRequest(slot.flashRequestId)
                slot.flashRequestId = 0 as i64
            }
        }
    }
}
