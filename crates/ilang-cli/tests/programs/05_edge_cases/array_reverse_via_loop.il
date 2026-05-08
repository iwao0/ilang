// Manual reverse via a `while` loop building a fresh array. There's
// no built-in `reverse` method, so this is the canonical pattern;
// we exercise it across primitive and heap-typed elements, and the
// empty/single-element edges.
use test

fn reverse_i64(xs: i64[]): i64[] {
    let result: i64[] = []
    let i = xs.length - 1
    while i >= 0 {
        result.push(xs[i])
        i -= 1
    }
    result
}

let xs: i64[] = [3, 1, 4, 1, 5, 9, 2, 6]
let rs = reverse_i64(xs)
test.expect(rs.length, 8)
test.expect(rs[0], 6)
test.expect(rs[7], 3)
// Source not mutated.
test.expect(xs[0], 3)
test.expect(xs.length, 8)

// Empty.
test.expect(reverse_i64([]).length, 0)

// Single element.
let one = reverse_i64([42])
test.expect(one.length, 1)
test.expect(one[0], 42)

// Heap-typed element (string).
fn reverse_str(xs: string[]): string[] {
    let result: string[] = []
    let i = xs.length - 1
    while i >= 0 {
        result.push(xs[i])
        i -= 1
    }
    result
}
let ss: string[] = ["a", "bb", "ccc"]
let rss = reverse_str(ss)
test.expectStr(rss[0], "ccc")
test.expectStr(rss[1], "bb")
test.expectStr(rss[2], "a")
test.expect(rss.length, 3)
