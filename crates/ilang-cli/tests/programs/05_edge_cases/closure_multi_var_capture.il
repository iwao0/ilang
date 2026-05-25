// Closure with multiple mutable captures of different shapes —
// a primitive counter and a heap-typed string accumulator. Both
// must share the lifetime of the closure value: their cells live
// until the closure drops, and the closure-drop path has to release
// the heap value before freeing the cell.
use std.test as test
fn make(): fn(i64): string {
    let count = 0
    let log = ""
    fn(x: i64): string {
        count = count + 1
        let tag = if x > 0 { "+" } else { "-" }
        log = log + "[" + tag + "]"
        log
    }
}

let f = make()
test.expectStr(f(1), "[+]")
test.expectStr(f(-2), "[+][-]")
test.expectStr(f(3), "[+][-][+]")
test.expectStr(f(0), "[+][-][+][-]")

// Second instance — its own cells.
let f2 = make()
test.expectStr(f2(5), "[+]")
test.expectStr(f(7), "[+][-][+][-][+]")    // f1 untouched
