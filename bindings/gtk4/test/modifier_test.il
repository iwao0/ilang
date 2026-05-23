/// gtk — `GdkModifierType` flag bits as they appear in
/// `<gdk/gdkenums.h>`. The `state` arg of every key / button signal
/// is OR'd from these, so the meta bit in particular has to stay
/// at 0x10000000 for libs/gui's `metaKey` flag to fire correctly.

use test
use gtk { GdkModifierType }

test.expect(GdkModifierType.shift_mask    as i64, 0x00000001)
test.expect(GdkModifierType.lock_mask     as i64, 0x00000002)
test.expect(GdkModifierType.control_mask  as i64, 0x00000004)
test.expect(GdkModifierType.alt_mask      as i64, 0x00000008)
test.expect(GdkModifierType.button1_mask  as i64, 0x00000100)
test.expect(GdkModifierType.button2_mask  as i64, 0x00000200)
test.expect(GdkModifierType.button3_mask  as i64, 0x00000400)
test.expect(GdkModifierType.button4_mask  as i64, 0x00000800)
test.expect(GdkModifierType.button5_mask  as i64, 0x00001000)
test.expect(GdkModifierType.meta_mask     as i64, 0x10000000)
