/// NSWindow — construct and read back geometry. Doesn't show
/// the window or run the event loop, so this is safe in
/// headless contexts.

use std.test as test
use appkit { NSWindow, NSWindowStyleMask, NSBackingStoreType,
             sharedApplication, makeWindow }
use foundation { makeRect, nsstr, ilstr }

let _ = sharedApplication()

let mask = NSWindowStyleMask.titled
    | NSWindowStyleMask.closable
    | NSWindowStyleMask.resizable
let win = makeWindow("ilang test window", 100.0, 200.0, 400.0, 300.0, mask)
test.expectFalse(win.handle == 0)

// The window's content view exists and reports a usable frame.
let cv = win.contentView
test.expectFalse(cv.handle == 0)
let cvFrame = cv.frame
test.expectTrue(cvFrame.size.width > 0.0)
test.expectTrue(cvFrame.size.height > 0.0)

// Direct alloc + init form too — confirms the typed enum
// parameters thread through to the underlying ObjC
// `initWithContentRect:styleMask:backing:defer:`.
let w2 = NSWindow.alloc().init(
    makeRect(0.0, 0.0, 200.0, 100.0),
    NSWindowStyleMask.titled,
    NSBackingStoreType.buffered,
    false
)
test.expectFalse(w2.handle == 0)

0
