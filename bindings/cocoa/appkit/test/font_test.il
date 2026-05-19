// NSFont — pull a few system fonts at known sizes and verify
// pointSize / familyName round-trip.

use test
use appkit { NSFont }
use foundation { nsstr, ilstr }

let f = NSFont.systemFontOfSize(13.0)
test.expectF64(f.pointSize, 13.0)
test.expectTrue(f.familyName.length > 0)

let bold = NSFont.boldSystemFontOfSize(17.0)
test.expectF64(bold.pointSize, 17.0)

let helv = NSFont.fontWithName(nsstr("Helvetica"), 24.0)
// `fontWithName:` returns nil if the family isn't installed.
// Helvetica ships with every macOS release; assume present.
test.expectFalse(helv.handle == 0)
test.expectF64(helv.pointSize, 24.0)
test.expectStr(ilstr(helv.familyName), "Helvetica")

// Unknown family returns a nil-handle wrapper.
let missing = NSFont.fontWithName(nsstr("__ilang_no_such_font__"), 12.0)
test.expectTrue(missing.handle == 0)

0
