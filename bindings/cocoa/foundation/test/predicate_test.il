/// NSPredicate — constant predicates + filter an NSArray.

use std.test
use foundation { NSPredicate, NSCompoundPredicate,
                 NSArray, NSMutableArray, NSNumber, nsstr }

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

// NSCompoundPredicate — AND of "SELF > 5" + "SELF < 13".
let pHi = NSPredicate.predicateWithFormat(
    nsstr("SELF > 5"), NSMutableArray.array()
)
let pLo = NSPredicate.predicateWithFormat(
    nsstr("SELF < 13"), NSMutableArray.array()
)
let pair = NSMutableArray.alloc().init()
pair.addObject(pHi)
pair.addObject(pLo)
let pAnd = NSCompoundPredicate.andPredicate(pair)
test.expectTrue(pAnd.evaluateWithObject(NSNumber.fromI64(7)))
test.expectFalse(pAnd.evaluateWithObject(NSNumber.fromI64(3)))
test.expectFalse(pAnd.evaluateWithObject(NSNumber.fromI64(15)))

// OR — "SELF < 3" OR "SELF > 13".
let pLow = NSPredicate.predicateWithFormat(
    nsstr("SELF < 3"), NSMutableArray.array()
)
let pHigh = NSPredicate.predicateWithFormat(
    nsstr("SELF > 13"), NSMutableArray.array()
)
let pair2 = NSMutableArray.alloc().init()
pair2.addObject(pLow)
pair2.addObject(pHigh)
let pOr = NSCompoundPredicate.orPredicate(pair2)
test.expectTrue(pOr.evaluateWithObject(NSNumber.fromI64(1)))
test.expectTrue(pOr.evaluateWithObject(NSNumber.fromI64(15)))
test.expectFalse(pOr.evaluateWithObject(NSNumber.fromI64(7)))

// NOT.
let pNot = NSCompoundPredicate.notPredicate(pHi)
test.expectTrue(pNot.evaluateWithObject(NSNumber.fromI64(1)))
test.expectFalse(pNot.evaluateWithObject(NSNumber.fromI64(10)))

0
