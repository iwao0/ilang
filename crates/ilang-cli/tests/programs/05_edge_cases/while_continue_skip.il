// `continue` inside a `while` loop jumps to the condition check,
// skipping the rest of the body. The basics fixtures only show
// continue inside `for-in`; the while-specific path (where
// programmer-written `i += 1` is the increment, not implicit) has
// a classic pitfall — increment after the continue is missed,
// causing infinite loop. This fixture pins down the correct shape.
use std.test

let i = 0
let total = 0
while i < 10 {
    i += 1                       // increment FIRST so continue is safe
    if i % 2 == 0 { continue }
    total += i
}
test.expect(total, 25)           // 1+3+5+7+9
test.expect(i, 10)

// continue + break in same loop.
let j = 0
let s = 0
while j < 100 {
    j += 1
    if j == 50 { break }
    if j % 10 != 0 { continue }
    s += j
}
test.expect(s, 100)              // 10+20+30+40
test.expect(j, 50)
