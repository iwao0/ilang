// Array literal forms: dynamic, fixed-length, trailing comma, nested.
// Plus index read/write, .length, and the empty-array annotation rule.
use std.test

// Dynamic vs fixed-length declaration syntax.
let xs: i64[] = [10, 20, 30]
let ys: i64[3] = [1, 2, 3]
test.expect(xs.length, 3)
test.expect(ys.length, 3)

// Trailing comma OK.
let trailing: i64[] = [1, 2, 3,]
test.expect(trailing.length, 3)
test.expect(trailing[2], 3)

// Index read + write.
test.expect(xs[0], 10)
xs[0] = 100
test.expect(xs[0], 100)

// Empty array literal needs an annotation.
let empty: i64[] = []
test.expect(empty.length, 0)
empty.push(7)
test.expect(empty.length, 1)
test.expect(empty[0], 7)

// Nested arrays.
let grid: i64[][] = [[1, 2], [3, 4], [5, 6]]
test.expect(grid.length, 3)
test.expect(grid[1].length, 2)
test.expect(grid[1][0], 3)
test.expect(grid[2][1], 6)
