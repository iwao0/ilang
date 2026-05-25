/// NSWorkspace — read-only shared singleton + URL parse safety.
/// We don't call openURL / openFile — those would launch external
/// apps.

use std.test as test
use appkit { NSWorkspace }

let ws = NSWorkspace.sharedWorkspace()
test.expectFalse(ws.handle == 0)

// Two calls return the same singleton.
let ws2 = NSWorkspace.sharedWorkspace()
test.expectTrue(ws.handle == ws2.handle)

0
