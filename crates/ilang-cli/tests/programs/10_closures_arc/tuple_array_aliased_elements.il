// Tuple / array literal elements that are aliased heap refs need
// retain at construction (ca137fc). Pre-fix JIT segfaulted /
// out-of-bounds-panicked on the second access.
use test

fn run(): i64 {
    let total: i64 = 0
    for _ in 0..200 {
        let a: i64[] = [1, 2, 3]
        let b: i64[] = [4, 5, 6]
        let arr: i64[][] = [a, b]
        let t: (i64[], i64) = (a, 99)
        let r = arr[0][0] + arr[1][2] + t[0][1] + t[1]
        test.expect(r, 1 + 6 + 2 + 99)
    }
    total
}

let _ = run()
test.expectBool(true, true)
