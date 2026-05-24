// libc is present everywhere — declaring it `@optional` should
// produce the same behaviour as a regular `@lib("c") fn`,
// with `os.libLoaded("c")` reporting true.
use os
use test

@extern(C) {
    @lib("c") @optional fn abs(x: i32): i32

    // `@lib` declarations only resolve through the extern-C codegen
    // path; wrap the call so ordinary script code can drive it.
    pub fn callAbs(x: i32): i32 { abs(x) }
}

test.expectTrue(os.libLoaded("c"))

let result = if os.libLoaded("c") {
    callAbs(-9)
} else {
    -1 as i32
}
test.expect(result as i64, 9)
