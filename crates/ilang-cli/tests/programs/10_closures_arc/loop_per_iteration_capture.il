// A `let` declared inside a loop body is a FRESH binding each iteration,
// so a closure that captures it snapshots that iteration's value — the
// three closures return 0, 1, 2, not all 2. This is the opposite of
// `closures_array_share_cell.il` (two closures sharing ONE cell): here
// each turn of the loop allocates its own cell. Pins the classic
// loop-variable-capture distinction.
//
// expect: ok
use std.test as test

fn build(): (fn(): i64)[] {
    let fns: (fn(): i64)[] = []
    let i = 0
    while i < 3 {
        let x = i                       // fresh cell per iteration
        fns.push(fn(): i64 { x })
        i = i + 1
    }
    fns
}

let fns = build()
test.expect(fns[0](), 0)
test.expect(fns[1](), 1)
test.expect(fns[2](), 2)
console.log("ok")
