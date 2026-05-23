/// gtk — `GdkModifierType` flag bits as they appear in
/// `<gdk/gdkenums.h>`. The `state` arg of every key / button signal
/// is OR'd from these, so the meta bit in particular has to stay
/// at 0x10000000 for libs/gui's `metaKey` flag to fire correctly.

use test
use gtk { GdkModifierType }

test.expect(GdkModifierType.shift   as i64, 0x00000001)
test.expect(GdkModifierType.lock    as i64, 0x00000002)
test.expect(GdkModifierType.control as i64, 0x00000004)
test.expect(GdkModifierType.alt     as i64, 0x00000008)
test.expect(GdkModifierType.button1 as i64, 0x00000100)
test.expect(GdkModifierType.button2 as i64, 0x00000200)
test.expect(GdkModifierType.button3 as i64, 0x00000400)
test.expect(GdkModifierType.button4 as i64, 0x00000800)
test.expect(GdkModifierType.button5 as i64, 0x00001000)
test.expect(GdkModifierType.meta    as i64, 0x10000000)
