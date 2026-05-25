/// NSScreen — read-only screen geometry. Assumes the test
/// runs on a host with at least one display (Macs in headless
/// CI still have a virtual screen via WindowServer).

use std.test as test
use appkit { NSScreen }

let main = NSScreen.mainScreen()
test.expectFalse(main.handle == 0)

// Main-screen frame and visibleFrame are real rects.
let frame = main.frame
test.expectTrue(frame.size.width > 0.0)
test.expectTrue(frame.size.height > 0.0)
let visible = main.visibleFrame
test.expectTrue(visible.size.width > 0.0)
test.expectTrue(visible.size.height > 0.0)
// `visibleFrame` is the frame minus the menu bar / Dock, so
// `visibleFrame.size.height <= frame.size.height`.
test.expectTrue(visible.size.height <= frame.size.height)

// Screens enumeration has at least the main screen.
let all = NSScreen.screens()
test.expectTrue(all.count >= 1)

0
