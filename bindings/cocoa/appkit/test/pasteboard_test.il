// NSPasteboard — copy + paste round-trip on the general
// clipboard. This DOES touch the user's actual clipboard.

use test
use appkit { NSPasteboard, PasteboardTypeString,
             pasteboardCopy, pasteboardPaste }
use foundation { nsstr, ilstr }

let pb = NSPasteboard.generalPasteboard()
test.expectFalse(pb.handle == 0)

// Copy + paste a unique sentinel so we don't accidentally
// match leftover clipboard data from before the test ran.
let marker = "ilang appkit pasteboard test 4e0a3"
pasteboardCopy(marker)
let pasted = pasteboardPaste()
test.expectTrue(pasted.isSome)
test.expectStr(pasted.unwrap(), marker)

// Direct setString/stringForType round-trip.
pb.clearContents()
pb.setString(nsstr("hello"), nsstr(PasteboardTypeString))
test.expectStr(ilstr(pb.stringForType(nsstr(PasteboardTypeString))), "hello")

0
