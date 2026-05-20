/// NSPasteboard — copy + paste round-trip on a private
/// `pasteboardWithUniqueName` instance, so the user's actual
/// system clipboard is left untouched.

use test
use appkit { NSPasteboard, PasteboardTypeString }
use foundation { nsstr, ilstr }

let pb = NSPasteboard.pasteboardWithUniqueName()
test.expectFalse(pb.handle == 0)

// Direct setString/stringForType round-trip on the private
// pasteboard.
pb.clearContents()
pb.setString(nsstr("hello"), nsstr(PasteboardTypeString))
test.expectStr(ilstr(pb.stringForType(nsstr(PasteboardTypeString))), "hello")

// A second clear + write should overwrite the prior contents.
let marker = "ilang appkit pasteboard test 4e0a3"
pb.clearContents()
pb.setString(nsstr(marker), nsstr(PasteboardTypeString))
test.expectStr(ilstr(pb.stringForType(nsstr(PasteboardTypeString))), marker)

0
