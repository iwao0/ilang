/// gtk — assorted enum / bitfield discriminants. Pure-value checks
/// only; no DLL calls so this fixture runs everywhere.

use std.test as test
use gtk { GApplicationFlags, GtkOrientation, GtkAlign }

// `GApplicationFlags.flags_none` is the new spelling of the C
// `G_APPLICATION_DEFAULT_FLAGS` macro that the hand-rolled binding
// used to expose.
test.expect(GApplicationFlags.flags_none as i64, 0)

test.expect(GtkOrientation.horizontal as i64, 0)
test.expect(GtkOrientation.vertical   as i64, 1)

test.expect(GtkAlign.fill   as i64, 0)
test.expect(GtkAlign.start  as i64, 1)
test.expect(GtkAlign.end    as i64, 2)
test.expect(GtkAlign.center as i64, 3)
