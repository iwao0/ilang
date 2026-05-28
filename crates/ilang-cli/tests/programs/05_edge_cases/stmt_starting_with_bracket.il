// A statement that starts with an array literal `[...]` is its own
// statement, not a subscript glued onto the previous expression. The
// postfix index only binds when `[` is on the same line as the
// expression it indexes (same newline rule as a postfix call).
use std.test as test

let sink: i64[] = []
[1, 2, 3].forEach(fn(x: i64) { sink.push(x) })
test.expect(sink.length, 3)
test.expect(sink[0], 1)
test.expect(sink[2], 3)

// A bare array-literal statement after another expression statement
// (the preceding `test.expect(..)` is a call, not a let).
let xs: i64[] = [10, 20, 30]
test.expect(xs.length, 3)
[7, 8].forEach(fn(x: i64) { sink.push(x) })
test.expect(sink.length, 5)

// Same-line indexing — including chained `m[i][j]` — is unchanged.
test.expect(xs[1], 20)
let m: i64[][] = [[1, 2], [3, 4]]
test.expect(m[1][0], 3)
