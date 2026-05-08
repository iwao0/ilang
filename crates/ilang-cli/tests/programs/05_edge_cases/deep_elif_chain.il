// `if/elif/.../else` chain with many arms. The parser folds these
// into a right-leaning `If { else_branch: If { else_branch: ... } }`
// tree; lowering must traverse it without exploding compile time
// AND, more importantly, type-check the whole chain producing one
// unified type (here, `string`).
use test

fn name(n: i64): string {
    if n == 0 { "zero" }
    elif n == 1 { "one" }
    elif n == 2 { "two" }
    elif n == 3 { "three" }
    elif n == 4 { "four" }
    elif n == 5 { "five" }
    elif n == 6 { "six" }
    elif n == 7 { "seven" }
    elif n == 8 { "eight" }
    elif n == 9 { "nine" }
    else { "many" }
}

test.expectStr(name(0), "zero")
test.expectStr(name(1), "one")
test.expectStr(name(5), "five")
test.expectStr(name(9), "nine")
test.expectStr(name(10), "many")
test.expectStr(name(-1), "many")
test.expectStr(name(100), "many")

// Long while with multiple early-return paths.
fn search(xs: i64[], target: i64): i64 {
    let i = 0
    while i < xs.length {
        if xs[i] == target { return i }
        if xs[i] < 0 { return -1 }       // sentinel
        i += 1
    }
    return -1
}
test.expect(search([10, 20, 30, 40], 30), 2)
test.expect(search([10, 20, -5, 40], 40), -1)   // hit -5 first
test.expect(search([], 0), -1)
test.expect(search([5], 5), 0)
test.expect(search([5], 99), -1)
