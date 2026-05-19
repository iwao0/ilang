// NSPredicate — constant predicates + filter an NSArray.

use test
use foundation { NSPredicate, NSArray, NSMutableArray, NSNumber, nsstr }

// Constant predicates evaluate ignoring the object.
let pTrue = NSPredicate.predicateWithValue(true)
let pFalse = NSPredicate.predicateWithValue(false)
let dummy = NSNumber.fromI64(0)
test.expectTrue(pTrue.evaluateWithObject(dummy))
test.expectFalse(pFalse.evaluateWithObject(dummy))

// Format-string predicate over NSNumber: SELF > 10.
let p = NSPredicate.predicateWithFormat(
    nsstr("SELF > 10"), NSMutableArray.array()
)
test.expectTrue(p.evaluateWithObject(NSNumber.fromI64(11)))
test.expectFalse(p.evaluateWithObject(NSNumber.fromI64(5)))
test.expectFalse(p.evaluateWithObject(NSNumber.fromI64(10)))

// filteredArrayUsingPredicate — keep elements satisfying the
// predicate.
let xs = NSMutableArray.alloc().init()
xs.addObject(NSNumber.fromI64(3))
xs.addObject(NSNumber.fromI64(7))
xs.addObject(NSNumber.fromI64(11))
xs.addObject(NSNumber.fromI64(15))
let kept = xs.filteredArrayUsingPredicate(p)
test.expect(kept.count, 2)

0
