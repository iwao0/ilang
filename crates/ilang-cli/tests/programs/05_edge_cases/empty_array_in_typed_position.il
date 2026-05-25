// Empty array literal `[]` flows into any typed slot whose annotation
// supplies the element type — fn args, tuple elements, `some(...)`
// inner. Pre-fix only the `let x: T[] = []` path threaded the elem
// type into the JIT; the others errored with "JIT array literal
// must have at least one element".
use std.test as test
fn count(xs: i64[]): i64 { xs.length }
test.expect(count([]), 0)

let t: (i64[], string) = ([], "hi")
test.expect(t[0].length, 0)
test.expectStr(t[1], "hi")

let o: i64[]? = some([])
test.expect(o.unwrap().length, 0)

// Sanity: same shapes with non-empty literals still work.
test.expect(count([1, 2, 3]), 3)
let t2: (i64[], string) = ([1, 2], "two")
test.expect(t2[0].length, 2)
let o2: i64[]? = some([7])
test.expect(o2.unwrap()[0], 7)
