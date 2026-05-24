/// NSPointerArray — strong-object variant + add/remove/replace.

use std.test
use foundation { NSPointerArray, NSString, nsstr, ilstr }

let pa = NSPointerArray.strongObjectsPointerArray()
test.expect(pa.count as i64, 0)

pa.addPointer(nsstr("a"))
pa.addPointer(nsstr("b"))
pa.addPointer(nsstr("c"))
test.expect(pa.count as i64, 3)

let mid = NSString.__wrap_handle(pa.pointerAtIndex(1 as u64).handle)
test.expectStr(ilstr(mid), "b")

// insert at index.
pa.insertPointer(nsstr("Z"), 0 as u64)
test.expect(pa.count as i64, 4)
let first = NSString.__wrap_handle(pa.pointerAtIndex(0 as u64).handle)
test.expectStr(ilstr(first), "Z")

// replace.
pa.replacePointerAtIndex(0 as u64, nsstr("X"))
let firstAfter = NSString.__wrap_handle(pa.pointerAtIndex(0 as u64).handle)
test.expectStr(ilstr(firstAfter), "X")

// remove.
pa.removePointerAtIndex(0 as u64)
test.expect(pa.count as i64, 3)

// allObjects round-trips.
let all = pa.allObjects
test.expect(all.count, 3)

0
