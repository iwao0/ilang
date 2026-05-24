// Body-local `let y: T[]` referencing the generic fn's own type
// param (fba3d1b — `current_type_params` fallback in
// validate_type and rewrite of let annotations).
use std.test

fn singleton<T>(x: T): T[] {
    let arr: T[] = [x]
    arr
}

fn first_via_let<T>(arr: T[]): T {
    let n: i64 = arr.length
    let _ = n
    let head: T = arr[0]
    head
}

let s = singleton(42)
test.expect(s.length, 1)
test.expect(s[0], 42)

test.expect(first_via_let([10, 20, 30]), 10)
test.expectStr(first_via_let(["a", "b"]), "a")
