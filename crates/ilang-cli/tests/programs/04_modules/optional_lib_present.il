// libc is present everywhere — declaring it `optional` should
// produce the same behaviour as a regular `@extern("c") fn`,
// with `os.libLoaded("c")` reporting true.
// interp: skip
use os
use test

@extern("c", optional) fn abs(x: i32): i32

test.expectTrue(os.libLoaded("c"))

let result = if os.libLoaded("c") {
    abs(-9)
} else {
    -1 as i32
}
test.expect(result as i64, 9)
