// `@handle struct` — opaque pointer-sized handle types for Win32 /
// libnotify / Wayland-style SDK pointer typedefs. Per syntax.md §13b
// "@handle struct Name {}".
use std.test as test

@extern(C, "fake_lib") {
    @handle pub struct MyHandle {}
    @handle pub struct OtherHandle {}
}

// The null handle.
let null_h: MyHandle = 0 as MyHandle
test.expect(null_h as i64, 0)

// Round-trip through i64.
let h: MyHandle = 0x1234 as MyHandle
test.expect(h as i64, 0x1234)
let raw: i64 = h as i64
let back: MyHandle = raw as MyHandle
test.expect(back as i64, 0x1234)

// Cross-handle cast: nominally distinct, but the SDK pattern of
// passing one handle where another is expected via `as` is allowed.
let o: OtherHandle = h as OtherHandle
test.expect(o as i64, 0x1234)
let h_again: MyHandle = o as MyHandle
test.expect(h_again as i64, 0x1234)
