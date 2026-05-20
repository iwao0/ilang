/// NSSet / NSMutableSet / NSCountedSet / NSIndexSet.

use test
use foundation { NSMutableArray, NSMutableSet, NSCountedSet, NSIndexSet,
                 nsstr, makeRange }

// NSMutableSet — add / remove / contains.
let s = NSMutableSet.set()
test.expect(s.count, 0)
s.addObject(nsstr("apple"))
s.addObject(nsstr("banana"))
s.addObject(nsstr("apple"))  // dup — set ignores
test.expect(s.count, 2)
test.expectTrue(s.containsObject(nsstr("apple")))
test.expectFalse(s.containsObject(nsstr("cherry")))

s.removeObject(nsstr("apple"))
test.expect(s.count, 1)
test.expectFalse(s.containsObject(nsstr("apple")))

// Set algebra.
let a = NSMutableSet.set()
a.addObject(nsstr("x"))
a.addObject(nsstr("y"))
let b = NSMutableSet.set()
b.addObject(nsstr("y"))
b.addObject(nsstr("z"))

let u = NSMutableSet.set()
u.addObject(nsstr("x"))
u.addObject(nsstr("y"))
u.unionSet(b)
test.expect(u.count, 3)

let inter = NSMutableSet.set()
inter.addObject(nsstr("x"))
inter.addObject(nsstr("y"))
inter.intersectSet(b)
test.expect(inter.count, 1)  // only "y"
test.expectTrue(inter.containsObject(nsstr("y")))

// NSCountedSet — multiplicities.
let arr = NSMutableArray.alloc().init()
arr.addObject(nsstr("a"))
arr.addObject(nsstr("a"))
arr.addObject(nsstr("a"))
arr.addObject(nsstr("b"))
let cs = NSCountedSet.alloc().initWithArray(arr)
test.expect(cs.count, 2)  // distinct count
test.expect(cs.countForObject(nsstr("a")), 3)
test.expect(cs.countForObject(nsstr("b")), 1)
test.expect(cs.countForObject(nsstr("z")), 0)

// NSIndexSet basics.
let ix = NSIndexSet.indexSetWithIndexesInRange(makeRange(5 as u64, 10 as u64))
test.expect(ix.count, 10)

let single = NSIndexSet.indexSetWithIndex(42 as u64)
test.expect(single.count, 1)

// NSMutableSet.removeAllObjects.
let bigSet = NSMutableSet.set()
bigSet.addObject(nsstr("p"))
bigSet.addObject(nsstr("q"))
bigSet.addObject(nsstr("r"))
test.expect(bigSet.count, 3)
bigSet.removeAllObjects()
test.expect(bigSet.count, 0)

// minusSet.
let ma = NSMutableSet.set()
ma.addObject(nsstr("1"))
ma.addObject(nsstr("2"))
ma.addObject(nsstr("3"))
let mb = NSMutableSet.set()
mb.addObject(nsstr("2"))
ma.minusSet(mb)
test.expect(ma.count, 2)
test.expectFalse(ma.containsObject(nsstr("2")))

// isSubsetOfSet / intersectsSet.
let sub = NSMutableSet.set()
sub.addObject(nsstr("1"))
sub.addObject(nsstr("3"))
test.expectTrue(sub.isSubsetOfSet(ma))
let disjoint = NSMutableSet.set()
disjoint.addObject(nsstr("z"))
test.expectFalse(disjoint.intersectsSet(ma))
test.expectTrue(sub.intersectsSet(ma))

0
