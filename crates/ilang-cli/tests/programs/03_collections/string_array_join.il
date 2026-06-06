// `string[].join(sep)` — concatenate every element with the given
// separator. Per syntax.md §8 "Built-in array methods".
use std.test as test

let xs: string[] = ["a", "b", "c"]
test.expectStr(xs.join(","), "a,b,c")
test.expectStr(xs.join(""), "abc")
test.expectStr(xs.join(" / "), "a / b / c")

// Empty / single-element edge cases.
let empty: string[] = []
test.expectStr(empty.join(","), "")
test.expectStr(empty.join(""), "")

let one: string[] = ["only"]
test.expectStr(one.join(","), "only")
test.expectStr(one.join("--"), "only")

// Separator with multibyte characters round-trips intact.
let cs: string[] = ["朝", "昼", "夜"]
test.expectStr(cs.join("·"), "朝·昼·夜")

// join doesn't mutate the source array.
let src: string[] = ["x", "y"]
let _r = src.join(",")
test.expect(src.length, 2)
test.expectStr(src[0], "x")
test.expectStr(src[1], "y")
