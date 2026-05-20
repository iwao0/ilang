/// NSNotificationCenter — register a block observer, post
/// synchronously (NotificationCenter delivers on the posting
/// thread when queue is nil), assert side effects.

use test
use foundation { NSNotificationCenter, NSNumber, NSObject, nil,
                 nsstr, ilstr }

let center = NSNotificationCenter.defaultCenter()
let counter = NSNumber.fromI64(0)
let countRef: NSNumber[] = [counter]

let token = center.addObserver(
    nsstr("ilang.test.event"), nil, nil,
    fn(raw: i64) {
        // The notification handle arrives raw; bump our counter
        // array by replacing the element.
        let prev = countRef[0].i64Value
        countRef[0] = NSNumber.fromI64(prev + 1)
    }
)

center.postNotificationName(nsstr("ilang.test.event"), nil)
center.postNotificationName(nsstr("ilang.test.event"), nil)
center.postNotificationName(nsstr("ilang.test.event"), nil)
test.expect(countRef[0].i64Value, 3)

// Unrelated name doesn't fire our block.
center.postNotificationName(nsstr("ilang.test.other"), nil)
test.expect(countRef[0].i64Value, 3)

center.removeObserver(token)

// After remove, our block stops firing.
center.postNotificationName(nsstr("ilang.test.event"), nil)
test.expect(countRef[0].i64Value, 3)

0
