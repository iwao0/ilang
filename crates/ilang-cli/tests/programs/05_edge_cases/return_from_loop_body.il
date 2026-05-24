// `return` inside a `for-in` / `while` body must unwind the loop
// AND the enclosing function. A regression that only `break`s the
// loop would leave the function falling through to the post-loop
// expression — silent wrong-result case.
use std.test

fn find_first_even(xs: i64[]): i64? {
    for x in xs {
        if x % 2 == 0 { return some(x) }
    }
    none
}
let r1 = find_first_even([1, 3, 5, 8, 10])
test.expectBool(r1.isSome, true)
test.expect(r1.unwrap(), 8)

let r2 = find_first_even([1, 3, 5])
test.expectBool(r2.isNone, true)

let r3 = find_first_even([])
test.expectBool(r3.isNone, true)

fn count_until(xs: i64[], stop: i64): i64 {
    let i = 0
    while i < xs.length {
        if xs[i] == stop { return i }
        i += 1
    }
    return -1
}
test.expect(count_until([10, 20, 30, 40], 30), 2)
test.expect(count_until([10, 20], 99), -1)
test.expect(count_until([], 0), -1)

// Return from nested loops — must unwind BOTH.
fn first_pair_summing(xs: i64[], target: i64): i64 {
    for a in xs {
        for b in xs {
            if a + b == target { return a * 100 + b }
        }
    }
    return -1
}
test.expect(first_pair_summing([1, 2, 3, 4], 5), 104)   // outer a=1 first hits at b=4
test.expect(first_pair_summing([1, 2, 3], 100), -1)
