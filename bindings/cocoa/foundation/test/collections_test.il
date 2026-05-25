/// NSArray / NSMutableArray / NSDictionary / NSMutableDictionary.

use std.test as test
use foundation { NSObject, NSString, NSArray, NSMutableArray,
            NSDictionary, NSMutableDictionary, NSEnumerator,
            makeRange, nsstr, ilstr }

// NSMutableArray basics: alloc → init → addObject → count / objectAtIndex.
let arr = NSMutableArray.alloc().init()
test.expect(arr.count, 0)

arr.addObject(nsstr("a"))
arr.addObject(nsstr("b"))
arr.addObject(nsstr("c"))
test.expect(arr.count, 3)

let first = NSString.__wrap_handle(arr.objectAtIndex(0).handle)
test.expectStr(ilstr(first), "a")

// containsObject — NSString `isEqual:` makes the lookup work
// by value, not pointer identity.
test.expectTrue(arr.containsObject(nsstr("b")))
test.expectFalse(arr.containsObject(nsstr("zz")))

// componentsJoinedByString.
let joined = arr.componentsJoinedByString(nsstr("-"))
test.expectStr(ilstr(joined), "a-b-c")

// removeLastObject shrinks count.
arr.removeLastObject()
test.expect(arr.count, 2)

// arrayByAddingObject returns a fresh array, leaves receiver alone.
let extended = arr.arrayByAddingObject(nsstr("d"))
test.expect(extended.count, 3)
test.expect(arr.count, 2)

// isEqualToArray — value equality.
let copy = NSMutableArray.alloc().init()
copy.addObject(nsstr("a"))
copy.addObject(nsstr("b"))
test.expectTrue(arr.isEqualToArray(copy))

// NSMutableDictionary basics.
let dict = NSMutableDictionary.alloc().init()
test.expect(dict.count, 0)

dict.setObject(nsstr("v1"), nsstr("k1"))
dict.setObject(nsstr("v2"), nsstr("k2"))
test.expect(dict.count, 2)

let v1 = NSString.__wrap_handle(dict.objectForKey(nsstr("k1")).handle)
test.expectStr(ilstr(v1), "v1")

// removeObjectForKey shrinks.
dict.removeObjectForKey(nsstr("k1"))
test.expect(dict.count, 1)

// allKeys / allValues round-trip.
dict.setObject(nsstr("v3"), nsstr("k3"))
test.expect(dict.allKeys.count, 2)
test.expect(dict.allValues.count, 2)

// NSArray.indexOfObject — value lookup.
let arr2 = NSMutableArray.alloc().init()
arr2.addObject(nsstr("apple"))
arr2.addObject(nsstr("banana"))
arr2.addObject(nsstr("cherry"))
test.expect(arr2.indexOfObject(nsstr("banana")) as i64, 1)

// firstObject / lastObject.
let first2 = NSString.__wrap_handle(arr2.firstObject.handle)
test.expectStr(ilstr(first2), "apple")
let last2 = NSString.__wrap_handle(arr2.lastObject.handle)
test.expectStr(ilstr(last2), "cherry")

// subarrayWithRange.
let mid = arr2.subarrayWithRange(makeRange(1 as u64, 2 as u64))
test.expect(mid.count, 2)

// arrayByAddingObjectsFromArray.
let combined = arr2.arrayByAddingObjectsFromArray(arr2)
test.expect(combined.count, 6)

// removeObject by value.
arr2.removeObject(nsstr("banana"))
test.expect(arr2.count, 2)
test.expectFalse(arr2.containsObject(nsstr("banana")))

// insertObject at index.
arr2.insertObject(nsstr("ZZ"), 0 as i64)
test.expect(arr2.count, 3)
let firstAfter = NSString.__wrap_handle(arr2.objectAtIndex(0).handle)
test.expectStr(ilstr(firstAfter), "ZZ")

// removeObjectAtIndex.
arr2.removeObjectAtIndex(0)
test.expect(arr2.count, 2)

// replaceObjectAtIndex.
arr2.replaceObjectAtIndex(0, nsstr("X"))
let after = NSString.__wrap_handle(arr2.objectAtIndex(0).handle)
test.expectStr(ilstr(after), "X")

// removeAllObjects.
arr2.removeAllObjects()
test.expect(arr2.count, 0)

// NSEnumerator: walk an array with nextObject.
let walk = NSMutableArray.alloc().init()
walk.addObject(nsstr("a"))
walk.addObject(nsstr("b"))
walk.addObject(nsstr("c"))
let it = NSEnumerator.__wrap_handle(walk.objectEnumerator().handle)
let all = it.allObjects
test.expect(all.count, 3)

// nextObject increments on a fresh enumerator.
let it2 = NSEnumerator.__wrap_handle(walk.objectEnumerator().handle)
let n1 = NSString.__wrap_handle(it2.nextObject().handle)
test.expectStr(ilstr(n1), "a")
let n2 = NSString.__wrap_handle(it2.nextObject().handle)
test.expectStr(ilstr(n2), "b")

// reverseObjectEnumerator yields back-to-front.
let rit = NSEnumerator.__wrap_handle(walk.reverseObjectEnumerator().handle)
let r1 = NSString.__wrap_handle(rit.nextObject().handle)
test.expectStr(ilstr(r1), "c")

0

