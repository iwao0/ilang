// `let alias = arr` rebinds without copying — both names refer to the
// same heap allocation. Mutations through one are visible through the
// other. (The class-aliasing case is in 02_classes/arc_aliasing.il;
// this is the array variant, which the existing fixtures don't cover.)
use std.test

let arr: i64[] = [1, 2, 3]
let alias = arr

alias[1] = 1234
test.expect(arr[1], 1234)
test.expect(alias[1], 1234)

arr.push(99)
test.expect(alias.length, 4)
test.expect(alias[3], 99)

alias.push(100)
test.expect(arr.length, 5)
test.expect(arr[4], 100)

// Pop via one is observable via the other.
let popped = alias.pop()
test.expectBool(popped.isSome, true)
test.expect(popped.unwrap(), 100)
test.expect(arr.length, 4)
