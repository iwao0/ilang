// `loop { break <aliased var> }` retains the value past the
// surrounding scope's release (98cd0db). Without the fix the JIT
// hands the loop's slot a borrowed pointer the binding is about
// to drop and `r[0]` reads freed memory.
use test

fn run(): i64[] {
    let arr: i64[] = [1, 2, 3]
    loop { break arr }
}

let total: i64 = 0
for _ in 0..200 {
    let r = run()
    test.expect(r[0], 1)
    test.expect(r[2], 3)
}
test.expectBool(true, true)
