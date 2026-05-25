/// NSSortDescriptor + NSArray.sortedArrayUsingDescriptors.

use std.test as test
use foundation { NSNumber, NSMutableArray, NSSortDescriptor,
                 nsstr, ilstr }

// Sort numbers ascending — keyPath `self` works for raw values.
let xs = NSMutableArray.alloc().init()
xs.addObject(NSNumber.fromI64(3))
xs.addObject(NSNumber.fromI64(1))
xs.addObject(NSNumber.fromI64(4))
xs.addObject(NSNumber.fromI64(1))
xs.addObject(NSNumber.fromI64(5))

let asc = NSSortDescriptor.sortDescriptorWithKey(nsstr("self"), true)
let desc = NSSortDescriptor.sortDescriptorWithKey(nsstr("self"), false)

let descs = NSMutableArray.alloc().init()
descs.addObject(asc)
let sortedAsc = xs.sortedArrayUsingDescriptors(descs)
test.expect(sortedAsc.count, 5)
let firstAsc = NSNumber.__wrap_handle(sortedAsc.objectAtIndex(0).handle)
test.expect(firstAsc.i64Value, 1)
let lastAsc = NSNumber.__wrap_handle(sortedAsc.objectAtIndex(4).handle)
test.expect(lastAsc.i64Value, 5)

// Descending via reversedSortDescriptor.
let rev = NSMutableArray.alloc().init()
rev.addObject(asc.reversedSortDescriptor)
let sortedDesc = xs.sortedArrayUsingDescriptors(rev)
let firstDesc = NSNumber.__wrap_handle(sortedDesc.objectAtIndex(0).handle)
test.expect(firstDesc.i64Value, 5)

// Direct descending descriptor.
let descSet = NSMutableArray.alloc().init()
descSet.addObject(desc)
let sortedDirect = xs.sortedArrayUsingDescriptors(descSet)
let firstDirect = NSNumber.__wrap_handle(sortedDirect.objectAtIndex(0).handle)
test.expect(firstDirect.i64Value, 5)

// Descriptor properties.
test.expectStr(ilstr(asc.key), "self")
test.expectTrue(asc.ascending)
test.expectFalse(desc.ascending)

0
