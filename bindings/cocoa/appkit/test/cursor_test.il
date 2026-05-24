/// NSCursor — static accessors return the standard system
/// cursor instances. We don't push / set them (would change
/// the active cursor for the test host).

use std.test
use appkit { NSCursor, sharedApplication }

// Force AppKit to load (NSApplicationLoad inside) before
// asking for a cursor. Without it the framework's class
// initialiser hasn't run on a non-GUI process.
let _ = sharedApplication()

// `arrowCursor` is the universally-defined system cursor.
let arrow = NSCursor.arrow()
test.expectFalse(arrow.handle == 0)

// The standard cursors are singletons; two calls return the
// same instance handle.
test.expect(NSCursor.arrow().handle, arrow.handle)

0
