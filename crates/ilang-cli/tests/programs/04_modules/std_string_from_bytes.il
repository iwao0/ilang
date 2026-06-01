// `std.ffi.stringFromBytes(p, n)` copies `n` bytes from a raw pointer
// into a fresh owned ilang `string`. Length-counted companion to
// `stringFromCstr` — used by binding shims that surface
// `(const char*, size_t)` payloads like `WGPUStringView` whose length
// is a real count, not a NUL terminator.

use std.test as test
use std.ffi { stringFromBytes }

@extern(C) {
    fn fromBytes(buf: u8[], n: i64): string {
        stringFromBytes(buf, n)
    }
    fn fromNull(n: i64): string {
        stringFromBytes(0 as *const void, n)
    }
}

// ASCII payload, full length — "hello".
let hello: u8[] = [
    104 as u8, 101 as u8, 108 as u8, 108 as u8, 111 as u8,
]
test.expectStr(fromBytes(hello, 5), "hello")

// `n` shorter than the buffer truncates without scanning for a NUL.
test.expectStr(fromBytes(hello, 3), "hel")

// Zero length → empty string, regardless of buffer contents.
test.expectStr(fromBytes(hello, 0), "")

// UTF-8 multi-byte payload — "日本" (e6 97 a5  e6 9c ac).
let nihon: u8[] = [
    0xE6 as u8, 0x97 as u8, 0xA5 as u8,
    0xE6 as u8, 0x9C as u8, 0xAC as u8,
]
test.expectStr(fromBytes(nihon, 6), "日本")

// Null pointer guard — `p == 0` returns `""` no matter what `n` is.
test.expectStr(fromNull(8), "")

// Negative length → empty (same guard).
test.expectStr(fromBytes(hello, -1 as i64), "")
