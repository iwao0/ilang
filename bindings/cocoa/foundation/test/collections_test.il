// NSArray / NSMutableArray / NSDictionary / NSMutableDictionary.

use test
use foundation { NSObject, NSString, NSArray, NSMutableArray,
            NSDictionary, NSMutableDictionary, nsstr, ilstr }

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

0
