// `match make()` on a fresh heap enum scrutinee releases the
// scrutinee at the merge so we don't leak (e3bb440).
use std.test as test
enum Box {
    holds: (i64[])
}

fn make(i: i64): Box {
    let arr: i64[] = [i, i + 1, i + 2]
    Box.holds(arr)
}

let total: i64 = 0
for i in 0..1000 {
    total = total + match make(i) {
        holds(x) { x[0] }
    }
}
test.expect(total, 999 * 1000 / 2)
