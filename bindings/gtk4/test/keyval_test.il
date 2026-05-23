/// gtk — `Keyval` discriminants must match the GDK keysym table
/// (`<gdk/gdkkeysyms.h>`). libs/gui's Linux backend maps these to
/// W3C `KeyboardEvent.code` strings, so a drift between the
/// binding and the SDK headers would silently mis-route keystrokes.

use test
use gtk { Keyval }

test.expect(Keyval.unknown    as i64, 0x0000)
test.expect(Keyval.keyA       as i64, 0x0061)
test.expect(Keyval.keyN       as i64, 0x006e)
test.expect(Keyval.keyZ       as i64, 0x007a)
test.expect(Keyval.digit0     as i64, 0x0030)
test.expect(Keyval.digit9     as i64, 0x0039)
test.expect(Keyval.space      as i64, 0x0020)
test.expect(Keyval.enter      as i64, 0xff0d)
test.expect(Keyval.escape     as i64, 0xff1b)
test.expect(Keyval.backspace  as i64, 0xff08)
test.expect(Keyval.tab        as i64, 0xff09)
test.expect(Keyval.arrowLeft  as i64, 0xff51)
test.expect(Keyval.arrowUp    as i64, 0xff52)
test.expect(Keyval.arrowRight as i64, 0xff53)
test.expect(Keyval.arrowDown  as i64, 0xff54)
